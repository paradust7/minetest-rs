//! MinetestConnection
//!
//!
//!
use std::net::SocketAddr;

use crate::peer::peer::Peer;
use crate::wire::command::*;
use crate::wire::types::*;
use anyhow::bail;
use anyhow::Result;

/// This is owned by the driver
pub struct MinetestConnection {
    peer: Peer,
}

impl MinetestConnection {
    pub fn new(peer: Peer) -> Self {
        Self { peer: peer }
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.peer.remote_addr()
    }

    /// Send a command to the client
    pub async fn send(&self, command: ToClientCommand) -> Result<()> {
        self.peer.send(Command::ToClient(command)).await
    }

    pub async fn send_access_denied(&self, code: AccessDeniedCode) -> Result<()> {
        self.send(AccessDeniedSpec { code }.into()).await
    }

    /// Await a command from the peer
    /// Returns (channel, reliable flag, Command)
    /// Returns None when the peer is disconnected
    pub async fn recv(&mut self) -> Result<ToServerCommand> {
        match self.peer.recv().await? {
            Command::ToServer(command) => Ok(command),
            Command::ToClient(_) => {
                bail!("Received wrong direction command from SocketPeer")
            }
        }
    }
}

/// This is owned by the MinetestServer
pub struct MinetestConnectionRecord {}
