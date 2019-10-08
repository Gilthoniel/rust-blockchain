use std::net::{UdpSocket, SocketAddr};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::io;
use super::{Message, Event, Block};

struct Context {
  socket: Arc<UdpSocket>,
  addr: SocketAddr,
  peers: Vec<SocketAddr>,
  tx: Sender<Message>,
  num_ack: Arc<AtomicU64>,
}

pub struct Server {
  rx: Receiver<Message>,
}

fn handle_event(ctx: Context, msg: &Message, evt: Event, src: &SocketAddr) {
  match evt {
    Event::ProposeBlock(block) => {
      // Let's say the verification is done

      // Then we send the ack of the block
      let id = block.hash();
      let ack = Event::AckBlock(id);

      let buf = serde_json::to_vec(&Message::Event(ack)).unwrap();
      ctx.socket.send_to(&buf, &src).unwrap();
    },
    Event::AckBlock(id) => {
      println!("Server {:?} got ack for {} from {:?}", ctx.addr, id, src);
      ctx.num_ack.fetch_add(1, Ordering::Relaxed);
      if ctx.num_ack.load(Ordering::Relaxed) == (ctx.peers.len() as u64) {
        ctx.tx.send(msg.clone()).unwrap();
      }
    }
  }
}

impl Server {
  pub fn new(addr: &SocketAddr, peers: Vec<SocketAddr>) -> io::Result<Self> {
    let socket = Arc::new(UdpSocket::bind(addr)?);
    let addr = addr.clone();
    let (tx, rx) = mpsc::channel();
    let num_ack = Arc::new(AtomicU64::new(0));

    std::thread::spawn(move || {
      let mut buf = [0; 1024];
      loop {
        let (size, src) = socket.recv_from(&mut buf).unwrap();

        let msg: Message = serde_json::from_slice(&buf[..size]).unwrap();
        let ctx = Context {
          socket: socket.clone(),
          addr,
          peers: peers.clone(),
          tx: tx.clone(),
          num_ack: num_ack.clone(),
        };

        match msg.clone() {
          Message::Event(evt) => {
            handle_event(ctx, &msg, evt, &src);
          },
          Message::Request(data) => {
            let evt = Event::ProposeBlock(Block::new(data));
            let buf = serde_json::to_vec(&Message::Event(evt)).unwrap();
            
            for addr in peers.iter() {
              socket.send_to(&buf, addr).unwrap();
            }
          },
        };
      }
    });

    Ok(Server{ rx })
  }

  pub fn wait(self, f: impl Fn(Message) -> bool) {
    let mut is_waiting = true;
    while is_waiting {
      let msg = self.rx.recv().unwrap();
      is_waiting = !f(msg);
    }
  }
}
