//!
//! Minetest Proxy Server
//!
//! This heavily tests the code for serialization, deserialization,
//! packet splitting, and reliable retransmission.
//!
//! This is not just a simple packet forwarding proxy. Instead, it performs
//! split packet reconstruction and reliable tracking itself. Incoming
//! packets are deserialized to a stream of Commands (a strongly typed
//! representation of minetest data).
//!
//! To forward the Command to the other side, serialization, packet
//! splitting, and reliable tracking are performed in the opposite direction.
//!
//! If everything is correct, the proxied connection should be stable, and
//! durable to packet loss.
//!
//! As an added bonus, enabling verbose mode will print out the stream of
//! commands in both directions, in a human-readable format.
use anyhow::Result;

use minetest_protocol::peer::peer::PeerError;
use minetest_protocol::wire::command::ToClientCommand;
use minetest_protocol::CommandDirection;
use minetest_protocol::CommandRef;
use minetest_protocol::MinetestClient;
use minetest_protocol::MinetestConnection;
use minetest_protocol::MinetestServer;
use std::net::SocketAddr;

pub struct MinetestProxy {}

impl MinetestProxy {
    pub fn new(bind_addr: SocketAddr, forwarding_addr: SocketAddr, verbosity: u8) -> Self {
        let runner = MinetestProxyRunner {
            bind_addr,
            forwarding_addr,
            verbosity,
        };
        tokio::spawn(async move { runner.run().await });
        MinetestProxy {}
    }
}

struct MinetestProxyRunner {
    bind_addr: SocketAddr,
    forwarding_addr: SocketAddr,
    verbosity: u8,
}

impl MinetestProxyRunner {
    async fn run(self) {
        let mut server = MinetestServer::new(self.bind_addr);
        let mut next_id: u64 = 1;
        loop {
            tokio::select! {
                conn = server.accept() => {
                    let id = next_id;
                    next_id += 1;
                    println!("[P{}] New client connected from {:?}", id, conn.remote_addr());
                    let client = MinetestClient::connect(self.forwarding_addr).await.expect("Connect failed");
                    ProxyAdapterRunner::spawn(id, conn, client, self.verbosity);
                },
            }
        }
    }
}

pub struct ProxyAdapterRunner {
    id: u64,
    conn: MinetestConnection,
    client: MinetestClient,
    verbosity: u8,
}

impl ProxyAdapterRunner {
    pub fn spawn(id: u64, conn: MinetestConnection, client: MinetestClient, verbosity: u8) {
        let runner = ProxyAdapterRunner {
            id,
            conn,
            client,
            verbosity,
        };
        tokio::spawn(async move { runner.run().await });
    }

    pub async fn run(mut self) {
        match self.run_inner().await {
            Ok(_) => (),
            Err(err) => {
                let show_err = if let Some(err) = err.downcast_ref::<PeerError>() {
                    match err {
                        PeerError::PeerSentDisconnect => false,
                        _ => true,
                    }
                } else {
                    true
                };
                if show_err {
                    println!("[{}] Disconnected: {:?}", self.id, err)
                } else {
                    println!("[{}] Disconnected", self.id)
                }
            }
        }
    }

    pub async fn run_inner(&mut self) -> Result<()> {
        loop {
            tokio::select! {
                t = self.conn.recv() => {
                    let command = t?;
                    self.maybe_show(&command);
                    self.client.send(command).await?;
                },
                t = self.client.recv() => {
                    let command = t?;
                    self.maybe_show(&command);
                    self.conn.send(command).await?;
                }
            }
        }
    }

    pub fn is_bulk_command<Cmd: CommandRef>(&self, command: &Cmd) -> bool {
        if let Some(cmd) = command.toclient_ref() {
            match cmd {
                ToClientCommand::Blockdata(_) => true,
                ToClientCommand::Media(_) => true,
                _ => false,
            }
        } else {
            false
        }
    }

    pub fn maybe_show<Cmd: CommandRef>(&self, command: &Cmd) {
        let dir = match command.direction() {
            CommandDirection::ToClient => "S->C",
            CommandDirection::ToServer => "C->S",
        };
        let prefix = format!("[{}] {} ", self.id, dir);
        let mut verbosity = self.verbosity;
        if verbosity == 2 && self.is_bulk_command(command) {
            // Show the contents of smaller commands, but skip the huge ones
            verbosity = 1;
        }
        match verbosity {
            0 => (),
            1 => println!("{} {}", prefix, command.command_name()),
            2.. => println!("{} {:#?}", prefix, command),
        }
    }
}
