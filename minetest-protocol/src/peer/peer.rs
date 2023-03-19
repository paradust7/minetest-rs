//!
//! Peer
//!
//! Turns a datagram stream (e.g. from a UdpSocket) into a stream
//! of Minetest Commands, and vice versa.
//!
//! This handles reliable transport, as well as packet splitting and
//! split packet reconstruction.
//!
//! This also handles control packets. In particular, it keeps track
//! of the assigned peer id and includes it on every packet.
//!  
use anyhow::bail;
use anyhow::Result;
use rand::rngs::StdRng;
use rand::Rng;
use rand::SeedableRng;
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;

use crate::wire::command::Command;
use crate::wire::command::CommandProperties;
use crate::wire::command::ToClientCommand;
use crate::wire::deser::Deserialize;
use crate::wire::deser::Deserializer;
use crate::wire::packet::AckBody;
use crate::wire::packet::ControlBody;
use crate::wire::packet::InnerBody;
use crate::wire::packet::Packet;
use crate::wire::packet::PacketBody;
use crate::wire::packet::PeerId;
use crate::wire::packet::ReliableBody;
use crate::wire::packet::SetPeerIdBody;
use crate::wire::ser::Serialize;
use crate::wire::ser::VecSerializer;
use crate::wire::types::ProtocolContext;

use super::reliable_receiver::ReliableReceiver;
use super::reliable_sender::ReliableSender;
use super::split_receiver::SplitReceiver;
use super::split_sender::SplitSender;

use std::collections::VecDeque;
use std::net::SocketAddr;
use std::time::Duration;
use std::time::Instant;

// How long to accept peer_id == 0 from a client after sending set_peer_id
const INEXISTENT_PEER_ID_GRACE: Duration = Duration::from_secs(20);

#[derive(thiserror::Error, Debug)]
pub enum PeerError {
    #[error("Peer sent disconnect packet")]
    PeerSentDisconnect,
    #[error("Socket Closed")]
    SocketClosed,
    #[error("Controller Closed")]
    ControllerClosed,
    #[error("Internal Peer error")]
    InternalPeerError,
}

pub type ChannelNum = u8;
pub type FullSeqNum = u64;

// This is held by the driver that interfaces with the MinetestSocket
pub struct Peer {
    remote_addr: SocketAddr,
    remote_is_server: bool,
    /// TODO(paradust): Add backpressure
    send: UnboundedSender<Command>,
    recv: UnboundedReceiver<Result<Command>>,
}

impl Peer {
    pub fn remote_addr(&self) -> SocketAddr {
        self.remote_addr
    }

    pub fn is_server(&self) -> bool {
        self.remote_is_server
    }

    /// Send command to peer
    /// If this fails, the peer has disconnected.
    pub async fn send(&self, command: Command) -> Result<()> {
        self.send.send(command)?;
        Ok(())
    }

    /// Receive command from the peer
    /// Returns (channel, reliable flag, Command)
    /// If this fails, the peer is disconnected.
    pub async fn recv(&mut self) -> anyhow::Result<Command> {
        match self.recv.recv().await {
            Some(result) => result,
            None => bail!(PeerError::InternalPeerError),
        }
    }
}

// This is owned by the MinetestSocket
pub struct PeerIO {
    relay: UnboundedSender<SocketToPeer>,
}

pub fn new_peer(
    remote_addr: SocketAddr,
    remote_is_server: bool,
    peer_to_socket: UnboundedSender<PeerToSocket>,
) -> (Peer, PeerIO) {
    let (peer_send_tx, peer_send_rx) = unbounded_channel();
    let (peer_recv_tx, peer_recv_rx) = unbounded_channel();
    let (relay_tx, relay_rx) = unbounded_channel();

    let socket_peer = Peer {
        remote_addr,
        remote_is_server,
        send: peer_send_tx,
        recv: peer_recv_rx,
    };
    let socket_peer_io = PeerIO { relay: relay_tx };
    let socket_peer_runner = PeerRunner {
        remote_addr,
        remote_is_server,
        recv_context: ProtocolContext::latest_for_receive(remote_is_server),
        send_context: ProtocolContext::latest_for_send(remote_is_server),
        connect_time: Instant::now(),
        remote_peer_id: 0,
        local_peer_id: 0,
        from_socket: relay_rx,
        from_controller: peer_send_rx,
        to_controller: peer_recv_tx.clone(),
        to_socket: peer_to_socket,
        channels: vec![
            Channel::new(remote_is_server, peer_recv_tx.clone()),
            Channel::new(remote_is_server, peer_recv_tx.clone()),
            Channel::new(remote_is_server, peer_recv_tx.clone()),
        ],
        rng: StdRng::from_entropy(),
        now: Instant::now(),
        last_received: Instant::now(),
    };
    tokio::spawn(async move { socket_peer_runner.run().await });
    (socket_peer, socket_peer_io)
}

