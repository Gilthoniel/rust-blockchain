use std::net::{UdpSocket, SocketAddr};
use std::io;
use super::Message;

pub struct Client {
  socket: UdpSocket,
}

impl Client {
  pub fn new() -> io::Result<Self> {
    let socket = UdpSocket::bind("0.0.0.0:0")?;
    Ok(Client{ socket })
  }

  pub fn send_to(&self, addr: &SocketAddr, msg: Message) {
    let buf = serde_json::to_vec(&msg).unwrap();
    self.socket.send_to(&buf, addr).unwrap();
  }
}
