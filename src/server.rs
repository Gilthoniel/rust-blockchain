use rand::seq::SliceRandom;
use mio::{net::UdpSocket, Poll, Events, Token, Ready, PollOpt};
use std::collections::HashMap;
use std::net::{SocketAddr};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use std::io;
use super::{Message, Event, Block, BlockID};

const WAIT_TIMEOUT: Option<Duration> = Some(Duration::from_millis(100));

struct Proposal {
  id: BlockID,
  num_ack: u64,
}

struct Context {
  socket: Arc<UdpSocket>,
  poll: Poll,
  events: Mutex<Events>,
  addr: SocketAddr,
  peers: Vec<SocketAddr>,
  tx: Mutex<Sender<Block>>,
  queue: Mutex<HashMap<SocketAddr, Block>>,
  proposal: Mutex<Option<Proposal>>,
  blocks: Vec<Block>,
}

impl Context {
  fn propagate(&self, evt: &Event) {
    let buf = serde_json::to_vec(&Message::Event(evt.clone())).unwrap();
    let mut rng = rand::thread_rng();
    for addr in self.peers.choose_multiple(&mut rng, 3) {
      self.poll(Ready::writable());
      self.socket.send_to(&buf, addr).unwrap();
    }
  }

  fn handle_event(&self, evt: &Event, src: &SocketAddr) {
    match evt {
      Event::ProposeBlock(block, proposer) => {
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
        self.propagate(evt);

        // Send the ACK to the 
        let ack = Event::AckBlock(id);
        let buf = serde_json::to_vec(&Message::Event(ack)).unwrap();

        self.poll(Ready::writable());
        self.socket.send_to(&buf, &proposer).unwrap();
      },
      Event::AckBlock(id) => {
        println!("Server {:?} got ack for {} from {:?}", self.addr, id, src);

        let mut proposal = self.proposal.lock().unwrap();
        match proposal.as_mut() {
          Some(mut p) => {
            if *id != p.id {
              return;
            }

            p.num_ack += 1;
            if p.num_ack == (self.peers.len() as u64) {
              proposal.take();
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

        let mut queue = self.queue.lock().unwrap();
        let q = queue.get(leader);
        if let Some(block) = q {
          let block = block.clone();
          if block.hash() == *id {
            queue.clear();

            self.tx.lock().unwrap().send(block.clone()).unwrap();
          }
        }
      },
    }
  }

  fn handle_request(&self, data: Vec<u8>) {
    let block = self.blocks.last().unwrap().next(data);
    let mut queue = self.queue.lock().unwrap();
    queue.insert(self.addr, block.clone());
    drop(queue);

    let mut proposal = self.proposal.lock().unwrap();
    proposal.replace(Proposal{
      id: block.hash(),
      num_ack: 0,
    });

    let evt = Event::ProposeBlock(block, self.addr);
    let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();
    
    self.socket.send_to(&buf, self.peers.last().unwrap()).unwrap();
  }

  fn poll(&self, r: Ready) {
    let mut events = self.events.lock().unwrap();
    loop {
      self.poll.poll(&mut events, WAIT_TIMEOUT).unwrap();

      for event in events.iter() {
        if event.token() == Token(0) && event.readiness().contains(r) {
          return;
        }
      }
    }
  }
}

pub struct Server {
  tx_close: Option<Sender<()>>,
  thread: Option<std::thread::JoinHandle<()>>,
  rx: Receiver<Block>,
  ctx: Arc<Context>,
}

impl Server {
  pub fn new(addr: &SocketAddr, peers: Vec<SocketAddr>) -> io::Result<Self> {
    let socket = Arc::new(UdpSocket::bind(addr)?);
    let addr = addr.clone();
    let (tx, rx) = mpsc::channel();

    let poll = Poll::new().unwrap();
    poll.register(&socket, Token(0), Ready::writable() | Ready::readable(), PollOpt::edge()).unwrap();

    Ok(Server{
      tx_close: None,
      thread: None,
      rx,
      ctx: Arc::new(Context{
        socket,
        events: Mutex::new(Events::with_capacity(128)),
        poll,
        tx: Mutex::new(tx),
        addr,
        peers,
        queue: Mutex::new(HashMap::new()),
        proposal: Mutex::new(None),
        blocks: vec![Block::new(vec![])],
      }),
    })
  }

  pub fn start(&mut self) {
    let ctx = self.ctx.clone();

    let (tx, rx_close) = mpsc::channel();
    self.tx_close = Some(tx);

    self.thread = Some(std::thread::spawn(move || {
      let mut buf = [0; 1024];
      loop {
        if let Ok(_) = rx_close.try_recv() {
          return;
        }

        match ctx.socket.recv_from(&mut buf) {
          Ok((size, src)) => {
            let msg: Message = serde_json::from_slice(&buf[..size]).unwrap();

            match msg.clone() {
              Message::Event(evt) => {
                ctx.handle_event(&evt, &src);
              },
              Message::Request(data) => {
                ctx.handle_request(data);
              },
            };
          },
          Err(e) => {
            if e.kind() != io::ErrorKind::WouldBlock {
              println!("Error: {:?}", e);
              return;
            }

            // WouldBlock error so we need to wait for events
            ctx.poll(Ready::readable());
          }
        }
      }
    }));
  }

  pub fn stop(self) {
    // send the close message.
    if let Some(tx_close) = self.tx_close {
      tx_close.send(()).unwrap();
    }
    
    if let Some(th) = self.thread {
      th.join().unwrap();
    }

    println!("Server {:?} has closed.", self.ctx.addr);
  }

  pub fn wait(&self, f: impl Fn(Block) -> bool) {
    let mut is_waiting = true;
    while is_waiting {
      let msg = self.rx.recv().unwrap();
      is_waiting = !f(msg);
    }
  }
}
