mod service;
mod context;

use mio::Ready;
use log::{info, error};
use std::net::{SocketAddr};
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::io;
use super::{Message, Block};
use context::Context;
use service::block_service::BlockService;

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

    Ok(Server{
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
      let mut buf = [0; 1024];
      loop {
        if let Ok(_) = rx_close.try_recv() {
          return;
        }

        match ctx.get_socket().recv_from(&mut buf) {
          Ok((size, src)) => {
            let msg: Message = serde_json::from_slice(&buf[..size]).unwrap();

            match msg {
              Message::Event(evt) => {
                if let Err(e) = ctx.handle_event(&evt, &src) {
                  error!("Error when processing an event: {:?}", e);
                }
              },
              Message::Request(data) => {
                if let Err(e) = ctx.handle_request(data) {
                  error!("Error when processing a request: {:?}", e);
                }
              },
            };
          },
          Err(e) => {
            if e.kind() != io::ErrorKind::WouldBlock {
              error!("Error: {:?}", e);
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

    info!("Server {:?} has closed.", self.ctx.get_addr());
  }

  pub fn wait(&self, f: impl Fn(Block) -> bool) {
    let mut is_waiting = true;
    while is_waiting {
      let msg = self.rx_wait.recv().unwrap();
      is_waiting = !f(msg);
    }
  }
}
