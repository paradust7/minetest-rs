use anyhow::bail;

use super::command::Command;
use super::deser::Deserialize;
use super::deser::DeserializeError;
use super::deser::DeserializeResult;
use super::deser::Deserializer;
use super::ser::Serialize;
use super::ser::SerializeResult;
use super::ser::Serializer;

pub const PROTOCOL_ID: u32 = 0x4f457403;
pub const LATEST_PROTOCOL_VERSION: u16 = 41;
pub const MAX_PACKET_SIZE: usize = 512;
pub const SEQNUM_INITIAL: u16 = 65500;
pub const PACKET_HEADER_SIZE: usize = 7;
pub const RELIABLE_HEADER_SIZE: usize = 3;
pub const SPLIT_HEADER_SIZE: usize = 7;
pub const MAX_ORIGINAL_BODY_SIZE: usize =
    MAX_PACKET_SIZE - PACKET_HEADER_SIZE - RELIABLE_HEADER_SIZE;
pub const MAX_SPLIT_BODY_SIZE: usize = MAX_ORIGINAL_BODY_SIZE - SPLIT_HEADER_SIZE;

pub type PeerId = u16;

#[derive(Debug, Clone, PartialEq)]
pub struct AckBody {
    pub seqnum: u16,
}

impl AckBody {
    pub fn new(seqnum: u16) -> Self {
        AckBody { seqnum }
    }
    pub fn into_inner(self) -> InnerBody {
        InnerBody::Control(ControlBody::Ack(self))
    }
}

impl Serialize for AckBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        u16::serialize(&self.seqnum, ser)
    }
}

impl Deserialize for AckBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(Self {
            seqnum: u16::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SetPeerIdBody {
    pub peer_id: u16,
}

impl SetPeerIdBody {
    pub fn new(peer_id: u16) -> Self {
        Self { peer_id }
    }

    pub fn into_inner(self) -> InnerBody {
        InnerBody::Control(ControlBody::SetPeerId(self))
    }
}

impl Serialize for SetPeerIdBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        u16::serialize(&self.peer_id, ser)
    }
}

impl Deserialize for SetPeerIdBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(Self {
            peer_id: u16::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlBody {
    Ack(AckBody),
    SetPeerId(SetPeerIdBody),
    Ping,
    Disconnect,
}

impl ControlBody {
    pub fn into_inner(self) -> InnerBody {
        InnerBody::Control(self)
    }
}

impl Serialize for ControlBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use ControlBody::*;
        let control_type = match self {
            Ack(_) => 0,
            SetPeerId(_) => 1,
            Ping => 2,
            Disconnect => 3,
        };
        u8::serialize(&control_type, ser)?;
        match self {
            Ack(body) => Serialize::serialize(body, ser)?,
            SetPeerId(body) => Serialize::serialize(body, ser)?,
            Ping => (),
            Disconnect => (),
        };
        Ok(())
    }
}

impl Deserialize for ControlBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use ControlBody::*;
        let control_type = u8::deserialize(deser)?;
        match control_type {
            0 => Ok(Ack(Deserialize::deserialize(deser)?)),
            1 => Ok(SetPeerId(Deserialize::deserialize(deser)?)),
            2 => Ok(Ping),
            3 => Ok(Disconnect),
            _ => bail!(DeserializeError::InvalidValue(String::from(
                "Invalid control_type in ControlBody",
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OriginalBody {
    pub command: Command,
}

impl Serialize for OriginalBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&self.command, ser)
    }
}

impl Deserialize for OriginalBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(OriginalBody {
            command: Deserialize::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SplitBody {
    pub seqnum: u16,
    pub chunk_count: u16,
    pub chunk_num: u16,
    pub chunk_data: Vec<u8>,
}

impl Serialize for SplitBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&self.seqnum, ser)?;
        Serialize::serialize(&self.chunk_count, ser)?;
        Serialize::serialize(&self.chunk_num, ser)?;
        ser.write_bytes(&self.chunk_data)?;
        Ok(())
    }
}

impl Deserialize for SplitBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(SplitBody {
            seqnum: Deserialize::deserialize(deser)?,
            chunk_count: Deserialize::deserialize(deser)?,
            chunk_num: Deserialize::deserialize(deser)?,
            chunk_data: Vec::from(deser.take_all()),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReliableBody {
    pub seqnum: u16,
    pub inner: InnerBody,
}

impl Serialize for ReliableBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let packet_type: u8 = 3;
        Serialize::serialize(&packet_type, ser)?;
        Serialize::serialize(&self.seqnum, ser)?;
        Serialize::serialize(&self.inner, ser)?;
        Ok(())
    }
}