impl PeerIO {
    /// Parse the packet and send it to the runner
    /// Called by the MinetestSocket when a packet arrives for us
    ///
    pub fn send(&mut self, data: &[u8]) {
        // TODO: Add backpressure
        let _ = self.relay.send(SocketToPeer::Received(data.to_vec()));
    }
}

struct Channel {
    unreliable_out: VecDeque<InnerBody>,

    reliable_in: ReliableReceiver,
    reliable_out: ReliableSender,

    split_in: SplitReceiver,
    split_out: SplitSender,

    to_controller: UnboundedSender<Result<Command>>,
    now: Instant,
    recv_context: ProtocolContext,
    send_context: ProtocolContext,
}

impl Channel {
    pub fn new(remote_is_server: bool, to_controller: UnboundedSender<Result<Command>>) -> Self {
        Self {
            unreliable_out: VecDeque::new(),
            reliable_in: ReliableReceiver::new(),
            reliable_out: ReliableSender::new(),
            split_in: SplitReceiver::new(),
            split_out: SplitSender::new(),
            to_controller,
            now: Instant::now(),
            recv_context: ProtocolContext::latest_for_receive(remote_is_server),
            send_context: ProtocolContext::latest_for_send(remote_is_server),
        }
    }

    pub fn update_now(&mut self, now: &Instant) {
        self.now = *now;
    }

    pub fn update_context(
        &mut self,
        recv_context: &ProtocolContext,
        send_context: &ProtocolContext,
    ) {
        self.recv_context = *recv_context;
        self.send_context = *send_context;
    }

    /// Process a packet received from remote
    /// Possibly dispatching one or more Commands
    pub async fn process(&mut self, body: PacketBody) -> anyhow::Result<()> {
        match body {
            PacketBody::Reliable(rb) => self.process_reliable(rb).await?,
            PacketBody::Inner(ib) => self.process_inner(ib).await?,
        }
        Ok(())
    }

    pub async fn process_reliable(&mut self, body: ReliableBody) -> anyhow::Result<()> {
        self.reliable_in.push(body);
        while let Some(inner) = self.reliable_in.pop() {
            self.process_inner(inner).await?;
        }
        Ok(())
    }

    pub async fn process_inner(&mut self, body: InnerBody) -> anyhow::Result<()> {
        match body {
            InnerBody::Control(body) => self.process_control(body),
            InnerBody::Original(body) => self.process_command(body.command).await,
            InnerBody::Split(body) => {
                if let Some(payload) = self.split_in.push(self.now, body)? {
                    let mut buf = Deserializer::new(self.recv_context, &payload);
                    let command = Command::deserialize(&mut buf)?;
                    self.process_command(command).await;
                }
            }
        }
        Ok(())
    }

    pub fn process_control(&mut self, body: ControlBody) {
        match body {
            ControlBody::Ack(ack) => {
                self.reliable_out.process_ack(ack);
            }
            // Everything else is handled one level up
            _ => (),
        }
    }

    pub async fn process_command(&mut self, command: Command) {
        match self.to_controller.send(Ok(command)) {
            Ok(_) => (),
            Err(e) => panic!("Unexpected command channel shutdown: {:?}", e),
        }
    }

    /// Send command to remote
    pub fn send(&mut self, reliable: bool, command: Command) -> anyhow::Result<()> {
        let bodies = self.split_out.push(self.send_context, command)?;
        for body in bodies.into_iter() {
            self.send_inner(reliable, body);
        }
        Ok(())
    }

    pub fn send_inner(&mut self, reliable: bool, body: InnerBody) {
        if reliable {
            self.reliable_out.push(body);
        } else {
            self.unreliable_out.push_back(body);
        }
    }

    /// Check if the channel has anything ready to send.
    pub fn next_send(&mut self, now: Instant) -> Option<PacketBody> {
        match self.unreliable_out.pop_front() {
            Some(body) => return Some(PacketBody::Inner(body)),
            None => (),
        };
        match self.reliable_out.pop(now) {
            Some(body) => return Some(body),
            None => (),
        }
        None
    }

    /// Only call after exhausting next_send()
    pub fn next_timeout(&mut self) -> Option<Instant> {
        self.reliable_out.next_timeout()
    }
}

#[derive(Debug)]
pub enum SocketToPeer {
    /// TODO(paradust): Use buffer pool
    Received(Vec<u8>),
}

