use rand::seq::SliceRandom;
use log::{info, debug, trace};
use std::io;
use std::sync::Mutex;
use std::net::SocketAddr;
use std::collections::HashMap;
use super::Service;
use crate::{Event, Block, BlockID};
use crate::server::Context;

pub struct BlockService {
  buffer: Mutex<Vec<u8>>,
  queue: Mutex<HashMap<SocketAddr, Block>>,
  future_queue: Mutex<HashMap<SocketAddr, Block>>,
  blocks: Mutex<Vec<Block>>,
}

impl BlockService {
  pub fn new() -> Self {
    BlockService{
      buffer: Mutex::new(Vec::new()),
      // TODO: exclusive to peers.
      queue: Mutex::new(HashMap::new()),
      future_queue: Mutex::new(HashMap::new()),
      blocks: Mutex::new(vec![Block::new(vec![])]),
    }
  }

  fn process_propose_block(&self, ctx: &Context, block: &Block, proposer: &SocketAddr) {
    if !block.verify() {
      // block is invalid thus we abort any operation.
      return;
    }

    {
      let blocks = self.blocks.lock().unwrap();
      let next = blocks.last().unwrap().next(vec![]);
      if next.get_seed() != block.get_seed() {
        let mut queue = self.future_queue.lock().unwrap();
        queue.insert(proposer.clone(), block.clone());
        // Wait for the current round to finish before processing future blocks.
        return;
      }
    }

    {
      // Add the block in the map of proposed blocks so far
      // for this round.
      if proposer != ctx.get_addr() {
        // ... but not a proposal from the server itself.
        trace!("{} adding a new block proposal {}", ctx.get_addr(), block.hash());

        let mut queue = self.queue.lock().unwrap();

        queue.insert(proposer.clone(), block.clone());
      } else {
        // The event went around the ring of peers.
        return;
      }
    }

    // Propagate the proposal.
    ctx.propagate(&Event::ProposeBlock(block.clone(), proposer.clone()));

    // Then we send the ack of the block.
    let id = block.hash();

    // Send the ACK to the proposer.
    let ack = Event::AckBlock(id.clone());

    ctx.send(&ack, &proposer);
  }

  fn process_ack_block(&self, ctx: &Context, id: &BlockID, from: &SocketAddr) {
    trace!("{} got ack for {} from {:?}", ctx.get_addr(), id, from);

    let index = ctx.get_peers().iter().position(|p| p == from).unwrap();
    let mut queue = self.queue.lock().unwrap();

    if let Some(block) = queue.get_mut(ctx.get_addr()) {
      if block.hash() == *id {
        block.incr_ack(index);

        trace!("{} Ack for {} with {}: {}", ctx.get_addr(), id, from, block.get_ack());
        if block.get_ack() == ctx.get_peers().len() {
          ctx.propagate(&Event::ValidateBlock(id.clone()));
        }
      }
    }
  }

  fn process_validate_block(&self, ctx: &Context, id: &BlockID) {
    {
      let blocks = self.blocks.lock().unwrap();
      for b in blocks.iter() {
        if b.hash() == *id {
          return;
        }
      }
    }

    let leader = self.get_leader(ctx.get_peers());
    let queue = self.queue.lock().unwrap();
    let mut blocks = self.blocks.lock().unwrap();

    debug!("{} got validation for {} with leader {}", ctx.get_addr(), id, leader);

    if let Some(block) = queue.get(leader) {
      let block = block.clone();
      if block.hash() == *id {
        ctx.propagate(&Event::ValidateBlock(id.clone()));

        info!("{} is announcing block {} from leader {}", ctx.get_addr(), id, leader);
        ctx.announce_block(&block);

        // Store the new block.
        blocks.push(block.clone());

        drop(queue);
        drop(blocks);

        // Empty the queue of future blocks.
        let items: Vec<(_, _)> = {
          // Lock needs to be released for the handler.
          let mut queue = self.future_queue.lock().unwrap();
          queue.drain().collect()
        };
        
        for (proposer, block) in items {
          self.process_propose_block(ctx, &block, &proposer);
        }

        self.future_queue.lock().unwrap().clear();

        self.retry_block(ctx, id);
      }
    }
  }

  fn process_create_block(&self, ctx: &Context, data: &Vec<u8>) {
    let blocks = self.blocks.lock().unwrap();
    let mut block = blocks.last().unwrap().next(data.clone());
    let index = ctx.get_peers().iter().position(|p| p == ctx.get_addr()).unwrap();
    block.incr_ack(index);

    let mut queue = self.queue.lock().unwrap();
    queue.insert(ctx.get_addr().clone(), block.clone());
    drop(queue);
    drop(blocks);

    trace!("{} asking for block {}", ctx.get_addr(), block.hash());
    let evt = Event::ProposeBlock(block, ctx.get_addr().clone());

    ctx.propagate(&evt);
  }

  fn get_leader<'a>(&self, peers: &'a Vec<SocketAddr>) -> &'a SocketAddr {
    let blocks = self.blocks.lock().unwrap();
    let block = blocks.last().unwrap();
    let mut rng = block.get_rng();

    return peers.choose(&mut rng).unwrap();
  }

  fn retry_block(&self, ctx: &Context, id: &BlockID) {
    debug!("{} is retrying block {}", ctx.get_addr(), id);

    let buffer = self.buffer.lock().unwrap();
    if buffer.len() > 0 {
      self.process_create_block(ctx, &buffer);
    }
  }
}

impl Service for BlockService {
  fn process_event(&self, ctx: &Context, evt: &Event, from: &SocketAddr) -> io::Result<()> {
    match evt {
      Event::ProposeBlock(block, prop) => self.process_propose_block(ctx, block, prop),
      Event::AckBlock(id) => self.process_ack_block(ctx, id, from),
      Event::ValidateBlock(id) => self.process_validate_block(ctx, id),
    };

    Ok(())
  }

  fn process_request(&self, ctx: &Context, data: Vec<u8>) -> io::Result<()> {
    let mut buffer = self.buffer.lock().unwrap();
    buffer.extend(&data);

    self.process_create_block(ctx, &data);

    Ok(())
  }
}