impl Deserialize for ReliableBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let packet_type: u8 = Deserialize::deserialize(deser)?;
        if packet_type != 3 {
            bail!(DeserializeError::InvalidValue(
                "Invalid packet_type for ReliableBody".to_string(),
            ))
        }
        Ok(ReliableBody {
            seqnum: Deserialize::deserialize(deser)?,
            inner: Deserialize::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InnerBody {
    Control(ControlBody),
    Original(OriginalBody),
    Split(SplitBody),
}

impl InnerBody {
    pub fn into_reliable(self, seqnum: u16) -> PacketBody {
        PacketBody::Reliable(ReliableBody {
            seqnum: seqnum,
            inner: self,
        })
    }

    pub fn into_unreliable(self) -> PacketBody {
        PacketBody::Inner(self)
    }

    /// Get a reference to the Command this body contains, if any.
    /// If this is part of a split packet, None will be returned
    /// even though there is a fragment of a Command inside.
    pub fn command(&self) -> Option<&Command> {
        match self {
            InnerBody::Control(_) => None,
            InnerBody::Original(body) => Some(&body.command),
            InnerBody::Split(_) => None,
        }
    }
}

impl Serialize for InnerBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use InnerBody::*;
        let packet_type: u8 = match self {
            Control(..) => 0,
            Original(..) => 1,
            Split(..) => 2,
        };
        u8::serialize(&packet_type, ser)?;
        match self {
            Control(b) => Serialize::serialize(b, ser),
            Original(b) => Serialize::serialize(b, ser),
            Split(b) => Serialize::serialize(b, ser),
        }
    }
}

impl Deserialize for InnerBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use InnerBody::*;
        let packet_type = u8::deserialize(deser)?;
        match packet_type {
            0 => Ok(Control(Deserialize::deserialize(deser)?)),
            1 => Ok(Original(Deserialize::deserialize(deser)?)),
            2 => Ok(Split(Deserialize::deserialize(deser)?)),
            _ => bail!(DeserializeError::InvalidPacketKind(packet_type)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PacketBody {
    Reliable(ReliableBody),
    Inner(InnerBody),
}

impl PacketBody {
    pub fn inner(&self) -> &InnerBody {
        match self {
            PacketBody::Reliable(body) => &body.inner,
            PacketBody::Inner(inner) => &inner,
        }
    }
}

impl Serialize for PacketBody {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use PacketBody::*;
        // Both ReliableBody and InnerBody will emit their own packet type.
        match self {
            Reliable(body) => Serialize::serialize(body, ser),
            Inner(inner) => Serialize::serialize(inner, ser),
        }
    }
}

impl Deserialize for PacketBody {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use PacketBody::*;
        // Both ReliableBody and InnerBody expect to consume the packet type tag.
        // So only peek it.
        let packet_type = deser.peek(1)?[0];
        match packet_type {
            3 => Ok(Reliable(Deserialize::deserialize(deser)?)),
            _ => Ok(Inner(Deserialize::deserialize(deser)?)),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Packet {
    pub protocol_id: u32,
    pub sender_peer_id: PeerId,
    pub channel: u8,
    pub body: PacketBody,
}

impl Packet {
    pub fn new(sender_peer_id: PeerId, channel: u8, body: PacketBody) -> Self {
        Self {
            protocol_id: PROTOCOL_ID,
            sender_peer_id,
            channel,
            body,
        }
    }

    pub fn inner(&self) -> &InnerBody {
        self.body.inner()
    }

    pub fn as_control(&self) -> Option<&ControlBody> {
        match self.inner() {
            InnerBody::Control(control) => Some(&control),
            InnerBody::Original(_) => None,
            InnerBody::Split(_) => None,
        }
    }
}

impl Serialize for Packet {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&self.protocol_id, ser)?;
        Serialize::serialize(&self.sender_peer_id, ser)?;
        Serialize::serialize(&self.channel, ser)?;
        Serialize::serialize(&self.body, ser)?;
        Ok(())
    }
}

impl Deserialize for Packet {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let pkt = Packet {
            protocol_id: Deserialize::deserialize(deser)?,
            sender_peer_id: Deserialize::deserialize(deser)?,
            channel: Deserialize::deserialize(deser)?,
            body: Deserialize::deserialize(deser)?,
        };
        if pkt.protocol_id != PROTOCOL_ID {
            bail!(DeserializeError::InvalidProtocolId(pkt.protocol_id))
        }
        if !(0..=2).contains(&pkt.channel) {
            bail!(DeserializeError::InvalidChannel(pkt.channel))
        }
        Ok(pkt)
    }
}
