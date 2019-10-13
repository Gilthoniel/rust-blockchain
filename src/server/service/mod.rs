pub mod block_service;

use std::io;
use std::net::SocketAddr;
use crate::Event;
use crate::server::Context;

pub trait Service: Send + Sync + 'static {
  fn process_event(&self, ctx: &Context, evt: Event, from: &SocketAddr) -> io::Result<()>;

  fn process_request(&self, ctx: &Context, data: Vec<u8>) -> io::Result<()>;
}