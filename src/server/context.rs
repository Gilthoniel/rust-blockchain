use super::service::Service;
use crate::{Block, Event, Message};
use mio::{net::UdpSocket, Events, Poll, PollOpt, Ready, Token};
use log::{error, trace};
use std::io;
use std::net::SocketAddr;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

const WAIT_TIMEOUT: Option<Duration> = Some(Duration::from_millis(100));

pub struct Context {
  socket: Arc<UdpSocket>,
  poll: Poll,
  addr: SocketAddr,
  peers: Vec<SocketAddr>,
  tx: Mutex<Sender<Block>>,
  event_service: Option<Box<dyn Service>>,
  message_queue: Mutex<Vec<(Event, SocketAddr)>>,
}

const TOKEN: Token = Token(0);

impl Context {
  pub fn new(addr: SocketAddr, peers: Vec<SocketAddr>, tx: Sender<Block>) -> io::Result<Self> {
    let socket = Arc::new(UdpSocket::bind(&addr)?);

    // Socket poll to get readable and writable events from the OS.
    let poll = Poll::new()?;
    poll.register(
      &socket,
      TOKEN,
      Ready::readable() | Ready::writable(),
      PollOpt::edge(),
    )?;

    Ok(Context {
      socket,
      poll,
      tx: Mutex::new(tx),
      addr,
      peers,
      event_service: None,
      message_queue: Mutex::new(Vec::new()),
    })
  }

  pub fn get_addr(&self) -> &SocketAddr {
    &self.addr
  }

  pub fn get_peers(&self) -> &Vec<SocketAddr> {
    &self.peers
  }

  pub fn register_event_handler(&mut self, service: impl Service) -> io::Result<()> {
    self.event_service = Some(Box::new(service));

    Ok(())
  }

  pub fn next(&self) {
    let mut events = Events::with_capacity(128);
    self.poll.poll(&mut events, WAIT_TIMEOUT).unwrap();

    for event in events {
      if event.token() == TOKEN {
        if event.readiness().is_writable() {
          self.send_next();
        }

        if event.readiness().is_readable() {
          let mut buf = [0; 1024];
          let (size, src) = self.socket.recv_from(&mut buf).unwrap();

          let msg: Message = serde_json::from_slice(&buf[..size]).unwrap();

          match msg {
            Message::Event(evt) => {
              if let Err(e) = self.handle_event(&evt, &src) {
                error!("Error when processing an event: {:?}", e);
              }
            }
            Message::Request(data) => {
              if let Err(e) = self.handle_request(data) {
                error!("Error when processing a request: {:?}", e);
              }
            }
          };
        }
      }
    }
  }

  fn handle_event(&self, evt: &Event, from: &SocketAddr) -> io::Result<()> {
    trace!("{} received event {:?}", self.addr, evt);
    if let Some(h) = &self.event_service {
      h.process_event(self, evt, from)?;
    }

    Ok(())
  }

  fn handle_request(&self, data: Vec<u8>) -> io::Result<()> {
    if let Some(h) = &self.event_service {
      h.process_request(self, data)?;
    }

    Ok(())
  }

  pub fn send(&self, evt: &Event, addr: &SocketAddr) {
    let mut queue = self.message_queue.lock().unwrap();

    queue.push((evt.clone(), addr.clone()));
  }

  pub fn propagate(&self, evt: &Event) {
    let idx = self.peers.iter().position(|&p| p == self.addr).unwrap();
    let idx = (idx + 1) % self.peers.len();
    let to = self.peers.get(idx).unwrap();

    self.send(evt, to);
  }

  pub fn announce_block(&self, block: &Block) {
    self.tx.lock().unwrap().send(block.clone()).unwrap();
  }

  fn send_next(&self) {
    let mut queue = self.message_queue.lock().unwrap();
    match queue.pop() {
      Some((evt, to)) => {
        let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();
        self.socket.send_to(&buf, &to).unwrap();
      },
      None => (),
    };
  }
}
