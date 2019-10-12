use rand::seq::SliceRandom;
use mio::Ready;
use log::{debug, trace};
use std::io;
use std::sync::Mutex;
use std::net::SocketAddr;
use std::collections::HashMap;
use super::Service;
use crate::{Event, Message, Block, BlockID};
use crate::server::Context;

struct Proposal {
  id: BlockID,
  data: Vec<u8>,
  num_ack: u64,
}

pub struct BlockService {
  queue: Mutex<HashMap<SocketAddr, Block>>,
  proposal: Mutex<Option<Proposal>>,
  blocks: Mutex<Vec<Block>>,
}

impl BlockService {
  pub fn new() -> Self {
    BlockService{
      queue: Mutex::new(HashMap::new()),
      proposal: Mutex::new(None),
      blocks: Mutex::new(vec![Block::new(vec![])]),
    }
  }

  fn process_propose_block(&self, ctx: &Context, block: &Block, proposer: &SocketAddr) {
    if !block.verify() {
      // block is invalid thus we abort any operation.
      return;
    }

    // Verify if we already know this block.
    {
      let blocks = self.blocks.lock().unwrap();
      for b in blocks.iter() {
        // TODO: index in block
        if b.hash() == block.hash() {
          trace!("Got block already accepted. Aborting gossip.");
          return;
        }
      }
    }

    // Add the block in the map of proposed blocks so far
    // for this round.
    {
      let mut queue = self.queue.lock().unwrap();
      queue.insert(proposer.clone(), block.clone());
    }

    // Then we send the ack of the block.
    let id = block.hash();

    // Propagate the proposal.
    ctx.propagate(&Event::ProposeBlock(block.clone(), proposer.clone()));

    // Send the ACK to the proposer.
    let ack = Event::AckBlock(id);
    let buf = serde_json::to_vec(&Message::Event(ack)).unwrap();

    ctx.poll(Ready::writable());
    ctx.get_socket().send_to(&buf, &proposer).unwrap();
  }

  fn process_ack_block(&self, ctx: &Context, id: &BlockID, from: &SocketAddr) {
    trace!("Server {:?} got ack for {} from {:?}", ctx.get_addr(), id, from);

    let mut proposal = self.proposal.lock().unwrap();
    match proposal.as_mut() {
      Some(mut p) => {
        if *id != p.id {
          return;
        }

        p.num_ack += 1;
        if p.num_ack == (ctx.get_peers().len() as u64) {
          ctx.propagate(&Event::ValidateBlock(id.clone()));
        }
      }
      None => (),
    };
  }

  fn process_validate_block(&self, ctx: &Context, id: &BlockID) {
    debug!("Server {:?} got validation for {}", ctx.get_addr(), id);

    {
      let blocks = self.blocks.lock().unwrap();
      for b in blocks.iter() {
        if b.hash() == *id {
          trace!("Got a validation for a known block. ABorting gossip.");
          return;
        }
      }
    }

    let leader = self.get_leader(ctx.get_peers());
    let mut queue = self.queue.lock().unwrap();

    if let Some(block) = queue.get(leader) {
      let block = block.clone();
      if block.hash() == *id {
        ctx.announce_block(&block);

        // Clear the current of block proposals from other nodes.
        queue.clear();

        // Store the new block.
        self.blocks.lock().unwrap().push(block.clone());

        drop(queue);
        self.retry_block(ctx, id);
      }
    }
  }

  fn process_create_block(&self, ctx: &Context, data: &Vec<u8>) {
    let blocks = self.blocks.lock().unwrap();
    let block = blocks.last().unwrap().next(data.clone());
    let mut queue = self.queue.lock().unwrap();
    queue.insert(ctx.get_addr().clone(), block.clone());
    drop(queue);

    let mut proposal = self.proposal.lock().unwrap();
    proposal.replace(Proposal {
      id: block.hash(),
      data: data.clone(),
      num_ack: 0,
    });

    let evt = Event::ProposeBlock(block, ctx.get_addr().clone());
    let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();

    ctx.poll(Ready::writable());
    ctx
      .get_socket()
      .send_to(&buf, ctx.get_peers().last().unwrap())
      .unwrap();
  }

  fn get_leader<'a>(&self, peers: &'a Vec<SocketAddr>) -> &'a SocketAddr {
    let blocks = self.blocks.lock().unwrap();
    let block = blocks.first().unwrap();
    let mut rng = block.get_rng();

    return peers.choose(&mut rng).unwrap();
  }

  fn retry_block(&self, ctx: &Context, _: &BlockID) {
    debug!("Server {} is retrying block.", ctx.get_addr());

    let data = {
      let proposal = self.proposal.lock().unwrap();
      proposal.as_ref().unwrap().data.clone()
    };

    self.process_create_block(ctx, &data);
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
    self.process_create_block(ctx, &data);

    Ok(())
  }
}
