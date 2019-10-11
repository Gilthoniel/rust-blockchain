use rand::seq::SliceRandom;
use mio::Ready;
use std::io;
use std::sync::Mutex;
use std::net::SocketAddr;
use std::collections::HashMap;
use super::Service;
use crate::{Event, Message, Block, BlockID};
use crate::server::Context;

struct Proposal {
  id: BlockID,
  num_ack: u64,
}

pub struct BlockService {
  queue: Mutex<HashMap<SocketAddr, Block>>,
  proposal: Mutex<Option<Proposal>>,
  blocks: Vec<Block>,
}

impl BlockService {
  pub fn new() -> Self {
    BlockService{
      queue: Mutex::new(HashMap::new()),
      proposal: Mutex::new(None),
      blocks: vec![Block::new(vec![])],
    }
  }

  fn process_propose_block(&self, ctx: &Context, evt: &Event) {
    let (block, proposer) = if let Event::ProposeBlock(block, proposer) = evt {
      (block, proposer)
    } else {
      return;
    };

    if !block.verify() {
      // block is invalid thus we abort any operation.
      return;
    }

    let mut queue = self.queue.lock().unwrap();
    queue.insert(proposer.clone(), block.clone());
    drop(queue);

    // Then we send the ack of the block
    let id = block.hash();

    // Propagate the proposal
    ctx.propagate(evt);

    // Send the ACK to the
    let ack = Event::AckBlock(id);
    let buf = serde_json::to_vec(&Message::Event(ack)).unwrap();

    ctx.poll(Ready::writable());
    ctx.get_socket().send_to(&buf, &proposer).unwrap();
  }

  fn process_ack_block(&self, ctx: &Context, evt: &Event, from: &SocketAddr) {
    let id = if let Event::AckBlock(id) = evt {
      id
    } else {
      return;
    };

    println!("Server {:?} got ack for {} from {:?}", ctx.get_addr(), id, from);

    let mut proposal = self.proposal.lock().unwrap();
    match proposal.as_mut() {
      Some(mut p) => {
        if *id != p.id {
          return;
        }

        p.num_ack += 1;
        if p.num_ack == (ctx.get_peers().len() as u64) {
          proposal.take();
          ctx.propagate(&Event::ValidateBlock(id.clone()));
        }
      }
      None => (),
    };
  }

  fn process_validate_block(&self, ctx: &Context, evt: &Event) {
    let id = if let Event::ValidateBlock(id) = evt {
      id
    } else {
      return;
    };

    println!("Server {:?} got validation for {}", ctx.get_addr(), id);
    let block = self.blocks.first().unwrap();
    let mut rng = block.get_rng();
    let leader = ctx.get_peers().choose(&mut rng).unwrap();

    let mut queue = self.queue.lock().unwrap();
    let q = queue.get(leader);
    if let Some(block) = q {
      let block = block.clone();
      if block.hash() == *id {
        queue.clear();

        ctx.announce_block(&block);
      }
    }
  }
}

impl Service for BlockService {
  fn process_event(&self, ctx: &Context, evt: &Event, from: &SocketAddr) -> io::Result<()> {
    match evt {
      Event::ProposeBlock(_, _) => self.process_propose_block(ctx, evt),
      Event::AckBlock(_) => self.process_ack_block(ctx, evt, from),
      Event::ValidateBlock(_) => self.process_validate_block(ctx, evt),
    };

    Ok(())
  }

  fn process_request(&self, ctx: &Context, data: Vec<u8>) -> io::Result<()> {
    let block = self.blocks.last().unwrap().next(data);
    let mut queue = self.queue.lock().unwrap();
    queue.insert(ctx.get_addr().clone(), block.clone());
    drop(queue);

    let mut proposal = self.proposal.lock().unwrap();
    proposal.replace(Proposal {
      id: block.hash(),
      num_ack: 0,
    });

    let evt = Event::ProposeBlock(block, ctx.get_addr().clone());
    let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();

    ctx
      .get_socket()
      .send_to(&buf, ctx.get_peers().last().unwrap())
      .unwrap();

    Ok(())
  }
}
