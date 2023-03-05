//!
//! For now, the MinetestServer is just a wrapper around a MinetestSocket,
//! and a MinetestConnection is just a wrapper around a SocketPeer.
//!
//! In the future it may provide its own abstraction above the Minetest Commands.

use std::net::SocketAddr;
use std::time::Duration;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use super::conn::MinetestConnection;
use super::socket::MinetestSocket;

pub struct MinetestServer {
    accept_rx: UnboundedReceiver<MinetestConnection>,
}

impl MinetestServer {
    pub fn new(bind_addr: SocketAddr) -> Self {
        let (accept_tx, accept_rx) = unbounded_channel();
        let runner = MinetestServerRunner {
            bind_addr: bind_addr,
            accept_tx: accept_tx,
        };
        tokio::spawn(async move {
            runner.run().await;
        });
        Self {
            accept_rx: accept_rx,
        }
    }

    pub async fn accept(&mut self) -> MinetestConnection {
        self.accept_rx.recv().await.unwrap()
    }
}

struct MinetestServerRunner {
    bind_addr: SocketAddr,
    accept_tx: UnboundedSender<MinetestConnection>,
}

impl MinetestServerRunner {
    async fn run(self) {
        println!("MinetestServer starting on {}", self.bind_addr.to_string());
        let mut socket = loop {
            match MinetestSocket::new(self.bind_addr, true).await {
                Ok(socket) => break socket,
                Err(err) => {
                    println!("MinetestServer: bind failed: {}", err);
                    println!("Retrying in 5 seconds");
                    tokio::time::sleep(Duration::from_millis(5000)).await;
                }
            };
        };
        println!("MinetestServer started");
        loop {
            let t = socket.accept().await.unwrap();
            println!("MinetestServer accepted connection");
            let conn = MinetestConnection::new(t);
            match self.accept_tx.send(conn) {
                Ok(_) => (),
                Err(_) => println!("Unexpected send fail in MinetestServer"),
            }
        }
    }
}
