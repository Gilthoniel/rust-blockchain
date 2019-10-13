mod context;
mod service;

use super::{Block};
use context::Context;
use log::{info};
use service::block_service::BlockService;
use std::io;
use std::net::SocketAddr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;

pub struct Server {
  ctx: Arc<Context>,
  rx_wait: Receiver<Block>,
  thread: Option<std::thread::JoinHandle<()>>,
  tx_close: Option<Sender<()>>,
}

impl Server {
  pub fn new(addr: &SocketAddr, peers: Vec<SocketAddr>) -> io::Result<Self> {
    let (tx, rx_wait) = mpsc::channel();

    let mut ctx = Context::new(addr.clone(), peers, tx)?;
    ctx.register_event_handler(BlockService::new())?;

    Ok(Server {
      tx_close: None,
      thread: None,
      rx_wait,
      ctx: Arc::new(ctx),
    })
  }

  pub fn start(&mut self) {
    info!("Server {} is starting", self.ctx.get_addr());
    let ctx = self.ctx.clone();

    let (tx, rx_close) = mpsc::channel();
    self.tx_close = Some(tx);

    self.thread = Some(std::thread::spawn(move || {
      loop {
        if let Ok(_) = rx_close.try_recv() {
          return;
        }

        ctx.next();
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

    info!("{} has closed.", self.ctx.get_addr());
  }

  pub fn wait(&self, f: impl Fn(Block) -> bool) {
    let mut is_waiting = true;
    while is_waiting {
      let msg = self.rx_wait.recv().unwrap();
      is_waiting = !f(msg);
    }
  }
}
