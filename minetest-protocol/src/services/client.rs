use std::net::SocketAddr;

use anyhow::bail;

use super::socket::MinetestSocket;
use crate::peer::peer::Peer;
use crate::wire::command::*;

pub struct MinetestClient {
    remote_peer: Peer,
}

impl MinetestClient {
    pub async fn connect(connect_to: SocketAddr) -> anyhow::Result<Self> {
        let bind_addr = if connect_to.is_ipv4() {
            "0.0.0.0:0".parse()?
        } else {
            "[::]:0".parse()?
        };
        let mut socket = MinetestSocket::new(bind_addr, false).await?;

        // Send a null packet to server.
        // It should answer back, establishing a peer ids.
        let remote_peer = socket.add_peer(connect_to).await;

        Ok(Self { remote_peer })
    }

    /// If this fails, the client has disconnected.
    pub async fn recv(&mut self) -> anyhow::Result<ToClientCommand> {
        match self.remote_peer.recv().await.unwrap() {
            Command::ToClient(cmd) => Ok(cmd),
            Command::ToServer(_) => bail!("Invalid packet direction"),
        }
    }

    /// If this fails, the client has disconnected.
    pub async fn send(&mut self, command: ToServerCommand) -> anyhow::Result<()> {
        self.remote_peer.send(Command::ToServer(command)).await
    }
}
