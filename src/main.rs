mod server;
mod client;
mod block;

pub use block::*;

use serde::{Serialize, Deserialize};
use std::net::SocketAddr;
use server::Server;
use client::Client;

#[derive(Serialize, Deserialize, Clone)]
pub enum Event {
    ProposeBlock(Block, SocketAddr),
    AckBlock(BlockID),
    ValidateBlock(BlockID),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Event(Event),
    Request(Vec<u8>),
}

fn main() {
    let n: usize = 3;
    let mut peers = Vec::new();
    let mut srvs = Vec::new();
    for i in 0..n {
        let addr: SocketAddr = ([127, 0, 0, 1], 3000+(i as u16)).into();
        peers.push(addr.clone());
    }

    for i in 0..n {
        let srv = Server::new(&peers[i], peers.clone()).unwrap();
        srv.start();
        srvs.push(srv);
    }

    let cl = Client::new().unwrap();
    for to in peers {
        cl.send_to(&to, Message::Request(vec![1, 2, 3]));
    }

    for srv in srvs {
        srv.wait(|_| true);
    }
}
