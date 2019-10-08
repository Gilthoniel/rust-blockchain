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
    ProposeBlock(Block),
    AckBlock(BlockID),
}

#[derive(Serialize, Deserialize, Clone)]
pub enum Message {
    Event(Event),
    Request(Vec<u8>),
}

fn main() {
    let n: usize = 4;
    let mut peers = Vec::new();
    let mut srvs = Vec::new();
    for i in 0..n {
        let addr: SocketAddr = ([127, 0, 0, 1], 3000+(i as u16)).into();
        peers.push(addr.clone());
    }

    for i in 0..n {
        srvs.push(Server::new(&peers[i], peers.clone()).unwrap());
    }

    let cl = Client::new().unwrap();
    cl.send_to(peers.first().unwrap(), Message::Request(vec![1, 2, 3]));

    srvs.into_iter().nth(0).unwrap().wait(|_| true);
}