#[derive(Debug)]
pub enum PeerToSocket {
    // Acks are sent with higher priority
    SendImmediate(SocketAddr, Vec<u8>),
    Send(SocketAddr, Vec<u8>),
    PeerIsDisconnected(SocketAddr),
}

pub struct PeerRunner {
    remote_addr: SocketAddr,
    remote_is_server: bool,
    connect_time: Instant,
    recv_context: ProtocolContext,
    send_context: ProtocolContext,

    // TODO(paradust): These should have a limited size, and close connection on overflow.
    from_socket: UnboundedReceiver<SocketToPeer>,
    to_socket: UnboundedSender<PeerToSocket>,

    // TODO(paradust): These should have backpressure
    from_controller: UnboundedReceiver<Command>,
    to_controller: UnboundedSender<Result<Command>>,

    // This is the peer id in the Minetest protocol
    // Minetest's server uses these to keep track of clients, but we use the remote_addr.
    // Just use a randomly generated, not necessarily unique value, and keep it consistent.
    // Special ids: 0 is unassigned, and 1 for the server.
    remote_peer_id: PeerId,
    local_peer_id: PeerId,
    rng: StdRng,

    channels: Vec<Channel>,

    // Updated once per wakeup, to limit number of repeated syscalls
    now: Instant,

    // Time last packet was received. Used to timeout connection.
    last_received: Instant,
}

impl PeerRunner {
    pub fn update_now(&mut self) {
        self.now = Instant::now();
        for num in 0..=2 {
            self.channels[num].update_now(&self.now);
        }
    }

    pub fn serialize_for_send(&mut self, channel: u8, body: PacketBody) -> Result<Vec<u8>> {
        let pkt = Packet::new(self.local_peer_id, channel, body);
        let mut serializer = VecSerializer::new(self.send_context, 512);
        Packet::serialize(&pkt, &mut serializer)?;
        Ok(serializer.take())
    }

    pub async fn send_raw(&mut self, channel: u8, body: PacketBody) -> Result<()> {
        let raw = self.serialize_for_send(channel, body)?;
        self.to_socket
            .send(PeerToSocket::Send(self.remote_addr, raw))?;
        Ok(())
    }

    pub async fn send_raw_priority(&mut self, channel: u8, body: PacketBody) -> Result<()> {
        let raw = self.serialize_for_send(channel, body)?;
        self.to_socket
            .send(PeerToSocket::SendImmediate(self.remote_addr, raw))?;
        Ok(())
    }

    pub async fn run(mut self) {
        if let Err(err) = self.run_inner().await {
            // Top-level error handling for a peer.
            // If an error gets to this point, the peer is toast.
            // Send a disconnect packet, and a remove peer request to the socket
            // These channels might already be dead, so ignore any errors.
            let disconnected_cleanly: bool = if let Some(e) = err.downcast_ref::<PeerError>() {
                matches!(e, PeerError::PeerSentDisconnect)
            } else {
                false
            };
            if !disconnected_cleanly {
                // Send a disconnect packet
                let _ = self
                    .send_raw(0, (ControlBody::Disconnect).into_inner().into_unreliable())
                    .await;
            }
            let _ = self
                .to_socket
                .send(PeerToSocket::PeerIsDisconnected(self.remote_addr));

            // Tell the controller why we died
            let _ = self.to_controller.send(Err(err));
        }
    }

    pub async fn run_inner(&mut self) -> anyhow::Result<()> {
        self.update_now();

        // 10 years ought to be enough
        let never = self.now + Duration::from_secs(315576000);

        loop {
            // Before select, make sure everything ready to send has been sent,
            // and compute a resend timeout.
            let mut next_wakeup = never;
            for num in 0..=2 {
                loop {
                    let pkt = self.channels[num].next_send(self.now);
                    match pkt {
                        Some(body) => self.send_raw(num as u8, body).await?,
                        None => break,
                    }
                }
                if let Some(timeout) = self.channels[num].next_timeout() {
                    next_wakeup = std::cmp::min(next_wakeup, timeout);
                }
            }

            // rust-analyzer chokes on code inside select!, so keep it to a minimum.
            tokio::select! {
                msg = self.from_socket.recv() => self.handle_from_socket(msg).await?,
                command = self.from_controller.recv() => self.handle_from_controller(command).await?,
                _ = tokio::time::sleep_until(next_wakeup.into()) => self.handle_timeout().await?,
            }
        }
    }

