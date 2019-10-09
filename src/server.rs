use rand::seq::SliceRandom;
use std::collections::HashMap;
use std::net::{UdpSocket, SocketAddr};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::io;
use super::{Message, Event, Block, BlockID};

struct Proposal {
  id: BlockID,
  num_ack: u64,
}

struct Context {
  socket: Arc<UdpSocket>,
  addr: SocketAddr,
  peers: Vec<SocketAddr>,
  tx: Sender<Block>,
  queue: HashMap<SocketAddr, Block>,
  proposal: Option<Proposal>,
  blocks: Vec<Block>,
}

impl Context {
  fn propagate(&self, evt: &Event) {
    let buf = serde_json::to_vec(&Message::Event(evt.clone())).unwrap();
    let mut rng = rand::thread_rng();
    for addr in self.peers.choose_multiple(&mut rng, 3) {
      self.socket.send_to(&buf, addr).unwrap();
    }
  }

  fn handle_event(&mut self, evt: &Event, src: &SocketAddr) {
    match evt {
      Event::ProposeBlock(block, proposer) => {
        if !block.verify() {
          // block is invalid thus we abort any operation.
          return;
        }

        self.queue.insert(proposer.clone(), block.clone());

        // Then we send the ack of the block
        let id = block.hash();

        // Propagate the proposal
        self.propagate(evt);

        // Send the ACK to the 
        let ack = Event::AckBlock(id);
        let buf = serde_json::to_vec(&Message::Event(ack)).unwrap();
        self.socket.send_to(&buf, &proposer).unwrap();
      },
      Event::AckBlock(id) => {
        println!("Server {:?} got ack for {} from {:?}", self.addr, id, src);

        match self.proposal.as_mut() {
          Some(mut p) => {
            if *id != p.id {
              return;
            }

            p.num_ack += 1;
            if p.num_ack == (self.peers.len() as u64) {
              self.proposal = None;
              self.propagate(&Event::ValidateBlock(id.clone()));
            }
          },
          None => (),
        };
      },
      Event::ValidateBlock(id) => {
        println!("Server {:?} got validation for {:?}", self.addr, id);
        let block = self.blocks.first().unwrap();
        let mut rng = block.get_rng();
        let leader = self.peers.choose(&mut rng).unwrap();

        let q = self.queue.get(leader);
        if let Some(block) = q {
          let block = block.clone();
          if block.hash() == *id {
            self.queue.clear();

            self.tx.send(block.clone()).unwrap();
          }
        }
      },
    }
  }

  fn handle_request(&mut self, data: Vec<u8>) {
    let block = self.blocks.last().unwrap().next(data);
    self.queue.insert(self.addr, block.clone());
    self.proposal = Some(Proposal{
      id: block.hash(),
      num_ack: 0,
    });

    let evt = Event::ProposeBlock(block, self.addr);
    let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();
    
    self.socket.send_to(&buf, self.peers.last().unwrap()).unwrap();
  }
}

pub struct Server {
  rx: Receiver<Block>,
  ctx: Arc<Mutex<Context>>,
}

impl Server {
  pub fn new(addr: &SocketAddr, peers: Vec<SocketAddr>) -> io::Result<Self> {
    let socket = Arc::new(UdpSocket::bind(addr)?);
    let addr = addr.clone();
    let (tx, rx) = mpsc::channel();

    Ok(Server{
      rx,
      ctx: Arc::new(Mutex::new(Context{
        socket,
        tx,
        addr,
        peers,
        queue: HashMap::new(),
        proposal: None,
        blocks: vec![Block::new(vec![])],
      })),
    })
  }

  pub fn start(&self) {
    let ctx = self.ctx.clone();

    std::thread::spawn(move || {
      let mut ctx = ctx.lock().unwrap();
      let mut buf = [0; 1024];
      loop {
        let (size, src) = ctx.socket.recv_from(&mut buf).unwrap();

        let msg: Message = serde_json::from_slice(&buf[..size]).unwrap();

        match msg.clone() {
          Message::Event(evt) => {
            ctx.handle_event(&evt, &src);
          },
          Message::Request(data) => {
            ctx.handle_request(data);
          },
        };
      }
    });
  }

  pub fn wait(self, f: impl Fn(Block) -> bool) {
    let mut is_waiting = true;
    while is_waiting {
      let msg = self.rx.recv().unwrap();
      is_waiting = !f(msg);
    }
  }
}
