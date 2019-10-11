use rand::seq::SliceRandom;
use mio::{net::UdpSocket, Poll, PollOpt, Events, Ready, Token};
use std::net::{SocketAddr};
use std::sync::{Arc, Mutex};
use std::sync::mpsc::Sender;
use std::time::Duration;
use std::io;
use crate::{Block, Event, Message};
use super::service::Service;

const WAIT_TIMEOUT: Option<Duration> = Some(Duration::from_millis(100));

pub struct Context {
  socket: Arc<UdpSocket>,
  poll: Poll,
  events: Mutex<Events>,
  addr: SocketAddr,
  peers: Vec<SocketAddr>,
  tx: Mutex<Sender<Block>>,
  event_service: Option<Box<dyn Service>>,
}

impl Context {
  pub fn new(addr: SocketAddr, peers: Vec<SocketAddr>, tx: Sender<Block>) -> io::Result<Self> {
    let socket = Arc::new(UdpSocket::bind(&addr)?);

    // Socket poll to get readable and writable events from the OS.
    let poll = Poll::new()?;
    poll.register(&socket, Token(0), Ready::writable() | Ready::readable(), PollOpt::edge())?;

    Ok(Context {
      socket,
      events: Mutex::new(Events::with_capacity(128)),
      poll,
      tx: Mutex::new(tx),
      addr,
      peers,
      event_service: None,
    })
  }

  pub fn get_socket(&self) -> Arc<UdpSocket> {
    self.socket.clone()
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

  pub fn handle_event(&self, evt: &Event, from: &SocketAddr) -> io::Result<()> {
    if let Some(h) = &self.event_service {
      h.process_event(self, evt, from)?;
    }

    Ok(())
  }

  pub fn handle_request(&self, data: Vec<u8>) -> io::Result<()> {
    if let Some(h) = &self.event_service {
      h.process_request(self, data)?;
    }

    Ok(())
  }

  // Poll the socket to know when it can be read or written.
  pub fn poll(&self, r: Ready) {
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

  pub fn propagate(&self, evt: &Event) {
    let buf = serde_json::to_vec(&Message::Event(evt.clone())).unwrap();
    let mut rng = rand::thread_rng();
    for addr in self.peers.choose_multiple(&mut rng, 3) {
      self.poll(Ready::writable());
      self.socket.send_to(&buf, addr).unwrap();
    }
  }

  pub fn announce_block(&self, block: &Block) {
    self.tx.lock().unwrap().send(block.clone()).unwrap();
  }
}