    async fn handle_from_socket(&mut self, msg: Option<SocketToPeer>) -> anyhow::Result<()> {
        self.update_now();
        let msg = match msg {
            Some(msg) => msg,
            None => bail!(PeerError::SocketClosed),
        };
        match msg {
            SocketToPeer::Received(buf) => {
                let mut deser = Deserializer::new(self.recv_context, &buf);
                let pkt = Packet::deserialize(&mut deser)?;
                self.last_received = self.now;
                self.process_packet(pkt).await?;
            }
        };
        Ok(())
    }

    async fn handle_from_controller(&mut self, command: Option<Command>) -> anyhow::Result<()> {
        self.update_now();
        let command = match command {
            Some(command) => command,
            None => bail!(PeerError::ControllerClosed),
        };
        self.sniff_hello(&command);

        self.send_command(command).await?;
        Ok(())
    }

    async fn handle_timeout(&mut self) -> anyhow::Result<()> {
        self.update_now();
        self.process_timeouts().await?;
        Ok(())
    }

    // Process a packet received over network
    async fn process_packet(&mut self, pkt: Packet) -> anyhow::Result<()> {
        if !self.remote_is_server {
            // We're the server, assign the remote a peer_id.
            if self.remote_peer_id == 0 {
                // Assign a peer id
                self.local_peer_id = 1;
                self.remote_peer_id = self.rng.gen_range(2..65535);

                // Tell the client about it
                let set_peer_id = SetPeerIdBody::new(self.remote_peer_id).into_inner();
                self.channels[0].send_inner(true, set_peer_id);
            }
            if pkt.sender_peer_id == 0 {
                if self.now > self.connect_time + INEXISTENT_PEER_ID_GRACE {
                    // Malformed, ignore.
                    println!("Ignoring peer_id 0 packet");
                    return Ok(());
                }
            } else if pkt.sender_peer_id != self.remote_peer_id {
                // Malformed. Ignore
                println!("Invalid peer_id on packet");
                return Ok(());
            }
        } else {
            if pkt.sender_peer_id != 1 {
                println!("Server sending from wrong peer id");
                return Ok(());
            }
        }

        // Send ack right away
        if let Some(rb) = pkt.as_reliable() {
            self.send_ack(pkt.channel, rb).await?;
        }

        // Certain control packets need to be handled at the
        // top-level (here) instead of in a channel.
        // With the exception of disconnect, control packets must still be
        // passed to the channel, because they may have reliable bodies
        // (and affect seqnums)
        if let Some(control) = pkt.as_control() {
            match control {
                ControlBody::Ack(_) => {
                    // Handled by channel
                }
                ControlBody::SetPeerId(set_peer_id) => {
                    if !self.remote_is_server {
                        bail!("Invalid set_peer_id received from client");
                    } else {
                        if self.local_peer_id == 0 {
                            self.local_peer_id = set_peer_id.peer_id;
                        } else if self.local_peer_id != set_peer_id.peer_id {
                            bail!("Peer id mismatch in duplicate SetPeerId");
                        }
                    }
                }
                ControlBody::Ping => {
                    // no-op. Packet already updated timeout
                }
                ControlBody::Disconnect => bail!(PeerError::PeerSentDisconnect),
            }
        }
        // If this is a HELLO packet, sniff it to set our protocol context.
        if let Some(command) = pkt.body.command_ref() {
            self.sniff_hello(command);
        }

        self.channels[pkt.channel as usize].process(pkt.body).await
    }

    fn sniff_hello(&mut self, command: &Command) {
        match command {
            Command::ToClient(ToClientCommand::Hello(spec)) => {
                self.update_context(spec.serialization_ver, spec.proto_ver);
            }
            _ => (),
        }
    }

    fn update_context(&mut self, ser_fmt: u8, protocol_version: u16) {
        self.recv_context.protocol_version = protocol_version;
        self.recv_context.ser_fmt = ser_fmt;
        self.send_context.protocol_version = protocol_version;
        self.send_context.ser_fmt = ser_fmt;
        for num in 0..=2 {
            self.channels[num].update_context(&self.recv_context, &self.send_context);
        }
    }

    /// If this is a reliable packet, send an ack right away
    /// using a higher-priority out-of-band channel.
    async fn send_ack(&mut self, channel: u8, rb: &ReliableBody) -> anyhow::Result<()> {
        let ack = AckBody::new(rb.seqnum).into_inner().into_unreliable();
        self.send_raw_priority(channel, ack).await?;
        Ok(())
    }

    /// Send command to remote
    async fn send_command(&mut self, command: Command) -> anyhow::Result<()> {
        let channel = command.default_channel();
        let reliable = command.default_reliability();
        assert!((0..=2).contains(&channel));
        self.channels[channel as usize].send(reliable, command)
    }

    async fn process_timeouts(&mut self) -> anyhow::Result<()> {
        Ok(())
    }
}
