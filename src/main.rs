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
    simple_logger::init().unwrap();

    let n: usize = 3;
    let mut peers = Vec::new();
    let mut srvs = Vec::new();

    // Create the addresses.
    for i in 0..n {
        let addr: SocketAddr = ([127, 0, 0, 1], 3000+(i as u16)).into();
        peers.push(addr.clone());
    }

    // Create and start the servers.
    for i in 0..n {
        let mut srv = Server::new(&peers[i], peers.clone()).unwrap();
        srv.start();
        srvs.push(srv);
    }

    // Send a request to each server. They will all try to create a block.
    let cl = Client::new().unwrap();
    for to in peers {
        cl.send_to(&to, Message::Request(vec![1, 2, 3]));
    }

    // Wait for a block creation on each server to continue.
    for srv in &srvs {
        srv.wait(|_| true);
    }

    // Stop and clean each server. It waits for thread to close.
    for srv in srvs {
        srv.stop();
    }
}
