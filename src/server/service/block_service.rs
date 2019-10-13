use rand::seq::SliceRandom;
use log::{error, info, debug, trace};
use std::io;
use std::sync::Mutex;
use std::net::SocketAddr;
use super::Service;
use crate::{Event, Block};
use crate::server::Context;

pub struct BlockService {
  buffer: Mutex<Vec<u8>>,
  future_queue: Mutex<Vec<Block>>,
  blocks: Mutex<Vec<Block>>,
}

impl BlockService {
  pub fn new() -> Self {
    BlockService{
      buffer: Mutex::new(Vec::new()),
      future_queue: Mutex::new(Vec::new()),
      blocks: Mutex::new(vec![Block::new(([0, 0, 0, 0], 0).into(), vec![])]),
    }
  }

  fn process_propose_block(&self, ctx: &Context, block: Block) {
    if !block.verify() {
      // block is invalid thus we abort any operation.
      return;
    }

    if block.has_leader(ctx.get_addr()) {
      if block.get_ack() == ctx.get_peers().len() {
        ctx.propagate(&Event::ValidateBlock(block));
      } else {
        error!("Not enough ACKs");
      }
      return;
    }

    {
      let blocks = self.blocks.lock().unwrap();
      let last = blocks.last().unwrap();
      if !last.has_next(&block) {
        let mut queue = self.future_queue.lock().unwrap();
        queue.push(block.clone());
        // Wait for the current round to finish before processing future blocks.
        return;
      }
    }

    let mut block = block;
    let index = ctx.get_peers().iter().position(|p| p == ctx.get_addr()).unwrap();
    block.incr_ack(index);

    // Propagate the proposal.
    ctx.propagate(&Event::ProposeBlock(block));
  }

  fn process_validate_block(&self, ctx: &Context, block: Block, from: &SocketAddr) {
    {
      let blocks = self.blocks.lock().unwrap();
      for b in blocks.iter() {
        if b.hash() == block.hash() {
          return;
        }
      }
    }

    let leader = self.get_leader(ctx.get_peers());
    if block.has_leader(ctx.get_addr()) {
      // Got our own block so everyone saw it.
      return;
    }

    debug!("{} got validation for {} from {}", ctx.get_addr(), block.hash(), from);
    ctx.propagate(&Event::ValidateBlock(block.clone()));

    info!("{} is announcing block {} from leader {}", ctx.get_addr(), block.hash(), leader);
    ctx.announce_block(&block);

    // Store the new block.
    {
      let mut blocks = self.blocks.lock().unwrap();
      blocks.push(block.clone());
    }

    // Empty the queue of future blocks.
    let items: Vec<_> = {
      // Lock needs to be released for the handler.
      let mut queue = self.future_queue.lock().unwrap();
      queue.drain(..).collect()
    };
    
    for block in items {
      self.process_propose_block(ctx, block);
    }

    self.future_queue.lock().unwrap().clear();

    self.retry_block(ctx);
  }

  fn process_create_block(&self, ctx: &Context, data: &Vec<u8>) {
    let blocks = self.blocks.lock().unwrap();
    let mut block = blocks.last().unwrap().next(ctx.get_addr().clone(), data.clone());
    let index = ctx.get_peers().iter().position(|p| p == ctx.get_addr()).unwrap();
    block.incr_ack(index);
    drop(blocks);

    trace!("{} asking for block {}", ctx.get_addr(), block.hash());
    let evt = Event::ProposeBlock(block);

    ctx.propagate(&evt);
  }

  fn get_leader<'a>(&self, peers: &'a Vec<SocketAddr>) -> &'a SocketAddr {
    let blocks = self.blocks.lock().unwrap();
    let block = blocks.last().unwrap();
    let mut rng = block.get_rng();

    return peers.choose(&mut rng).unwrap();
  }

  fn retry_block(&self, ctx: &Context) {
    debug!("{} is retrying block", ctx.get_addr());

    let buffer = self.buffer.lock().unwrap();
    if buffer.len() > 0 {
      self.process_create_block(ctx, &buffer);
    }
  }
}

impl Service for BlockService {
  fn process_event(&self, ctx: &Context, evt: Event, from: &SocketAddr) -> io::Result<()> {
    match evt {
      Event::ProposeBlock(block) => self.process_propose_block(ctx, block),
      Event::ValidateBlock(block) => self.process_validate_block(ctx, block, from),
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
