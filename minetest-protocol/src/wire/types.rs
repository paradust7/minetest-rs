//! Minetest data types used inside of Commands / Packets.
//!
//! Derive macros MinetestSerialize and MinetestDeserialize are used to
//! produce ser/deser methods for many of the structs below. The order of
//! the fields inside the struct determines the order in which they are
//! serialized/deserialized, so be careful modifying anything below.
//! Their serialized representation must stay the same.
//!
//! NOTE: The derive macros currently do not work on structs with generic parameters.
//!
//! TODO(paradust): Having an assert!-like macro that generates Serialize/Deserialize
//! errors instead of aborts may be helpful for cleaning this up.
use anyhow::bail;
use minetest_protocol_derive::MinetestDeserialize;
use minetest_protocol_derive::MinetestSerialize;

use crate::itos;

use super::deser::Deserialize;
use super::deser::DeserializeError;
use super::deser::DeserializeResult;
use super::deser::Deserializer;
use super::packet::LATEST_PROTOCOL_VERSION;
use super::packet::SER_FMT_HIGHEST_READ;
use super::ser::Serialize;
use super::ser::SerializeError;
use super::ser::SerializeResult;
use super::ser::Serializer;
use super::ser::VecSerializer;
use super::util::compress_zlib;
use super::util::decompress_zlib;
use super::util::deserialize_json_string_if_needed;
use super::util::next_word;
use super::util::serialize_json_string_if_needed;
use super::util::skip_whitespace;
use super::util::split_by_whitespace;
use super::util::stoi;
use super::util::zstd_compress;
use super::util::zstd_decompress;
use std::ops::Deref;
use std::ops::DerefMut;
use std::ops::Div;
use std::ops::Mul;

#[allow(non_camel_case_types)]
pub type s8 = i8;

#[allow(non_camel_case_types)]
pub type s16 = i16;

#[allow(non_camel_case_types)]
pub type s32 = i32;

pub type CommandId = u8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandDirection {
    ToClient,
    ToServer,
}

impl CommandDirection {
    pub fn for_send(remote_is_server: bool) -> Self {
        use CommandDirection::*;
        match remote_is_server {
            true => ToServer,
            false => ToClient,
        }
    }

    pub fn for_receive(remote_is_server: bool) -> Self {
        Self::for_send(remote_is_server).flip()
    }

    pub fn flip(&self) -> Self {
        use CommandDirection::*;
        match self {
            ToClient => ToServer,
            ToServer => ToClient,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProtocolContext {
    pub dir: CommandDirection,
    pub protocol_version: u16,
    pub ser_fmt: u8,
}

impl ProtocolContext {
    pub fn latest_for_receive(remote_is_server: bool) -> Self {
        Self {
            dir: CommandDirection::for_receive(remote_is_server),
            protocol_version: LATEST_PROTOCOL_VERSION,
            ser_fmt: SER_FMT_HIGHEST_READ,
        }
    }

    pub fn latest_for_send(remote_is_server: bool) -> Self {
        Self {
            dir: CommandDirection::for_send(remote_is_server),
            protocol_version: LATEST_PROTOCOL_VERSION,
            ser_fmt: SER_FMT_HIGHEST_READ,
        }
    }
}

/// Rust String's must be valid UTF8. But Minetest's strings can contain arbitrary
/// binary data. The only way to store arbitrary bytes is with something like Vec<u8>,
/// which is not String-like. This provides a String-like alternative, that looks nice
/// in debug output.
#[derive(Clone, PartialEq)]
pub struct ByteString(pub Vec<u8>);

impl std::fmt::Debug for ByteString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format it as an escaped string
        std::fmt::Debug::fmt(&self.escape_ascii(), f)
    }
}

impl ByteString {
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn escape_ascii(&self) -> String {
        self.0.escape_ascii().to_string()
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl From<&[u8]> for ByteString {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

// Basic types
impl Serialize for bool {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let val: u8 = if *self { 1 } else { 0 };
        ser.write_bytes(&val.to_be_bytes()[..])
    }
}

impl Deserialize for bool {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let b = deser.take_n::<1>()?[0];
        Ok(match b {
            0 => false,
            1 => true,
            _ => bail!("Invalid bool: {}", b),
        })
    }
}

impl Serialize for u8 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for u8 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(deser.take_n::<1>()?[0])
    }
}

impl Serialize for u16 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for u16 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(u16::from_be_bytes(deser.take_n::<2>()?))
    }
}

impl Serialize for u32 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for u32 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(u32::from_be_bytes(deser.take_n::<4>()?))
    }
}

impl Serialize for u64 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for u64 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(u64::from_be_bytes(deser.take_n::<8>()?))
    }
}

impl Serialize for i8 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for i8 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(deser.take(1)?[0] as i8)
    }
}

impl Serialize for i16 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for i16 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(u16::from_be_bytes(deser.take_n::<2>()?) as i16)
    }
}

impl Serialize for i32 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for i32 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(u32::from_be_bytes(deser.take_n::<4>()?) as i32)
    }
}

impl Serialize for f32 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        ser.write_bytes(&self.to_be_bytes()[..])
    }
}

impl Deserialize for f32 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(f32::from_be_bytes(deser.take_n::<4>()?))
    }
}

impl Serialize for String {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u16::try_from(self.len())?, ser)?;
        ser.write_bytes(self.as_bytes())
    }
}

impl Deserialize for String {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let num_bytes = u16::deserialize(deser)? as usize;
        match std::str::from_utf8(deser.take(num_bytes)?) {
            Ok(s) => Ok(s.to_string()),
            Err(u) => bail!(DeserializeError::InvalidValue(u.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LongString {
    pub string: String,
}

impl Serialize for LongString {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u32::try_from(self.string.len())?, ser)?;
        ser.write_bytes(&self.string.as_bytes())
    }
}

impl Deserialize for LongString {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let num_bytes = u32::deserialize(deser)? as usize;
        match std::str::from_utf8(deser.take(num_bytes)?) {
            Ok(s) => Ok(LongString {
                string: s.to_string(),
            }),
            Err(u) => bail!(DeserializeError::InvalidValue(u.to_string())),
        }
    }
}

impl Deref for LongString {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

impl DerefMut for LongString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.string
    }
}

/// Corresponds to std::wstring in C++ land
#[derive(Debug, Clone, PartialEq)]
pub struct WString {
    pub string: String,
}

impl Deref for WString {
    type Target = String;
    fn deref(&self) -> &Self::Target {
        &self.string
    }
}

impl DerefMut for WString {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.string
    }
}

impl Serialize for WString {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let enc: Vec<u16> = self.string.encode_utf16().collect();

        Serialize::serialize(&u16::try_from(enc.len())?, ser)?;
        // TODO: This could be made more efficient.
        let mut buf: Vec<u8> = vec![0; 2 * enc.len()];
        let mut index: usize = 0;
        for codepoint in enc {
            buf[index] = (codepoint >> 8) as u8;
            buf[index + 1] = codepoint as u8;
            index += 2;
        }
        ser.write_bytes(&buf)
    }
}

impl Deserialize for WString {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let length = u16::deserialize(deser)? as usize;
        let raw = deser.take(2 * length)?;
        let mut seq: Vec<u16> = vec![0; length];
        for i in 0..length {
            seq[i] = u16::from_be_bytes(raw[2 * i..2 * i + 2].try_into().unwrap());
        }
        match String::from_utf16(&seq) {
            Ok(s) => Ok(WString { string: s }),
            Err(err) => bail!(DeserializeError::InvalidValue(err.to_string())),
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v2f {
    pub x: f32,
    pub y: f32,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, Copy, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v3f {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl v3f {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
    pub fn as_v3s32(&self) -> v3s32 {
        v3s32 {
            x: self.x.round() as i32,
            y: self.y.round() as i32,
            z: self.z.round() as i32,
        }
    }
}

impl Mul<f32> for v3f {
    type Output = v3f;
    fn mul(self, rhs: f32) -> Self::Output {
        v3f {
            x: self.x * rhs,
            y: self.y * rhs,
            z: self.z * rhs,
        }
    }
}

impl Div<f32> for v3f {
    type Output = v3f;
    fn div(self, rhs: f32) -> Self::Output {
        v3f {
            x: self.x / rhs,
            y: self.y / rhs,
            z: self.z / rhs,
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v2u32 {
    pub x: u32,
    pub y: u32,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v2s16 {
    pub x: s16,
    pub y: s16,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v3s16 {
    pub x: s16,
    pub y: s16,
    pub z: s16,
}

impl v3s16 {
    pub fn new(x: s16, y: s16, z: s16) -> Self {
        Self { x, y, z }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v2s32 {
    pub x: s32,
    pub y: s32,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct v3s32 {
    pub x: s32,
    pub y: s32,
    pub z: s32,
}

impl v3s32 {
    pub fn as_v3f(&self) -> v3f {
        v3f {
            x: self.x as f32,
            y: self.y as f32,
            z: self.z as f32,
        }
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct SColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

// Wrapped in a String (really a BinaryData16) with a 16-bit length
#[derive(Debug, Clone, PartialEq)]
pub struct Wrapped16<T> {
    pub value: T,
}

impl<T: Serialize> Serialize for Wrapped16<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let marker = ser.write_marker(2)?;
        Serialize::serialize(&self.value, ser)?;
        let wlen: u16 = u16::try_from(ser.marker_distance(&marker))?;
        ser.set_marker(marker, &wlen.to_be_bytes()[..])?;
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Wrapped16<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let wlen = u16::deserialize(deser)?;
        let mut restricted_deser = deser.slice(wlen as usize)?;
        Ok(Self {
            value: Deserialize::deserialize(&mut restricted_deser)?,
        })
    }
}

// Wrapped in a String32 (really a BinaryData32)
#[derive(Debug, Clone, PartialEq)]
pub struct Wrapped32<T> {
    pub value: T,
}

impl<T: Serialize> Serialize for Wrapped32<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let marker = ser.write_marker(4)?;
        Serialize::serialize(&self.value, ser)?;
        let wlen: u32 = u32::try_from(ser.marker_distance(&marker))?;
        ser.set_marker(marker, &wlen.to_be_bytes()[..])?;
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Wrapped32<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let wlen = u32::deserialize(deser)?;
        let mut restricted_deser = deser.slice(wlen as usize)?;
        Ok(Self {
            value: Deserialize::deserialize(&mut restricted_deser)?,
        })
    }
}

/// Binary data preceded by a U16 size
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryData16 {
    pub data: Vec<u8>,
}

impl BinaryData16 {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Serialize for BinaryData16 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u16::try_from(self.data.len())?, ser)?;
        ser.write_bytes(&self.data)?;
        Ok(())
    }
}

impl Deserialize for BinaryData16 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let num_bytes = u16::deserialize(deser)? as usize;
        Ok(BinaryData16 {
            data: Vec::from(deser.take(num_bytes)?),
        })
    }
}

/// Binary data preceded by a U32 size
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryData32 {
    pub data: Vec<u8>,
}

impl Serialize for BinaryData32 {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u32::try_from(self.data.len())?, ser)?;
        ser.write_bytes(&self.data)?;
        Ok(())
    }
}

impl Deserialize for BinaryData32 {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let num_bytes = u32::deserialize(deser)? as usize;
        Ok(BinaryData32 {
            data: Vec::from(deser.take(num_bytes)?),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixedArray<const COUNT: usize, T> {
    pub entries: [T; COUNT],
}

impl<const COUNT: usize, T: Serialize> Serialize for FixedArray<COUNT, T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        for ent in self.entries.iter() {
            Serialize::serialize(ent, ser)?;
        }
        Ok(())
    }
}

impl<const COUNT: usize, T: Deserialize> Deserialize for FixedArray<COUNT, T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let mut entries = Vec::with_capacity(COUNT);
        for _ in 0..COUNT {
            entries.push(Deserialize::deserialize(deser)?);
        }
        match entries.try_into() {
            Ok(entries) => Ok(Self { entries }),
            Err(_) => bail!(DeserializeError::InvalidValue("FixedArray bug".to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct FixedBinaryData<const COUNT: usize> {
    pub data: Vec<u8>,
}

impl<const COUNT: usize> Serialize for FixedBinaryData<COUNT> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        if COUNT != 0 && self.data.len() != COUNT {
            bail!(SerializeError::InvalidValue(format!(
                "FixedBinaryData<{}> incorrect data length {}",
                COUNT,
                self.data.len()
            )));
        }
        ser.write_bytes(&self.data[..])
    }
}

impl<const COUNT: usize> Deserialize for FixedBinaryData<COUNT> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let raw: &[u8] = if COUNT == 0 {
            deser.take_all()
        } else {
            deser.take(COUNT)?
        };
        Ok(FixedBinaryData::<COUNT> {
            data: Vec::from(raw),
        })
    }
}

/// Option is used for optional values at the end of a structure.
/// Once Option is used, all following must be Option as well.
impl<T: Serialize> Serialize for Option<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        match self {
            Some(ref v) => Serialize::serialize(v, ser),
            None => Ok(()),
        }
    }
}

impl<T: Deserialize> Deserialize for Option<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        if deser.remaining() > 0 {
            Ok(Some(T::deserialize(deser)?))
        } else {
            Ok(None)
        }
    }
}

// An Optional value controlled by a u16 size parameter.
// Unlike Option, this can appear anywhere in the message.

#[derive(Debug, Clone, PartialEq)]
pub enum Option16<T> {
    None,
    Some(T),
}
impl<T: Serialize> Serialize for Option16<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        match self {
            Option16::None => u16::serialize(&0u16, ser),
            Option16::Some(value) => {
                let mut buf = VecSerializer::new(ser.context(), 64);
                Serialize::serialize(value, &mut buf)?;
                let buf = buf.take();
                let num_bytes = u16::try_from(buf.len())?;
                u16::serialize(&num_bytes, ser)?;
                ser.write_bytes(&buf)?;
                Ok(())
            }
        }
    }
}

impl<T: Deserialize> Deserialize for Option16<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        match u16::deserialize(deser)? {
            0 => Ok(Option16::None),
            num_bytes => {
                let mut buf = deser.slice(num_bytes as usize)?;
                Ok(Option16::Some(Deserialize::deserialize(&mut buf)?))
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AddedObject {
    pub id: u16,
    pub typ: u8,
    pub init_data: Wrapped32<GenericInitData>,
}

/// This corresponds to GenericCAO::Initialize in minetest
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct GenericInitData {
    pub version: u8,
    pub name: String,
    pub is_player: bool,
    pub id: u16,
    pub position: v3f,
    pub rotation: v3f,
    pub hp: u16,
    pub messages: Array8<Wrapped32<ActiveObjectCommand>>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ActiveObjectMessage {
    pub id: u16,
    pub data: Wrapped16<ActiveObjectCommand>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ActiveObjectCommand {
    SetProperties(AOCSetProperties),
    UpdatePosition(AOCUpdatePosition),
    SetTextureMod(AOCSetTextureMod),
    SetSprite(AOCSetSprite),
    SetPhysicsOverride(AOCSetPhysicsOverride),
    SetAnimation(AOCSetAnimation),
    SetAnimationSpeed(AOCSetAnimationSpeed),
    SetBonePosition(AOCSetBonePosition),
    AttachTo(AOCAttachTo),
    Punched(AOCPunched),
    UpdateArmorGroups(AOCUpdateArmorGroups),
    SpawnInfant(AOCSpawnInfant),
    Obsolete1(AOCObsolete1),
}

const AO_CMD_SET_PROPERTIES: u8 = 0;
const AO_CMD_UPDATE_POSITION: u8 = 1;
const AO_CMD_SET_TEXTURE_MOD: u8 = 2;
const AO_CMD_SET_SPRITE: u8 = 3;
const AO_CMD_PUNCHED: u8 = 4;
const AO_CMD_UPDATE_ARMOR_GROUPS: u8 = 5;
const AO_CMD_SET_ANIMATION: u8 = 6;
const AO_CMD_SET_BONE_POSITION: u8 = 7;
const AO_CMD_ATTACH_TO: u8 = 8;
const AO_CMD_SET_PHYSICS_OVERRIDE: u8 = 9;
const AO_CMD_OBSOLETE1: u8 = 10;
const AO_CMD_SPAWN_INFANT: u8 = 11;
const AO_CMD_SET_ANIMATION_SPEED: u8 = 12;

impl ActiveObjectCommand {
    fn get_command_prefix(&self) -> u8 {
        match self {
            ActiveObjectCommand::SetProperties(_) => AO_CMD_SET_PROPERTIES,
            ActiveObjectCommand::UpdatePosition(_) => AO_CMD_UPDATE_POSITION,
            ActiveObjectCommand::SetTextureMod(_) => AO_CMD_SET_TEXTURE_MOD,
            ActiveObjectCommand::SetSprite(_) => AO_CMD_SET_SPRITE,
            ActiveObjectCommand::SetPhysicsOverride(_) => AO_CMD_SET_PHYSICS_OVERRIDE,
            ActiveObjectCommand::SetAnimation(_) => AO_CMD_SET_ANIMATION,
            ActiveObjectCommand::SetAnimationSpeed(_) => AO_CMD_SET_ANIMATION_SPEED,
            ActiveObjectCommand::SetBonePosition(_) => AO_CMD_SET_BONE_POSITION,
            ActiveObjectCommand::AttachTo(_) => AO_CMD_ATTACH_TO,
            ActiveObjectCommand::Punched(_) => AO_CMD_PUNCHED,
            ActiveObjectCommand::UpdateArmorGroups(_) => AO_CMD_UPDATE_ARMOR_GROUPS,
            ActiveObjectCommand::SpawnInfant(_) => AO_CMD_SPAWN_INFANT,
            ActiveObjectCommand::Obsolete1(_) => AO_CMD_OBSOLETE1,
        }
    }
}

impl Serialize for ActiveObjectCommand {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        u8::serialize(&self.get_command_prefix(), ser)?;
        match self {
            ActiveObjectCommand::SetProperties(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::UpdatePosition(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetTextureMod(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetSprite(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetPhysicsOverride(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetAnimation(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetAnimationSpeed(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SetBonePosition(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::AttachTo(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::Punched(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::UpdateArmorGroups(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::SpawnInfant(v) => Serialize::serialize(v, ser)?,
            ActiveObjectCommand::Obsolete1(v) => Serialize::serialize(v, ser)?,
        }
        Ok(())
    }
}

impl Deserialize for ActiveObjectCommand {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use ActiveObjectCommand::*;
        let cmd = u8::deserialize(deser)?;
        Ok(match cmd {
            AO_CMD_SET_PROPERTIES => SetProperties(Deserialize::deserialize(deser)?),
            AO_CMD_UPDATE_POSITION => UpdatePosition(Deserialize::deserialize(deser)?),
            AO_CMD_SET_TEXTURE_MOD => SetTextureMod(Deserialize::deserialize(deser)?),
            AO_CMD_SET_SPRITE => SetSprite(Deserialize::deserialize(deser)?),
            AO_CMD_PUNCHED => Punched(Deserialize::deserialize(deser)?),
            AO_CMD_UPDATE_ARMOR_GROUPS => UpdateArmorGroups(Deserialize::deserialize(deser)?),
            AO_CMD_SET_ANIMATION => SetAnimation(Deserialize::deserialize(deser)?),
            AO_CMD_SET_BONE_POSITION => SetBonePosition(Deserialize::deserialize(deser)?),
            AO_CMD_ATTACH_TO => AttachTo(Deserialize::deserialize(deser)?),
            AO_CMD_SET_PHYSICS_OVERRIDE => SetPhysicsOverride(Deserialize::deserialize(deser)?),
            AO_CMD_OBSOLETE1 => Obsolete1(Deserialize::deserialize(deser)?),
            AO_CMD_SPAWN_INFANT => SpawnInfant(Deserialize::deserialize(deser)?),
            AO_CMD_SET_ANIMATION_SPEED => SetAnimationSpeed(Deserialize::deserialize(deser)?),
            _ => bail!("ActiveObjectCommand: Invalid cmd={}", cmd),
        })
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetProperties {
    pub newprops: ObjectProperties,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ObjectProperties {
    pub version: u8, // must be 4
    pub hp_max: u16,
    pub physical: bool,
    pub _unused: u32,
    pub collision_box: aabb3f,
    pub selection_box: aabb3f,
    pub pointable: bool,
    pub visual: String,
    pub visual_size: v3f,
    pub textures: Array16<String>,
    pub spritediv: v2s16,
    pub initial_sprite_basepos: v2s16,
    pub is_visible: bool,
    pub makes_footstep_sound: bool,
    pub automatic_rotate: f32,
    pub mesh: String,
    pub colors: Array16<SColor>,
    pub collide_with_objects: bool,
    pub stepheight: f32,
    pub automatic_face_movement_dir: bool,
    pub automatic_face_movement_dir_offset: f32,
    pub backface_culling: bool,
    pub nametag: String,
    pub nametag_color: SColor,
    pub automatic_face_movement_max_rotation_per_sec: f32,
    pub infotext: String,
    pub wield_item: String,
    pub glow: s8,
    pub breath_max: u16,
    pub eye_height: f32,
    pub zoom_fov: f32,
    pub use_texture_alpha: bool,
    pub damage_texture_modifier: Option<String>,
    pub shaded: Option<bool>,
    pub show_on_minimap: Option<bool>,
    pub nametag_bgcolor: Option<SColor>,
    pub rotate_selectionbox: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCUpdatePosition {
    pub position: v3f,
    pub velocity: v3f,
    pub acceleration: v3f,
    pub rotation: v3f,
    pub do_interpolate: bool,
    pub is_end_position: bool,
    pub update_interval: f32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetTextureMod {
    pub modifier: String,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetSprite {
    pub base_pos: v2s16,
    pub anum_num_frames: u16,
    pub anim_frame_length: f32,
    pub select_horiz_by_yawpitch: bool,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetPhysicsOverride {
    pub override_speed: f32,
    pub override_jump: f32,
    pub override_gravity: f32,
    pub not_sneak: bool,
    pub not_sneak_glitch: bool,
    pub not_new_move: bool,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetAnimation {
    pub range: v2f, // this is always casted to v2s32 by minetest for some reason
    pub speed: f32,
    pub blend: f32,
    pub no_loop: bool,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetAnimationSpeed {
    pub speed: f32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSetBonePosition {
    pub bone: String,
    pub position: v3f,
    pub rotation: v3f,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCAttachTo {
    pub parent_id: s16,
    pub bone: String,
    pub position: v3f,
    pub rotation: v3f,
    pub force_visible: bool,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCPunched {
    pub hp: u16,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCUpdateArmorGroups {
    // name -> rating
    pub ratings: Array16<Pair<String, s16>>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCSpawnInfant {
    pub child_id: u16,
    pub typ: u8,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AOCObsolete1 {}

/// An array of items with no specified length.
/// The length is determined by buffer end.
#[derive(Debug, Clone, PartialEq)]
pub struct Array0<T> {
    pub vec: Vec<T>,
}

impl<T: Serialize> Serialize for Array0<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        for v in self.vec.iter() {
            Serialize::serialize(v, ser)?;
        }
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Array0<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let mut vec: Vec<T> = Vec::new();
        while deser.remaining() > 0 {
            vec.push(<T>::deserialize(deser)?);
        }
        Ok(Array0::<T> { vec: vec })
    }
}

/// An array of items with a u8 length prefix
#[derive(Debug, Clone, PartialEq)]
pub struct Array8<T> {
    pub vec: Vec<T>,
}

impl<T: Serialize> Serialize for Array8<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u8::try_from(self.vec.len())?, ser)?;
        for v in self.vec.iter() {
            Serialize::serialize(v, ser)?;
        }
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Array8<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let length = u8::deserialize(deser)? as usize;
        let mut vec: Vec<T> = Vec::with_capacity(length);
        for _ in 0..length {
            vec.push(<T>::deserialize(deser)?);
        }
        Ok(Array8::<T> { vec: vec })
    }
}

/// An array of items with a u16 length prefix
#[derive(Debug, Clone, PartialEq)]
pub struct Array16<T> {
    pub vec: Vec<T>,
}

impl<T> Array16<T> {
    pub fn len(&self) -> usize {
        self.vec.len()
    }
}

impl<T: Serialize> Serialize for Array16<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u16::try_from(self.vec.len())?, ser)?;
        for v in self.vec.iter() {
            Serialize::serialize(v, ser)?;
        }
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Array16<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let length = u16::deserialize(deser)? as usize;
        let mut vec: Vec<T> = Vec::with_capacity(length);
        for _ in 0..length {
            vec.push(<T>::deserialize(deser)?);
        }
        Ok(Array16::<T> { vec: vec })
    }
}

/// An array of items with a u32 length prefix
#[derive(Debug, Clone, PartialEq)]
pub struct Array32<T> {
    pub vec: Vec<T>,
}

impl<T: Serialize> Serialize for Array32<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&u32::try_from(self.vec.len())?, ser)?;
        for v in self.vec.iter() {
            Serialize::serialize(v, ser)?;
        }
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for Array32<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let length = u32::deserialize(deser)? as usize;
        // Sanity check to prevent memory DoS
        if length > deser.remaining() {
            bail!(DeserializeError::InvalidValue(
                "Array32 length too long".to_string(),
            ));
        }
        let mut vec: Vec<T> = Vec::with_capacity(length);
        for _ in 0..length {
            vec.push(<T>::deserialize(deser)?);
        }
        Ok(Array32::<T> { vec: vec })
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct MediaFileData {
    pub name: String,
    pub data: BinaryData32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct MediaAnnouncement {
    pub name: String,
    pub sha1_base64: String,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct SkyColor {
    pub day_sky: SColor,
    pub day_horizon: SColor,
    pub dawn_sky: SColor,
    pub dawn_horizon: SColor,
    pub night_sky: SColor,
    pub night_horizon: SColor,
    pub indoors: SColor,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct SunParams {
    pub visible: bool,
    pub texture: String,
    pub tonemap: String,
    pub sunrise: String,
    pub sunrise_visible: bool,
    pub scale: f32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct MoonParams {
    pub visible: bool,
    pub texture: String,
    pub tonemap: String,
    pub scale: f32,
}
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct StarParams {
    pub visible: bool,
    pub count: u32,
    pub starcolor: SColor,
    pub scale: f32,
    pub day_opacity: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct MinimapMode {
    pub typ: u16,
    pub label: String,
    pub size: u16,
    pub texture: String,
    pub scale: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerPos {
    pub position: v3f,     // serialized as v3s32, *100.0f
    pub speed: v3f,        // serialzied as v3s32, *100.0f
    pub pitch: f32,        // serialized as s32, *100.0f
    pub yaw: f32,          // serialized as s32, *100.0f
    pub keys_pressed: u32, // bitset
    pub fov: f32,          // serialized as u8, *80.0f
    pub wanted_range: u8,
}

impl Serialize for PlayerPos {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let s_position = (self.position * 100f32).as_v3s32();
        let s_speed = (self.speed * 100f32).as_v3s32();
        let s_pitch = (self.pitch * 100f32).round() as s32;
        let s_yaw = (self.yaw * 100f32).round() as s32;
        let s_fov = (self.fov * 80f32).round() as u8;

        Serialize::serialize(&s_position, ser)?;
        Serialize::serialize(&s_speed, ser)?;
        Serialize::serialize(&s_pitch, ser)?;
        Serialize::serialize(&s_yaw, ser)?;
        Serialize::serialize(&self.keys_pressed, ser)?;
        Serialize::serialize(&s_fov, ser)?;
        Serialize::serialize(&self.wanted_range, ser)?;
        Ok(())
    }
}

impl Deserialize for PlayerPos {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let s_position: v3s32 = Deserialize::deserialize(deser)?;
        let s_speed: v3s32 = Deserialize::deserialize(deser)?;
        let s_pitch: s32 = Deserialize::deserialize(deser)?;
        let s_yaw: s32 = Deserialize::deserialize(deser)?;
        let keys_pressed: u32 = Deserialize::deserialize(deser)?;
        let s_fov: u8 = Deserialize::deserialize(deser)?;
        let wanted_range: u8 = Deserialize::deserialize(deser)?;
        Ok(PlayerPos {
            position: s_position.as_v3f() / 100f32,
            speed: s_speed.as_v3f() / 100f32,
            pitch: (s_pitch as f32) / 100f32,
            yaw: (s_yaw as f32) / 100f32,
            keys_pressed: keys_pressed,
            fov: (s_fov as f32) / 80f32,
            wanted_range: wanted_range,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pair<T1, T2> {
    pub first: T1,
    pub second: T2,
}

impl<T1: Serialize, T2: Serialize> Serialize for Pair<T1, T2> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&self.first, ser)?;
        Serialize::serialize(&self.second, ser)?;
        Ok(())
    }
}

impl<T1: Deserialize, T2: Deserialize> Deserialize for Pair<T1, T2> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        Ok(Self {
            first: Deserialize::deserialize(deser)?,
            second: Deserialize::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum AccessDeniedCode {
    WrongPassword,
    UnexpectedData,
    Singleplayer,
    WrongVersion,
    WrongCharsInName,
    WrongName,
    TooManyUsers,
    EmptyPassword,
    AlreadyConnected,
    ServerFail,
    CustomString(String),
    Shutdown(String, bool), // custom message (or blank), should_reconnect
    Crash(String, bool),    // custom message (or blank), should_reconnect
}

impl Serialize for AccessDeniedCode {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use AccessDeniedCode::*;
        match self {
            WrongPassword => u8::serialize(&0, ser),
            UnexpectedData => u8::serialize(&1, ser),
            Singleplayer => u8::serialize(&2, ser),
            WrongVersion => u8::serialize(&3, ser),
            WrongCharsInName => u8::serialize(&4, ser),
            WrongName => u8::serialize(&5, ser),
            TooManyUsers => u8::serialize(&6, ser),
            EmptyPassword => u8::serialize(&7, ser),
            AlreadyConnected => u8::serialize(&8, ser),
            ServerFail => u8::serialize(&9, ser),
            CustomString(msg) => {
                u8::serialize(&10, ser)?;
                String::serialize(&msg, ser)?;
                Ok(())
            }
            Shutdown(msg, reconnect) => {
                u8::serialize(&11, ser)?;
                String::serialize(&msg, ser)?;
                bool::serialize(&reconnect, ser)?;
                Ok(())
            }
            Crash(msg, reconnect) => {
                u8::serialize(&12, ser)?;
                String::serialize(&msg, ser)?;
                bool::serialize(&reconnect, ser)?;
                Ok(())
            }
        }
    }
}

impl Deserialize for AccessDeniedCode {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use AccessDeniedCode::*;
        let deny_code = u8::deserialize(deser)?;
        match deny_code {
            0 => Ok(WrongPassword),
            1 => Ok(UnexpectedData),
            2 => Ok(Singleplayer),
            3 => Ok(WrongVersion),
            4 => Ok(WrongCharsInName),
            5 => Ok(WrongName),
            6 => Ok(TooManyUsers),
            7 => Ok(EmptyPassword),
            8 => Ok(AlreadyConnected),
            9 => Ok(ServerFail),
            10 => Ok(CustomString(String::deserialize(deser)?)),
            11 => Ok(Shutdown(
                String::deserialize(deser)?,
                (u8::deserialize(deser)? & 1) != 0,
            )),
            12 => Ok(Crash(
                String::deserialize(deser)?,
                (u8::deserialize(deser)? & 1) != 0,
            )),
            _ => Ok(CustomString(String::deserialize(deser)?)),
        }
    }
}

impl AccessDeniedCode {
    pub fn to_str<'a>(&'a self) -> &'a str {
        use AccessDeniedCode::*;
        match self {
            WrongPassword => "Invalid password",
            UnexpectedData => "Your client sent something the server didn't expect.  Try reconnecting or updating your client.",
            Singleplayer => "The server is running in simple singleplayer mode.  You cannot connect.",
            WrongVersion => "Your client's version is not supported.\nPlease contact the server administrator.",
            WrongCharsInName => "Player name contains disallowed characters",
            WrongName => "Player name not allowed",
            TooManyUsers => "Too many users",
            EmptyPassword => "Empty passwords are disallowed.  Set a password and try again.",
            AlreadyConnected => "Another client is connected with this name.  If your client closed unexpectedly, try again in a minute.",
            ServerFail => "Internal server error",
            CustomString(msg) => if msg.is_empty() { "unknown" } else { msg },
            Shutdown(msg, _) => if msg.is_empty() { "Server shutting down" } else { msg },
            Crash(msg, _) => if msg.is_empty() { "The server has experienced an internal error.  You will now be disconnected." } else { msg },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum HudStat {
    Pos(v2f),
    Name(String),
    Scale(v2f),
    Text(String),
    Number(u32),
    Item(u32),
    Dir(u32),
    Align(v2f),
    Offset(v2f),
    WorldPos(v3f),
    Size(v2s32),
    ZIndex(u32),
    Text2(String),
    Style(u32),
}

impl Serialize for HudStat {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use HudStat::*;
        match self {
            Pos(v) => {
                u8::serialize(&0, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Name(v) => {
                u8::serialize(&1, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Scale(v) => {
                u8::serialize(&2, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Text(v) => {
                u8::serialize(&3, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Number(v) => {
                u8::serialize(&4, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Item(v) => {
                u8::serialize(&5, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Dir(v) => {
                u8::serialize(&6, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Align(v) => {
                u8::serialize(&7, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Offset(v) => {
                u8::serialize(&8, ser)?;
                Serialize::serialize(v, ser)?;
            }
            WorldPos(v) => {
                u8::serialize(&9, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Size(v) => {
                u8::serialize(&10, ser)?;
                Serialize::serialize(v, ser)?;
            }
            ZIndex(v) => {
                u8::serialize(&11, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Text2(v) => {
                u8::serialize(&12, ser)?;
                Serialize::serialize(v, ser)?;
            }
            Style(v) => {
                u8::serialize(&13, ser)?;
                Serialize::serialize(v, ser)?;
            }
        }
        Ok(())
    }
}

impl Deserialize for HudStat {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use HudStat::*;
        let stat = u8::deserialize(deser)?;
        match stat {
            0 => Ok(Pos(Deserialize::deserialize(deser)?)),
            1 => Ok(Name(Deserialize::deserialize(deser)?)),
            2 => Ok(Scale(Deserialize::deserialize(deser)?)),
            3 => Ok(Text(Deserialize::deserialize(deser)?)),
            4 => Ok(Number(Deserialize::deserialize(deser)?)),
            5 => Ok(Item(Deserialize::deserialize(deser)?)),
            6 => Ok(Dir(Deserialize::deserialize(deser)?)),
            7 => Ok(Align(Deserialize::deserialize(deser)?)),
            8 => Ok(Offset(Deserialize::deserialize(deser)?)),
            9 => Ok(WorldPos(Deserialize::deserialize(deser)?)),
            10 => Ok(Size(Deserialize::deserialize(deser)?)),
            11 => Ok(ZIndex(Deserialize::deserialize(deser)?)),
            12 => Ok(Text2(Deserialize::deserialize(deser)?)),
            13 => Ok(Style(Deserialize::deserialize(deser)?)),
            _ => bail!(DeserializeError::InvalidValue(String::from(
                "HudStat invalid stat",
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SkyboxParams {
    pub bgcolor: SColor,
    pub typ: String,
    pub clouds: bool,
    pub fog_sun_tint: SColor,
    pub fog_moon_tint: SColor,
    pub fog_tint_type: String,
    // If skybox_type == "skybox"
    pub textures: Option<Array16<String>>,
    // If skybox_type == "regular"
    pub sky_color: Option<SkyColor>,
    pub body_orbit_tilt: Option<f32>,
}

impl Serialize for SkyboxParams {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        Serialize::serialize(&self.bgcolor, ser)?;
        Serialize::serialize(&self.typ, ser)?;
        Serialize::serialize(&self.clouds, ser)?;
        Serialize::serialize(&self.fog_sun_tint, ser)?;
        Serialize::serialize(&self.fog_moon_tint, ser)?;
        Serialize::serialize(&self.fog_tint_type, ser)?;
        if self.typ == "skybox" {
            Serialize::serialize(&self.textures, ser)?;
        } else if self.typ == "regular" {
            Serialize::serialize(&self.sky_color, ser)?;
        }
        if let Some(body_orbit_tilt) = self.body_orbit_tilt {
            Serialize::serialize(&body_orbit_tilt, ser)?;
        }
        Ok(())
    }
}

impl Deserialize for SkyboxParams {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let bgcolor: SColor = Deserialize::deserialize(deser)?;
        let typ: String = Deserialize::deserialize(deser)?;
        Ok(SkyboxParams {
            bgcolor: bgcolor,
            typ: typ.clone(),
            clouds: Deserialize::deserialize(deser)?,
            fog_sun_tint: Deserialize::deserialize(deser)?,
            fog_moon_tint: Deserialize::deserialize(deser)?,
            fog_tint_type: Deserialize::deserialize(deser)?,
            textures: if typ == "skybox" {
                Some(Deserialize::deserialize(deser)?)
            } else {
                None
            },
            sky_color: if typ == "regular" {
                Some(Deserialize::deserialize(deser)?)
            } else {
                None
            },
            body_orbit_tilt: Deserialize::deserialize(deser)?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MinimapModeList {
    pub mode: u16,
    pub vec: Vec<MinimapMode>,
}

impl Serialize for MinimapModeList {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // The length of the list is a u16 which precedes `mode`,
        // which makes the layout not fit into any usual pattern.
        Serialize::serialize(&u16::try_from(self.vec.len())?, ser)?;
        Serialize::serialize(&self.mode, ser)?;
        for v in self.vec.iter() {
            Serialize::serialize(v, ser)?;
        }
        Ok(())
    }
}

impl Deserialize for MinimapModeList {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let count: u16 = Deserialize::deserialize(deser)?;
        let mode: u16 = Deserialize::deserialize(deser)?;
        let mut vec: Vec<MinimapMode> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            vec.push(Deserialize::deserialize(deser)?);
        }
        Ok(MinimapModeList {
            mode: mode,
            vec: vec,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthMechsBitset {
    pub legacy_password: bool,
    pub srp: bool,
    pub first_srp: bool,
}

impl Serialize for AuthMechsBitset {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let mut value: u32 = 0;
        if self.legacy_password {
            value |= 1;
        }
        if self.srp {
            value |= 2;
        }
        if self.first_srp {
            value |= 4;
        }
        Serialize::serialize(&value, ser)
    }
}

impl Deserialize for AuthMechsBitset {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let value: u32 = Deserialize::deserialize(deser)?;
        Ok(AuthMechsBitset {
            legacy_password: (value & 1) != 0,
            srp: (value & 2) != 0,
            first_srp: (value & 4) != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZLibCompressed<T> {
    pub value: T,
}

impl<T: Serialize> Serialize for ZLibCompressed<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // TODO(paradust): Performance nightmare.

        // Serialize 'value' to a temporary buffer, and then compress
        let mut tmp = VecSerializer::new(ser.context(), 1024);
        Serialize::serialize(&self.value, &mut tmp)?;
        let tmp = tmp.take();
        let tmp = miniz_oxide::deflate::compress_to_vec_zlib(&tmp, 6);

        // Write the size as a u32, followed by the data
        Serialize::serialize(&u32::try_from(tmp.len())?, ser)?;
        ser.write_bytes(&tmp)?;
        Ok(())
    }
}

impl<T: Deserialize> Deserialize for ZLibCompressed<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let num_bytes = u32::deserialize(deser)? as usize;
        let data = deser.take(num_bytes)?;
        // TODO(paradust): DANGEROUS. There is no decompression size bound.
        match miniz_oxide::inflate::decompress_to_vec_zlib(&data) {
            Ok(decompressed) => {
                let mut tmp = Deserializer::new(deser.context(), &decompressed);
                Ok(Self {
                    value: Deserialize::deserialize(&mut tmp)?,
                })
            }
            Err(err) => bail!(DeserializeError::DecompressionFailed(err.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ZStdCompressed<T> {
    pub value: T,
}

impl<T: Serialize> Serialize for ZStdCompressed<T> {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // Serialize 'value' into a temporary buffer
        // TODO(paradust): Performance concern, could stream instead
        let mut tmp = VecSerializer::new(ser.context(), 65536);
        Serialize::serialize(&self.value, &mut tmp)?;
        let tmp = tmp.take();
        match zstd_compress(&tmp, |chunk| {
            ser.write_bytes(chunk)?;
            Ok(())
        }) {
            Ok(_) => Ok(()),
            Err(err) => bail!(SerializeError::CompressionFailed(err.to_string())),
        }
    }
}

impl<T: Deserialize> Deserialize for ZStdCompressed<T> {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        // Decompress to a temporary buffer
        let mut tmp: Vec<u8> = Vec::with_capacity(65536);
        match zstd_decompress(deser.peek_all(), |chunk| {
            tmp.extend_from_slice(chunk);
            Ok(())
        }) {
            Ok(consumed) => {
                deser.take(consumed)?;
                let mut tmp_deser = Deserializer::new(deser.context(), &tmp);
                Ok(Self {
                    value: Deserialize::deserialize(&mut tmp_deser)?,
                })
            }
            Err(err) => bail!(DeserializeError::DecompressionFailed(err.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ItemdefList {
    pub itemdef_manager_version: u8,
    pub defs: Array16<Wrapped16<ItemDef>>,
    pub aliases: Array16<ItemAlias>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum ItemType {
    None,
    Node,
    Craft,
    Tool,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ToolGroupCap {
    pub uses: s16,
    pub maxlevel: s16,
    // (level, time)
    pub times: Array32<Pair<s16, f32>>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ToolCapabilities {
    pub version: u8,
    pub full_punch_interval: f32,
    pub max_drop_level: s16,
    // (name, tool group cap)
    pub group_caps: Array32<Pair<String, ToolGroupCap>>,
    // (name, rating)
    pub damage_groups: Array32<Pair<String, s16>>,
    pub punch_attack_uses: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct SimpleSoundSpec {
    pub name: String,
    pub gain: f32,
    pub pitch: f32,
    pub fade: f32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ItemDef {
    pub version: u8,
    pub item_type: ItemType,
    pub name: String,
    pub description: String,
    pub inventory_image: String,
    pub wield_image: String,
    pub wield_scale: v3f,
    pub stack_max: s16,
    pub usable: bool,
    pub liquids_pointable: bool,
    pub tool_capabilities: Option16<ToolCapabilities>,
    pub groups: Array16<Pair<String, s16>>,
    pub node_placement_prediction: String,
    pub sound_place: SimpleSoundSpec,
    pub sound_place_failed: SimpleSoundSpec,
    pub range: f32,
    pub palette_image: String,
    pub color: SColor,
    pub inventory_overlay: String,
    pub wield_overlay: String,
    pub short_description: Option<String>,
    pub place_param2: Option<u8>,
    pub sound_use: Option<SimpleSoundSpec>,
    pub sound_use_air: Option<SimpleSoundSpec>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ItemAlias {
    pub name: String,
    pub convert_to: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TileDef {
    pub name: String,
    pub animation: TileAnimationParams,
    // These are stored in a single u8 flags
    pub backface_culling: bool,
    pub tileable_horizontal: bool,
    pub tileable_vertical: bool,
    // The flags also determine which of these is present
    pub color_rgb: Option<(u8, u8, u8)>,
    pub scale: u8,
    pub align_style: AlignStyle,
}

const TILE_FLAG_BACKFACE_CULLING: u16 = 1 << 0;
const TILE_FLAG_TILEABLE_HORIZONTAL: u16 = 1 << 1;
const TILE_FLAG_TILEABLE_VERTICAL: u16 = 1 << 2;
const TILE_FLAG_HAS_COLOR: u16 = 1 << 3;
const TILE_FLAG_HAS_SCALE: u16 = 1 << 4;
const TILE_FLAG_HAS_ALIGN_STYLE: u16 = 1 << 5;

impl Serialize for TileDef {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        u8::serialize(&6, ser)?; // tiledef version
        Serialize::serialize(&self.name, ser)?;
        Serialize::serialize(&self.animation, ser)?;
        let mut flags: u16 = 0;
        if self.backface_culling {
            flags |= TILE_FLAG_BACKFACE_CULLING;
        }
        if self.tileable_horizontal {
            flags |= TILE_FLAG_TILEABLE_HORIZONTAL;
        }
        if self.tileable_vertical {
            flags |= TILE_FLAG_TILEABLE_VERTICAL;
        }
        if self.color_rgb.is_some() {
            flags |= TILE_FLAG_HAS_COLOR;
        }
        if self.scale != 0 {
            flags |= TILE_FLAG_HAS_SCALE;
        }
        if self.align_style != AlignStyle::Node {
            flags |= TILE_FLAG_HAS_ALIGN_STYLE;
        }
        u16::serialize(&flags, ser)?;
        if let Some(color) = &self.color_rgb {
            u8::serialize(&color.0, ser)?;
            u8::serialize(&color.1, ser)?;
            u8::serialize(&color.2, ser)?;
        }
        if self.scale != 0 {
            u8::serialize(&self.scale, ser)?;
        }
        if self.align_style != AlignStyle::Node {
            Serialize::serialize(&self.align_style, ser)?;
        }
        Ok(())
    }
}

impl Deserialize for TileDef {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let version: u8 = u8::deserialize(deser)?;
        if version != 6 {
            bail!(DeserializeError::InvalidValue(
                "Invalid TileDef version".to_string(),
            ));
        }
        let name = String::deserialize(deser)?;
        let animation = TileAnimationParams::deserialize(deser)?;
        let flags = u16::deserialize(deser)?;
        let color = if (flags & TILE_FLAG_HAS_COLOR) != 0 {
            Some((
                u8::deserialize(deser)?,
                u8::deserialize(deser)?,
                u8::deserialize(deser)?,
            ))
        } else {
            None
        };
        let scale = if (flags & TILE_FLAG_HAS_SCALE) != 0 {
            u8::deserialize(deser)?
        } else {
            0
        };
        let align_style = if (flags & TILE_FLAG_HAS_ALIGN_STYLE) != 0 {
            AlignStyle::deserialize(deser)?
        } else {
            AlignStyle::Node
        };

        Ok(Self {
            name,
            animation,
            backface_culling: (flags & TILE_FLAG_BACKFACE_CULLING) != 0,
            tileable_horizontal: (flags & TILE_FLAG_TILEABLE_HORIZONTAL) != 0,
            tileable_vertical: (flags & TILE_FLAG_TILEABLE_VERTICAL) != 0,
            color_rgb: color,
            scale,
            align_style,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TileAnimationParams {
    None,
    VerticalFrames {
        aspect_w: u16,
        aspect_h: u16,
        length: f32,
    },
    Sheet2D {
        frames_w: u8,
        frames_h: u8,
        frame_length: f32,
    },
}

// TileAnimationType
const TAT_NONE: u8 = 0;
const TAT_VERTICAL_FRAMES: u8 = 1;
const TAT_SHEET_2D: u8 = 2;

impl Serialize for TileAnimationParams {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let typ = match self {
            TileAnimationParams::None => TAT_NONE,
            TileAnimationParams::VerticalFrames { .. } => TAT_VERTICAL_FRAMES,
            TileAnimationParams::Sheet2D { .. } => TAT_SHEET_2D,
        };
        u8::serialize(&typ, ser)?;
        match self {
            TileAnimationParams::None => {}
            TileAnimationParams::VerticalFrames {
                aspect_w,
                aspect_h,
                length,
            } => {
                u16::serialize(&aspect_w, ser)?;
                u16::serialize(&aspect_h, ser)?;
                f32::serialize(&length, ser)?;
            }
            TileAnimationParams::Sheet2D {
                frames_w,
                frames_h,
                frame_length,
            } => {
                u8::serialize(&frames_w, ser)?;
                u8::serialize(&frames_h, ser)?;
                f32::serialize(&frame_length, ser)?;
            }
        };
        Ok(())
    }
}

impl Deserialize for TileAnimationParams {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let typ = u8::deserialize(deser)?;
        match typ {
            TAT_NONE => Ok(TileAnimationParams::None),
            TAT_VERTICAL_FRAMES => Ok(TileAnimationParams::VerticalFrames {
                aspect_w: Deserialize::deserialize(deser)?,
                aspect_h: Deserialize::deserialize(deser)?,
                length: Deserialize::deserialize(deser)?,
            }),
            TAT_SHEET_2D => Ok(TileAnimationParams::Sheet2D {
                frames_w: Deserialize::deserialize(deser)?,
                frames_h: Deserialize::deserialize(deser)?,
                frame_length: Deserialize::deserialize(deser)?,
            }),
            _ => bail!(DeserializeError::InvalidValue(format!(
                "Invalid TileAnimationParams type {} at: {:?}",
                typ, deser.data
            ))),
        }
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum AlignStyle {
    Node,
    World,
    UserDefined,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum DrawType {
    Normal,
    AirLike,
    Liquid,
    FlowingLiquid,
    GlassLike,
    AllFaces,
    AllFacesOptional,
    TorchLike,
    SignLike,
    PlantLike,
    FenceLike,
    RailLike,
    NodeBox,
    GlassLikeFramed,
    FireLike,
    GlassLikeFramedOptional,
    Mesh,
    PlantLikeRooted,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ContentFeatures {
    pub version: u8,
    pub name: String,
    pub groups: Array16<Pair<String, s16>>,
    pub param_type: u8,
    pub param_type_2: u8,
    pub drawtype: DrawType,
    pub mesh: String,
    pub visual_scale: f32,
    // this was an attempt to be tiledef length, but then they added an extra 6 tiledefs without fixing it
    pub unused_six: u8,
    pub tiledef: FixedArray<6, TileDef>,
    pub tiledef_overlay: FixedArray<6, TileDef>,
    pub tiledef_special: Array8<TileDef>,
    pub alpha_for_legacy: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub palette_name: String,
    pub waving: u8,
    pub connect_sides: u8,
    pub connects_to_ids: Array16<u16>,
    pub post_effect_color: SColor,
    pub leveled: u8,
    pub light_propagates: u8,
    pub sunlight_propagates: u8,
    pub light_source: u8,
    pub is_ground_content: bool,
    pub walkable: bool,
    pub pointable: bool,
    pub diggable: bool,
    pub climbable: bool,
    pub buildable_to: bool,
    pub rightclickable: bool,
    pub damage_per_second: u32,
    pub liquid_type_bc: u8,
    pub liquid_alternative_flowing: String,
    pub liquid_alternative_source: String,
    pub liquid_viscosity: u8,
    pub liquid_renewable: bool,
    pub liquid_range: u8,
    pub drowning: u8,
    pub floodable: bool,
    pub node_box: NodeBox,
    pub selection_box: NodeBox,
    pub collision_box: NodeBox,
    pub sound_footstep: SimpleSoundSpec,
    pub sound_dig: SimpleSoundSpec,
    pub sound_dug: SimpleSoundSpec,
    pub legacy_facedir_simple: bool,
    pub legacy_wallmounted: bool,
    pub node_dig_prediction: Option<String>,
    pub leveled_max: Option<u8>,
    pub alpha: Option<AlphaMode>,
    pub move_resistance: Option<u8>,
    pub liquid_move_physics: Option<bool>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeBox {
    Regular,
    Fixed(NodeBoxFixed),
    Wallmounted(NodeBoxWallmounted),
    Leveled(NodeBoxLeveled),
    Connected(NodeBoxConnected),
}

impl Serialize for NodeBox {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // Unused version number, always 6
        u8::serialize(&6, ser)?;

        let typ = match self {
            NodeBox::Regular => 0,
            NodeBox::Fixed(_) => 1,
            NodeBox::Wallmounted(_) => 2,
            NodeBox::Leveled(_) => 3,
            NodeBox::Connected(_) => 4,
        };
        u8::serialize(&typ, ser)?;
        match self {
            NodeBox::Regular => Ok(()),
            NodeBox::Fixed(v) => Serialize::serialize(v, ser),
            NodeBox::Wallmounted(v) => Serialize::serialize(v, ser),
            NodeBox::Leveled(v) => Serialize::serialize(v, ser),
            NodeBox::Connected(v) => Serialize::serialize(v, ser),
        }
    }
}

impl Deserialize for NodeBox {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let ver = u8::deserialize(deser)?;
        if ver != 6 {
            bail!(DeserializeError::InvalidValue(
                "Invalid NodeBox ver".to_string(),
            ));
        }
        let typ = u8::deserialize(deser)?;
        match typ {
            0 => Ok(NodeBox::Regular),
            1 => Ok(NodeBox::Fixed(Deserialize::deserialize(deser)?)),
            2 => Ok(NodeBox::Wallmounted(Deserialize::deserialize(deser)?)),
            3 => Ok(NodeBox::Leveled(Deserialize::deserialize(deser)?)),
            4 => Ok(NodeBox::Connected(Deserialize::deserialize(deser)?)),
            _ => bail!(DeserializeError::InvalidValue(
                "Invalid NodeBox type".to_string(),
            )),
        }
    }
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct aabb3f {
    pub min_edge: v3f,
    pub max_edge: v3f,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct NodeBoxLeveled {
    pub fixed: Array16<aabb3f>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct NodeBoxFixed {
    pub fixed: Array16<aabb3f>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct NodeBoxWallmounted {
    pub wall_top: aabb3f,
    pub wall_bottom: aabb3f,
    pub wall_side: aabb3f,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct NodeBoxConnected {
    pub fixed: Array16<aabb3f>,
    pub connect_top: Array16<aabb3f>,
    pub connect_bottom: Array16<aabb3f>,
    pub connect_front: Array16<aabb3f>,
    pub connect_left: Array16<aabb3f>,
    pub connect_back: Array16<aabb3f>,
    pub connect_right: Array16<aabb3f>,
    pub disconnected_top: Array16<aabb3f>,
    pub disconnected_bottom: Array16<aabb3f>,
    pub disconnected_front: Array16<aabb3f>,
    pub disconnected_left: Array16<aabb3f>,
    pub disconnected_back: Array16<aabb3f>,
    pub disconnected_right: Array16<aabb3f>,
    pub disconnected: Array16<aabb3f>,
    pub disconnected_sides: Array16<aabb3f>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum AlphaMode {
    Blend,
    Clip,
    Opaque,
    LegacyCompat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeDefManager {
    pub content_features: Vec<(u16, ContentFeatures)>,
}

/// The way this structure is encoded is really unusual, in order to
/// allow the ContentFeatures to be extended in the future without
/// changing the encoding.
impl Serialize for NodeDefManager {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // Version
        u8::serialize(&1, ser)?;
        let count: u16 = u16::try_from(self.content_features.len())?;
        u16::serialize(&count, ser)?;
        // The serialization of content_features is wrapped in a String32
        // Write a marker so we can write the size later
        let string32_wrapper = ser.write_marker(4)?;
        for (i, f) in self.content_features.iter() {
            u16::serialize(i, ser)?;
            // The contents of each feature is wrapped in a String16.
            let string16_wrapper = ser.write_marker(2)?;
            Serialize::serialize(f, ser)?;
            let wlen: u16 = u16::try_from(ser.marker_distance(&string16_wrapper))?;
            ser.set_marker(string16_wrapper, &wlen.to_be_bytes()[..])?;
        }
        let wlen: u32 = u32::try_from(ser.marker_distance(&string32_wrapper))?;
        ser.set_marker(string32_wrapper, &wlen.to_be_bytes()[..])?;
        Ok(())
    }
}

impl Deserialize for NodeDefManager {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let version = u8::deserialize(deser)?;
        if version != 1 {
            bail!(DeserializeError::InvalidValue(
                "Bad NodeDefManager version".to_string(),
            ));
        }
        let count: u16 = u16::deserialize(deser)?;
        let string32_wrapper_len: u32 = u32::deserialize(deser)?;
        // Shadow deser with a restricted deserializer
        let mut deser = deser.slice(string32_wrapper_len as usize)?;
        let mut content_features: Vec<(u16, ContentFeatures)> = Vec::with_capacity(count as usize);
        for _ in 0..count {
            let i = u16::deserialize(&mut deser)?;
            let string16_wrapper_len: u16 = u16::deserialize(&mut deser)?;
            let mut inner_deser = deser.slice(string16_wrapper_len as usize)?;
            let f = ContentFeatures::deserialize(&mut inner_deser)?;
            content_features.push((i, f));
        }
        Ok(Self { content_features })
    }
}

// A "block" is 16x16x16 "nodes"
const MAP_BLOCKSIZE: u16 = 16;

// Number of nodes in a block
const NODECOUNT: u16 = MAP_BLOCKSIZE * MAP_BLOCKSIZE * MAP_BLOCKSIZE;

#[derive(Debug, Clone, PartialEq)]
pub struct MapBlock {
    pub is_underground: bool,
    pub day_night_diff: bool,
    pub generated: bool,
    pub lighting_complete: Option<u16>,
    pub nodes: MapNodesBulk,
    pub node_metadata: NodeMetadataList, // m_node_metadata.serialize(os, version, disk);
}

impl Serialize for MapBlock {
    /// MapBlock is a bit of a nightmare, because the compression algorithm
    /// and where the compression is applied (to the whole struct, or to
    /// parts of it) depends on the serialization format version.
    ///
    /// For now, only ser_fmt >= 28 is supported.
    /// For ver 28, only the nodes and nodemeta are compressed using zlib.
    /// For >= 29, the entire thing is compressed using zstd.
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let ver = ser.context().ser_fmt;
        let real_ser = ser;
        let mut tmp_ser = VecSerializer::new(real_ser.context(), 32768);
        let ser = &mut tmp_ser;
        let header = MapBlockHeader {
            is_underground: self.is_underground,
            day_night_diff: self.day_night_diff,
            generated: self.generated,
            lighting_complete: self.lighting_complete,
        };
        Serialize::serialize(&header, ser)?;
        if ver >= 29 {
            Serialize::serialize(&self.nodes, ser)?;
        } else {
            // Serialize and compress using zlib
            let mut inner = VecSerializer::new(ser.context(), 32768);
            Serialize::serialize(&self.nodes, &mut inner)?;
            let compressed = compress_zlib(&inner.take());
            ser.write_bytes(&compressed)?;
        }
        if ver >= 29 {
            Serialize::serialize(&self.node_metadata, ser)?;
        } else {
            // Serialize and compress using zlib
            let mut inner = VecSerializer::new(ser.context(), 32768);
            Serialize::serialize(&self.node_metadata, &mut inner)?;
            let compressed = compress_zlib(&inner.take());
            ser.write_bytes(&compressed)?;
        }
        if ver >= 29 {
            // The whole thing is zstd compressed
            let tmp = tmp_ser.take();
            zstd_compress(&tmp, |chunk| real_ser.write_bytes(chunk))?;
        } else {
            // Just write it directly
            let tmp = tmp_ser.take();
            real_ser.write_bytes(&tmp)?;
        }
        Ok(())
    }
}

///
/// This is a helper for MapBlock ser/deser
/// Not exposed publicly.
struct MapBlockHeader {
    pub is_underground: bool,
    pub day_night_diff: bool,
    pub generated: bool,
    pub lighting_complete: Option<u16>,
}

impl Serialize for MapBlockHeader {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let mut flags: u8 = 0;
        if self.is_underground {
            flags |= 0x1;
        }
        if self.day_night_diff {
            flags |= 0x2;
        }
        if !self.generated {
            flags |= 0x8;
        }
        u8::serialize(&flags, ser)?;
        if ser.context().ser_fmt >= 27 {
            if let Some(lighting_complete) = self.lighting_complete {
                u16::serialize(&lighting_complete, ser)?;
            } else {
                bail!("lighting_complete must be set for ver >= 27");
            }
        }
        u8::serialize(&2, ser)?; // content_width == 2
        u8::serialize(&2, ser)?; // params_width == 2
        Ok(())
    }
}

impl Deserialize for MapBlockHeader {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let flags = u8::deserialize(deser)?;
        if flags != (flags & (0x1 | 0x2 | 0x8)) {
            bail!(DeserializeError::InvalidValue(
                "Invalid MapBlock flags".to_string(),
            ));
        }
        let lighting_complete = if deser.context().ser_fmt >= 27 {
            Some(u16::deserialize(deser)?)
        } else {
            None
        };
        let content_width = u8::deserialize(deser)?;
        let params_width = u8::deserialize(deser)?;
        if content_width != 2 || params_width != 2 {
            bail!(DeserializeError::InvalidValue(
                "Corrupt MapBlock: content_width and params_width not both 2".to_string(),
            ));
        }
        Ok(Self {
            is_underground: (flags & 0x1) != 0,
            day_night_diff: (flags & 0x2) != 0,
            generated: (flags & 0x8) == 0,
            lighting_complete: lighting_complete,
        })
    }
}

impl Deserialize for MapBlock {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let ver = deser.context().ser_fmt;
        if ver < 28 {
            bail!("Unsupported ser fmt");
        }
        // TODO(paradust): I can't make the borrow checker happy with sharing
        // code here, so for now the code has two different paths.
        if ver >= 29 {
            let mut tmp: Vec<u8> = Vec::new();
            // Decompress to a temporary buffer
            let bytes_taken = zstd_decompress(deser.peek_all(), |chunk| {
                tmp.extend_from_slice(chunk);
                Ok(())
            })?;
            deser.take(bytes_taken)?;
            let deser = &mut Deserializer::new(deser.context(), &tmp);
            let header: MapBlockHeader = Deserialize::deserialize(deser)?;
            let nodes = Deserialize::deserialize(deser)?;
            let node_metadata = Deserialize::deserialize(deser)?;
            Ok(Self {
                is_underground: header.is_underground,
                day_night_diff: header.day_night_diff,
                generated: header.generated,
                lighting_complete: header.lighting_complete,
                nodes,
                node_metadata,
            })
        } else {
            let header: MapBlockHeader = Deserialize::deserialize(deser)?;
            let (consumed, nodes_raw) = decompress_zlib(deser.peek_all())?;
            deser.take(consumed)?;
            let nodes = {
                let mut tmp = Deserializer::new(deser.context(), &nodes_raw);
                Deserialize::deserialize(&mut tmp)?
            };
            let (consumed, metadata_raw) = decompress_zlib(deser.peek_all())?;
            deser.take(consumed)?;
            let node_metadata = {
                let mut tmp = Deserializer::new(deser.context(), &metadata_raw);
                Deserialize::deserialize(&mut tmp)?
            };
            Ok(Self {
                is_underground: header.is_underground,
                day_night_diff: header.day_night_diff,
                generated: header.generated,
                lighting_complete: header.lighting_complete,
                nodes,
                node_metadata,
            })
        }
    }
}

/// This has a special serialization, presumably to make it compress better.
/// Each param is stored in a separate array.
#[derive(Debug, Clone, PartialEq)]
pub struct MapNodesBulk {
    pub nodes: [MapNode; NODECOUNT as usize],
}

impl Serialize for MapNodesBulk {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let nodecount = NODECOUNT as usize;
        // Write all param0 first
        ser.write(2 * nodecount as usize, |buf| {
            assert!(buf.len() == 2 * nodecount as usize);
            for i in 0..nodecount {
                let v = self.nodes[i].param0.to_be_bytes();
                buf[2 * i] = v[0];
                buf[2 * i + 1] = v[1];
            }
        })?;
        // Write all param1
        ser.write(nodecount, |buf| {
            assert!(buf.len() == nodecount);
            for i in 0..nodecount {
                buf[i] = self.nodes[i].param1;
            }
        })?;
        // Write all param2
        ser.write(nodecount, |buf| {
            assert!(buf.len() == nodecount);
            for i in 0..nodecount {
                buf[i] = self.nodes[i].param2;
            }
        })?;
        Ok(())
    }
}

impl Deserialize for MapNodesBulk {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let nodecount = NODECOUNT as usize;
        let data = deser.take(4 * nodecount)?;
        let mut nodes: Vec<MapNode> = Vec::with_capacity(nodecount);
        let param1_offset = 2 * nodecount;
        let param2_offset = 3 * nodecount;
        for i in 0..nodecount {
            nodes.push(MapNode {
                param0: u16::from_be_bytes(data[2 * i..2 * i + 2].try_into().unwrap()),
                param1: data[param1_offset + i],
                param2: data[param2_offset + i],
            })
        }
        Ok(Self {
            nodes: match nodes.try_into() {
                Ok(value) => value,
                Err(_) => bail!("Bug in MapNodesBulk"),
            },
        })
    }
}

/// The default serialization is used for single nodes.
/// But for transferring entire blocks, MapNodeBulk is used instead.
#[derive(Debug, Clone, Copy, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct MapNode {
    pub param0: u16,
    pub param1: u8,
    pub param2: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NodeMetadataList {
    pub metadata: Array16<Pair<BlockPos, NodeMetadata>>,
}

impl Serialize for NodeMetadataList {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        if self.metadata.len() == 0 {
            u8::serialize(&0, ser)?; // version 0 indicates no data
            return Ok(());
        }
        u8::serialize(&2, ser)?; // version == 2
        Serialize::serialize(&self.metadata, ser)?;
        Ok(())
    }
}

impl Deserialize for NodeMetadataList {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let ver = u8::deserialize(deser)?;
        if ver == 0 {
            return Ok(Self {
                metadata: Array16 { vec: Vec::new() },
            });
        } else if ver == 2 {
            Ok(Self {
                metadata: Deserialize::deserialize(deser)?,
            })
        } else {
            bail!(DeserializeError::InvalidValue(
                "Invalid NodeMetadataList version".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AbsNodeMetadataList {
    pub metadata: Array16<Pair<AbsBlockPos, NodeMetadata>>,
}

impl Serialize for AbsNodeMetadataList {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        if self.metadata.len() == 0 {
            u8::serialize(&0, ser)?; // version 0 indicates no data
            return Ok(());
        }
        u8::serialize(&2, ser)?; // version == 2
        Serialize::serialize(&self.metadata, ser)?;
        Ok(())
    }
}

impl Deserialize for AbsNodeMetadataList {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let ver = u8::deserialize(deser)?;
        if ver == 0 {
            return Ok(Self {
                metadata: Array16 { vec: Vec::new() },
            });
        } else if ver == 2 {
            Ok(Self {
                metadata: Deserialize::deserialize(deser)?,
            })
        } else {
            bail!(DeserializeError::InvalidValue(
                "Invalid AbsNodeMetadataList version".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AbsBlockPos {
    pos: v3s16,
}

/// BlockPos addresses a node within a block
/// It is equivalent to (16*z + y)*16 + x, where x,y,z are from 0 to 15.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockPos {
    pub raw: u16,
}

impl BlockPos {
    pub fn new(x: s16, y: s16, z: s16) -> Self {
        let valid = 0..(MAP_BLOCKSIZE as s16);
        assert!(valid.contains(&x) && valid.contains(&y) && valid.contains(&z));
        let x = x as u16;
        let y = y as u16;
        let z = z as u16;
        Self {
            raw: (MAP_BLOCKSIZE * z + y) * MAP_BLOCKSIZE + x,
        }
    }

    pub fn from_xyz(pos: v3s16) -> Self {
        Self::new(pos.x, pos.y, pos.z)
    }

    pub fn to_xyz(&self) -> v3s16 {
        let x = self.raw % 16;
        let y = (self.raw / 16) % 16;
        let z = (self.raw / 256) % 16;
        v3s16::new(x as i16, y as i16, z as i16)
    }
}

impl Serialize for BlockPos {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        u16::serialize(&self.raw, ser)?;
        Ok(())
    }
}

impl Deserialize for BlockPos {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let raw = u16::deserialize(deser)?;
        if raw >= 4096 {
            bail!(DeserializeError::InvalidValue(
                "Invalid BlockPos".to_string(),
            ))
        }
        Ok(Self { raw })
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct NodeMetadata {
    pub stringvars: Array32<StringVar>,
    pub inventory: Inventory,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct StringVar {
    pub name: String,
    pub value: BinaryData32,
    pub is_private: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Inventory {
    pub entries: Vec<InventoryEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InventoryEntry {
    // Inventory lists to keep
    KeepList(String),
    // Inventory lists to add or update
    Update(InventoryList),
}

/// Inventory is sent as a "almost" line-based text format.
/// Unfortutely there's no way to simplify this code, it has to mirror
/// the way Minetest does it exactly, because it is so arbitrary.
impl Serialize for Inventory {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        for entry in &self.entries {
            match entry {
                InventoryEntry::KeepList(list_name) => {
                    // TODO(paradust): Performance. A format!-like macro that
                    //                 writes directly to ser could be faster.
                    ser.write_bytes(b"KeepList ")?;
                    ser.write_bytes(list_name.as_bytes())?;
                    ser.write_bytes(b"\n")?;
                }
                InventoryEntry::Update(list) => {
                    // Takes care of the List header line
                    Serialize::serialize(list, ser)?;
                }
            }
        }
        ser.write_bytes(b"EndInventory\n")?;
        Ok(())
    }
}

impl Deserialize for Inventory {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let mut result = Self {
            entries: Vec::new(),
        };
        while deser.remaining() > 0 {
            // Peek the line, but don't take it yet.
            let line = deser.peek_line()?;
            let words = split_by_whitespace(line);
            if words.len() == 0 {
                deser.take_line()?;
                continue;
            }
            let name = words[0];
            if name == b"EndInventory" || name == b"End" {
                // Take the line
                deser.take_line()?;
                return Ok(result);
            } else if name == b"List" {
                // InventoryList will take the line
                result
                    .entries
                    .push(InventoryEntry::Update(InventoryList::deserialize(deser)?));
            } else if name == b"KeepList" {
                if words.len() < 2 {
                    bail!(DeserializeError::InvalidValue(
                        "KeepList missing name".to_string(),
                    ));
                }
                match std::str::from_utf8(&words[1]) {
                    Ok(s) => result.entries.push(InventoryEntry::KeepList(s.to_string())),
                    Err(_) => {
                        bail!(DeserializeError::InvalidValue(
                            "KeepList name is invalid UTF8".to_string(),
                        ))
                    }
                }
                // Take the line
                deser.take_line()?;
            } else {
                // Anything else is supposed to be ignored. Gross.
                deser.take_line()?;
            }
        }
        // If we ran out before seeing the end marker, it's an error
        bail!(DeserializeError::Eof)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryList {
    pub name: String,
    pub width: u32,
    pub items: Vec<ItemStackUpdate>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItemStackUpdate {
    Empty,
    Keep, // this seems to not be used yet
    Item(ItemStack),
}

impl Serialize for InventoryList {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // List <name> <size>
        ser.write_bytes(b"List ")?;
        ser.write_bytes(self.name.as_bytes())?;
        ser.write_bytes(b" ")?;
        ser.write_bytes(self.items.len().to_string().as_bytes())?;
        ser.write_bytes(b"\n")?;

        // Width <width>
        ser.write_bytes(b"Width ")?;
        ser.write_bytes(self.width.to_string().as_bytes())?;
        ser.write_bytes(b"\n")?;

        for item in self.items.iter() {
            match item {
                ItemStackUpdate::Empty => ser.write_bytes(b"Empty\n")?,
                ItemStackUpdate::Keep => ser.write_bytes(b"Keep\n")?,
                ItemStackUpdate::Item(itemstack) => {
                    // Writes Item line
                    Serialize::serialize(itemstack, ser)?;
                }
            }
        }
        ser.write_bytes(b"EndInventoryList\n")?;
        Ok(())
    }
}

impl Deserialize for InventoryList {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        // First line should be: List <name> <item_count>
        let line = deser.take_line()?;
        let words = split_by_whitespace(line);
        if words.len() != 3 || words[0] != b"List" {
            bail!(DeserializeError::InvalidValue(
                "Broken List tag".to_string(),
            ));
        }
        let list_name = std::str::from_utf8(words[1])?;
        let _count: u32 = stoi(words[2])?;
        let mut result = Self {
            name: list_name.to_string(),
            width: 0,
            items: Vec::new(),
        };
        while deser.remaining() > 0 {
            // Peek the line, but don't take it yet.
            let line = deser.peek_line()?;
            let words = split_by_whitespace(line);
            if words.len() == 0 {
                deser.take_line()?;
                continue;
            }
            let name = words[0];
            if name == b"EndInventoryList" || name == b"end" {
                deser.take_line()?;
                return Ok(result);
            } else if name == b"Width" {
                if words.len() < 2 {
                    bail!(DeserializeError::InvalidValue(
                        "Width value missing".to_string(),
                    ));
                }
                result.width = stoi(words[1])?;
                deser.take_line()?;
            } else if name == b"Item" {
                // ItemStack takes the line
                result
                    .items
                    .push(ItemStackUpdate::Item(Deserialize::deserialize(deser)?));
            } else if name == b"Empty" {
                result.items.push(ItemStackUpdate::Empty);
                deser.take_line()?;
            } else if name == b"Keep" {
                result.items.push(ItemStackUpdate::Keep);
                deser.take_line()?;
            } else {
                // Ignore unrecognized lines
                deser.take_line()?;
            }
        }
        bail!(DeserializeError::Eof)
    }
}

// Custom deserialization, part of Inventory
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStack {
    pub name: String,
    pub count: u16,
    pub wear: u16,
    pub metadata: ItemStackMetadata,
}

impl Serialize for ItemStack {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // Item <name_json> [count] [wear] [metadata]
        ser.write_bytes(b"Item ")?;
        serialize_json_string_if_needed(
            &self.name.as_bytes(),
            |chunk| Ok(ser.write_bytes(chunk)?),
        )?;

        let mut parts = 1;
        if !self.metadata.string_vars.is_empty() {
            parts = 4;
        } else if self.wear != 0 {
            parts = 3;
        } else if self.count != 1 {
            parts = 2;
        }

        if parts >= 2 {
            ser.write_bytes(b" ")?;
            ser.write_bytes(self.count.to_string().as_bytes())?;
        }
        if parts >= 3 {
            ser.write_bytes(b" ")?;
            ser.write_bytes(self.wear.to_string().as_bytes())?;
        }
        if parts >= 4 {
            ser.write_bytes(b" ")?;
            Serialize::serialize(&self.metadata, ser)?;
        }
        ser.write_bytes(b"\n")?;
        Ok(())
    }
}

impl Deserialize for ItemStack {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        // Item "name maybe escaped" [count] [wear] ["metadata escaped"]
        let line = deser.take_line()?;
        let err = DeserializeError::InvalidValue("Truncated Item line".to_string());
        let (word, line) = next_word(line).ok_or(err)?;
        if word != b"Item" {
            bail!(DeserializeError::InvalidValue(
                "Invalid Item line".to_string(),
            ));
        }
        let line = skip_whitespace(line);
        let (name, skip) = deserialize_json_string_if_needed(line)?;
        let line = skip_whitespace(&line[skip..]);

        let mut result = Self {
            name: std::str::from_utf8(&name)?.to_string(),
            count: 1,
            wear: 0,
            metadata: ItemStackMetadata {
                string_vars: Vec::new(),
            },
        };
        if let Some((word, line)) = next_word(line) {
            result.count = stoi(word)?;
            if let Some((word, line)) = next_word(line) {
                result.wear = stoi(word)?;
                let line = skip_whitespace(line);
                if line.len() > 0 {
                    let mut tmp_deser = Deserializer::new(deser.context(), line);
                    result.metadata = ItemStackMetadata::deserialize(&mut tmp_deser)?;
                }
            }
        }
        Ok(result)
    }
}

// Custom deserialization as json blob
#[derive(Debug, Clone, PartialEq)]
pub struct ItemStackMetadata {
    pub string_vars: Vec<(ByteString, ByteString)>,
}

const DESERIALIZE_START: &[u8; 1] = b"\x01";
const DESERIALIZE_KV_DELIM: &[u8; 1] = b"\x02";
const DESERIALIZE_PAIR_DELIM: &[u8; 1] = b"\x03";

impl Serialize for ItemStackMetadata {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let mut buf: Vec<u8> = Vec::new();
        buf.extend(DESERIALIZE_START);
        for (key, val) in self.string_vars.iter() {
            if !key.is_empty() || !val.is_empty() {
                buf.extend(key.as_bytes());
                buf.extend(DESERIALIZE_KV_DELIM);
                buf.extend(val.as_bytes());
                buf.extend(DESERIALIZE_PAIR_DELIM);
            }
        }
        serialize_json_string_if_needed(&buf, |chunk| ser.write_bytes(chunk))?;
        Ok(())
    }
}

impl Deserialize for ItemStackMetadata {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let (raw, count) = deserialize_json_string_if_needed(deser.peek_all())?;
        deser.take(count)?;
        let mut result = Self {
            string_vars: Vec::new(),
        };
        let raw = &raw[..]; // easier to work with slice
        if raw.len() == 0 {
            return Ok(result);
        }
        if raw[0] != DESERIALIZE_START[0] {
            bail!(DeserializeError::InvalidValue(
                "ItemStackMetadata bad start".to_string(),
            ));
        }
        let mut raw = &raw[1..];
        // This is odd, but matches the behavior of ItemStackMetadata::deSerialize
        while raw.len() != 0 {
            let kv_delim_pos = raw
                .iter()
                .position(|ch| *ch == DESERIALIZE_KV_DELIM[0])
                .unwrap_or(raw.len());
            let name = &raw[..kv_delim_pos];
            raw = &raw[kv_delim_pos..];
            if raw.len() > 0 {
                raw = &raw[1..];
            }
            let pair_delim_pos = raw
                .iter()
                .position(|ch| *ch == DESERIALIZE_PAIR_DELIM[0])
                .unwrap_or(raw.len());
            let var = &raw[..pair_delim_pos];
            raw = &raw[pair_delim_pos..];
            if raw.len() > 0 {
                raw = &raw[1..];
            }
            result.string_vars.push((name.into(), var.into()));
        }
        Ok(result)
    }
}

/// This is the way ADD_PARTICLESPAWNER is serialized.
/// It seems to be an older version of ParticleParameters
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AddParticleSpawnerLegacy {
    pub amount: u16,
    pub time: f32,

    // start only
    pub pos_start: RangedParameterLegacy<v3f>,
    pub vel_start: RangedParameterLegacy<v3f>,
    pub acc_start: RangedParameterLegacy<v3f>,
    pub exptime_start: RangedParameterLegacy<f32>,
    pub size_start: RangedParameterLegacy<f32>,

    pub collision_detection: bool,
    pub texture_string: LongString,
    pub id: u32,
    pub vertical: bool,
    pub collision_removal: bool,
    pub attached_id: u16,
    pub animation: TileAnimationParams,
    pub glow: u8,
    pub object_collision: bool,
    pub node_param0: u16,
    pub node_param2: u8,
    pub node_tile: u8,

    // Only present in protocol_ver >= 40
    pub extra: Option<AddParticleSpawnerExtra>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AddParticleSpawnerExtra {
    pub pos_start_bias: f32,
    pub vel_start_bias: f32,
    pub acc_start_bias: f32,
    pub exptime_start_bias: f32,
    pub size_start_bias: f32,

    pub pos_end: RangedParameter<v3f>,
    pub vel_end: RangedParameter<v3f>,
    pub acc_end: RangedParameter<v3f>,
    pub exptime_end: RangedParameter<f32>,
    pub size_end: RangedParameter<f32>,

    pub texture: ServerParticleTextureNewPropsOnly,

    pub drag: TweenedParameter<RangedParameter<v3f>>,
    pub jitter: TweenedParameter<RangedParameter<v3f>>,
    pub bounce: TweenedParameter<RangedParameter<f32>>,
    pub attractor: Attractor, // attract_kind, followed by p.attract.serialize, p.attract_origin.ser, etc
    pub radius: TweenedParameter<RangedParameter<v3f>>,
    pub texpool: Array16<ServerParticleTexture>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Attractor {
    None,
    Point(PointAttractor),
    Line(LineAttractor),
    Plane(PlaneAttractor),
}

impl Serialize for Attractor {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let kind: u8 = match self {
            Attractor::None => 0,
            Attractor::Point(_) => 1,
            Attractor::Line(_) => 2,
            Attractor::Plane(_) => 3,
        };
        u8::serialize(&kind, ser)?;
        match self {
            Attractor::None => (),
            Attractor::Point(v) => Serialize::serialize(v, ser)?,
            Attractor::Line(v) => Serialize::serialize(v, ser)?,
            Attractor::Plane(v) => Serialize::serialize(v, ser)?,
        }
        Ok(())
    }
}

impl Deserialize for Attractor {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let kind = u8::deserialize(deser)?;
        Ok(match kind {
            0 => Attractor::None,
            1 => Attractor::Point(Deserialize::deserialize(deser)?),
            2 => Attractor::Line(Deserialize::deserialize(deser)?),
            3 => Attractor::Plane(Deserialize::deserialize(deser)?),
            _ => bail!("Invalid AttractorKind: {}", kind),
        })
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct PointAttractor {
    pub attract: TweenedParameter<RangedParameter<f32>>,
    pub origin: TweenedParameter<v3f>,
    pub attachment: u16,
    pub kill: u8,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct LineAttractor {
    pub attract: TweenedParameter<RangedParameter<f32>>,
    pub origin: TweenedParameter<v3f>,
    pub attachment: u16,
    pub kill: u8,
    pub direction: TweenedParameter<v3f>,
    pub direction_attachment: u16,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct PlaneAttractor {
    pub attract: TweenedParameter<RangedParameter<f32>>,
    pub origin: TweenedParameter<v3f>,
    pub attachment: u16,
    pub kill: u8,
    pub direction: TweenedParameter<v3f>,
    pub direction_attachment: u16,
}

/// This is serialized as part of a combined 'flags' field on
/// ServerParticleTexture, so it doesn't implement the  methods
/// on its own.
#[derive(Debug, Clone, PartialEq)]
pub enum BlendMode {
    Alpha,
    Add,
    Sub,
    Screen,
}

impl BlendMode {
    fn to_u8(&self) -> u8 {
        match self {
            BlendMode::Alpha => 0,
            BlendMode::Add => 1,
            BlendMode::Sub => 2,
            BlendMode::Screen => 3,
        }
    }

    fn from_u8(value: u8) -> DeserializeResult<BlendMode> {
        Ok(match value {
            0 => BlendMode::Alpha,
            1 => BlendMode::Add,
            2 => BlendMode::Sub,
            3 => BlendMode::Screen,
            _ => bail!("Invalid BlendMode u8: {}", value),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerParticleTextureNewPropsOnly {
    pub blend_mode: BlendMode,
    pub alpha: TweenedParameter<f32>,
    pub scale: TweenedParameter<v2f>,
    pub animation: Option<TileAnimationParams>,
}

impl Serialize for ServerParticleTextureNewPropsOnly {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let mut flags: u8 = self.blend_mode.to_u8() << 1;
        if self.animation.is_some() {
            flags |= 1;
        }
        u8::serialize(&flags, ser)?;
        Serialize::serialize(&self.alpha, ser)?;
        Serialize::serialize(&self.scale, ser)?;
        Serialize::serialize(&self.animation, ser)?;
        Ok(())
    }
}

impl Deserialize for ServerParticleTextureNewPropsOnly {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let flags: u8 = u8::deserialize(deser)?;
        let animated: bool = (flags & 1) != 0;
        let blend_mode = BlendMode::from_u8(flags >> 1)?;
        Ok(Self {
            blend_mode,
            alpha: Deserialize::deserialize(deser)?,
            scale: Deserialize::deserialize(deser)?,
            animation: if animated {
                Deserialize::deserialize(deser)?
            } else {
                None
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerParticleTexture {
    pub blend_mode: BlendMode,
    pub alpha: TweenedParameter<f32>,
    pub scale: TweenedParameter<v2f>,
    pub string: LongString,
    pub animation: Option<TileAnimationParams>,
}

impl Serialize for ServerParticleTexture {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let mut flags: u8 = self.blend_mode.to_u8() << 1;
        if self.animation.is_some() {
            flags |= 1;
        }
        u8::serialize(&flags, ser)?;
        Serialize::serialize(&self.alpha, ser)?;
        Serialize::serialize(&self.scale, ser)?;
        Serialize::serialize(&self.string, ser)?;
        Serialize::serialize(&self.animation, ser)?;
        Ok(())
    }
}

impl Deserialize for ServerParticleTexture {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let flags: u8 = u8::deserialize(deser)?;
        let animated: bool = (flags & 1) != 0;
        let blend_mode = BlendMode::from_u8(flags >> 1)?;
        Ok(Self {
            blend_mode,
            alpha: Deserialize::deserialize(deser)?,
            scale: Deserialize::deserialize(deser)?,
            string: Deserialize::deserialize(deser)?,
            animation: if animated {
                Deserialize::deserialize(deser)?
            } else {
                None
            },
        })
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum TweenStyle {
    Fwd,
    Rev,
    Pulse,
    Flicker,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct TweenedParameter<T: Serialize + Deserialize> {
    pub style: TweenStyle,
    pub reps: u16,
    pub beginning: f32,
    pub start: T,
    pub end: T,
}

/// This is the send format used by SendSpawnParticle
/// See ParticleParameters::serialize
#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct ParticleParameters {
    pub pos: v3f,
    pub vel: v3f,
    pub acc: v3f,
    pub expiration_time: f32,
    pub size: f32,
    pub collision_detection: bool,
    pub texture: LongString, // ServerParticleTexture.string
    pub vertical: bool,
    pub collision_removal: bool,
    pub animation: TileAnimationParams,
    pub glow: u8,
    pub object_collision: bool,
    // These are omitted in earlier protocol versions
    pub node_param0: Option<u16>,
    pub node_param2: Option<u8>,
    pub node_tile: Option<u8>,
    pub drag: Option<v3f>,
    pub jitter: Option<RangedParameter<v3f>>,
    pub bounce: Option<RangedParameter<f32>>,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct RangedParameter<T: Serialize + Deserialize> {
    pub min: T,
    pub max: T,
    pub bias: f32,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct RangedParameterLegacy<T: Serialize + Deserialize> {
    pub min: T,
    pub max: T,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct Lighting {
    pub shadow_intensity: f32,
    pub saturation: f32,
    pub exposure: AutoExposure,
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub struct AutoExposure {
    pub luminance_min: f32,
    pub luminance_max: f32,
    pub exposure_correction: f32,
    pub speed_dark_bright: f32,
    pub speed_bright_dark: f32,
    pub center_weight_power: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum HudSetParam {
    SetHotBarItemCount(s32),
    SetHotBarImage(String),
    SetHotBarSelectedImage(String),
}

impl Serialize for HudSetParam {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        use HudSetParam::*;
        let param: u16 = match self {
            SetHotBarItemCount(_) => 1,
            SetHotBarImage(_) => 2,
            SetHotBarSelectedImage(_) => 3,
        };
        Serialize::serialize(&param, ser)?;
        match self {
            SetHotBarItemCount(v) => {
                // The value is wrapped in a a String16
                u16::serialize(&4, ser)?;
                Serialize::serialize(v, ser)?;
            }
            SetHotBarImage(v) => Serialize::serialize(v, ser)?,
            SetHotBarSelectedImage(v) => Serialize::serialize(v, ser)?,
        };
        Ok(())
    }
}

impl Deserialize for HudSetParam {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        use HudSetParam::*;
        let param: u16 = Deserialize::deserialize(deser)?;
        Ok(match param {
            1 => {
                let size = u16::deserialize(deser)?;
                if size != 4 {
                    bail!("Invalid size in SetHotBarItemCount: {}", size);
                }
                SetHotBarItemCount(s32::deserialize(deser)?)
            }
            2 => SetHotBarImage(Deserialize::deserialize(deser)?),
            3 => SetHotBarSelectedImage(Deserialize::deserialize(deser)?),
            _ => bail!("Invalid HudSetParam param: {}", param),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct HudFlags {
    pub hotbar_visible: bool,
    pub healthbar_visible: bool,
    pub crosshair_visible: bool,
    pub wielditem_visible: bool,
    pub breathbar_visible: bool,
    pub minimap_visible: bool,
    pub minimap_radar_visible: bool,
    pub basic_debug: bool,
    pub chat_visible: bool,
}

impl HudFlags {
    pub fn to_u32(&self) -> u32 {
        let mut flags: u32 = 0;
        flags |= (self.hotbar_visible as u32) << 0;
        flags |= (self.healthbar_visible as u32) << 1;
        flags |= (self.crosshair_visible as u32) << 2;
        flags |= (self.wielditem_visible as u32) << 3;
        flags |= (self.breathbar_visible as u32) << 4;
        flags |= (self.minimap_visible as u32) << 5;
        flags |= (self.minimap_radar_visible as u32) << 6;
        flags |= (self.basic_debug as u32) << 7;
        flags |= (self.chat_visible as u32) << 8;
        flags
    }

    pub fn from_u32(flags: u32) -> Self {
        Self {
            hotbar_visible: (flags & (1 << 0)) != 0,
            healthbar_visible: (flags & (1 << 1)) != 0,
            crosshair_visible: (flags & (1 << 2)) != 0,
            wielditem_visible: (flags & (1 << 3)) != 0,
            breathbar_visible: (flags & (1 << 4)) != 0,
            minimap_visible: (flags & (1 << 5)) != 0,
            minimap_radar_visible: (flags & (1 << 6)) != 0,
            basic_debug: (flags & (1 << 7)) != 0,
            chat_visible: (flags & (1 << 8)) != 0,
        }
    }
}

impl Serialize for HudFlags {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        let value = self.to_u32();
        u32::serialize(&value, ser)
    }
}

impl Deserialize for HudFlags {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let value = u32::deserialize(deser)?;
        if (value & !0b111111111) != 0 {
            bail!("Invalid HudFlags: {}", value);
        }
        Ok(HudFlags::from_u32(value))
    }
}

#[derive(Debug, Clone, PartialEq, MinetestSerialize, MinetestDeserialize)]
pub enum InteractAction {
    StartDigging,
    StopDigging,
    DiggingCompleted,
    Place,
    Use,
    Activate,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PointedThing {
    Nothing,
    Node {
        under_surface: v3s16,
        above_surface: v3s16,
    },
    Object {
        object_id: u16,
    },
}

impl Serialize for PointedThing {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        // version, always 0
        u8::serialize(&0, ser)?;

        let typ: u8 = match self {
            PointedThing::Nothing => 0,
            PointedThing::Node { .. } => 1,
            PointedThing::Object { .. } => 2,
        };
        u8::serialize(&typ, ser)?;

        match self {
            PointedThing::Nothing => (),
            PointedThing::Node {
                under_surface,
                above_surface,
            } => {
                Serialize::serialize(under_surface, ser)?;
                Serialize::serialize(above_surface, ser)?;
            }
            PointedThing::Object { object_id } => {
                Serialize::serialize(object_id, ser)?;
            }
        }
        Ok(())
    }
}

impl Deserialize for PointedThing {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let ver = u8::deserialize(deser)?;
        if ver != 0 {
            bail!("Invalid PointedThing version: {}", ver);
        }
        let typ = u8::deserialize(deser)?;
        Ok(match typ {
            0 => PointedThing::Nothing,
            1 => PointedThing::Node {
                under_surface: Deserialize::deserialize(deser)?,
                above_surface: Deserialize::deserialize(deser)?,
            },
            2 => PointedThing::Object {
                object_id: Deserialize::deserialize(deser)?,
            },
            _ => bail!("Invalid PointedThing type: {}", typ),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InventoryAction {
    Move {
        count: u16,
        from_inv: InventoryLocation,
        from_list: String,
        from_i: s16,
        to_inv: InventoryLocation,
        to_list: String,
        to_i: Option<s16>,
    },
    Craft {
        count: u16,
        craft_inv: InventoryLocation,
    },
    Drop {
        count: u16,
        from_inv: InventoryLocation,
        from_list: String,
        from_i: s16,
    },
}

impl Serialize for InventoryAction {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        match self {
            InventoryAction::Move {
                count,
                from_inv,
                from_list,
                from_i,
                to_inv,
                to_list,
                to_i,
            } => {
                if to_i.is_some() {
                    ser.write_bytes(b"Move ")?;
                } else {
                    ser.write_bytes(b"MoveSomewhere ")?;
                }
                ser.write_bytes(itos!(count))?;
                ser.write_bytes(b" ")?;
                Serialize::serialize(from_inv, ser)?;
                ser.write_bytes(b" ")?;
                ser.write_bytes(from_list.as_bytes())?;
                ser.write_bytes(b" ")?;
                ser.write_bytes(itos!(from_i))?;
                ser.write_bytes(b" ")?;
                Serialize::serialize(to_inv, ser)?;
                ser.write_bytes(b" ")?;
                ser.write_bytes(to_list.as_bytes())?;
                if let Some(to_i) = to_i {
                    ser.write_bytes(b" ")?;
                    ser.write_bytes(itos!(to_i))?;
                }
            }
            InventoryAction::Craft { count, craft_inv } => {
                ser.write_bytes(b"Craft ")?;
                ser.write_bytes(itos!(count))?;
                ser.write_bytes(b" ")?;
                Serialize::serialize(craft_inv, ser)?;
                // This extra space is present in Minetest
                ser.write_bytes(b" ")?;
            }
            InventoryAction::Drop {
                count,
                from_inv,
                from_list,
                from_i,
            } => {
                ser.write_bytes(b"Drop ")?;
                ser.write_bytes(itos!(count))?;
                ser.write_bytes(b" ")?;
                Serialize::serialize(from_inv, ser)?;
                ser.write_bytes(b" ")?;
                ser.write_bytes(from_list.as_bytes())?;
                ser.write_bytes(b" ")?;
                ser.write_bytes(itos!(from_i))?;
            }
        }
        Ok(())
    }
}

impl Deserialize for InventoryAction {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let word = deser.take_word(true);
        if word == b"Move" || word == b"MoveSomewhere" {
            Ok(InventoryAction::Move {
                count: stoi(deser.take_word(true))?,
                from_inv: Deserialize::deserialize(deser)?,
                from_list: std::str::from_utf8(deser.take_word(true))?.to_owned(),
                from_i: stoi(deser.take_word(true))?,
                to_inv: Deserialize::deserialize(deser)?,
                to_list: std::str::from_utf8(deser.take_word(true))?.to_owned(),
                to_i: if word == b"Move" {
                    Some(stoi(deser.take_word(true))?)
                } else {
                    None
                },
            })
        } else if word == b"Drop" {
            Ok(InventoryAction::Drop {
                count: stoi(deser.take_word(true))?,
                from_inv: Deserialize::deserialize(deser)?,
                from_list: std::str::from_utf8(deser.take_word(true))?.to_owned(),
                from_i: stoi(deser.take_word(true))?,
            })
        } else if word == b"Craft" {
            Ok(InventoryAction::Craft {
                count: stoi(deser.take_word(true))?,
                craft_inv: Deserialize::deserialize(deser)?,
            })
        } else {
            bail!("Invalid InventoryAction kind");
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InventoryLocation {
    Undefined,
    CurrentPlayer,
    Player { name: String },
    NodeMeta { pos: v3s16 },
    Detached { name: String },
}

impl Serialize for InventoryLocation {
    fn serialize<S: Serializer>(&self, ser: &mut S) -> SerializeResult {
        match self {
            InventoryLocation::Undefined => ser.write_bytes(b"undefined")?,
            InventoryLocation::CurrentPlayer => ser.write_bytes(b"current_player")?,
            InventoryLocation::Player { name } => {
                ser.write_bytes(b"player:")?;
                ser.write_bytes(name.as_bytes())?;
            }
            InventoryLocation::NodeMeta { pos } => {
                ser.write_bytes(format!("nodemeta:{},{},{}", pos.x, pos.y, pos.z).as_bytes())?;
            }
            InventoryLocation::Detached { name } => {
                ser.write_bytes(b"detached:")?;
                ser.write_bytes(name.as_bytes())?;
            }
        }
        Ok(())
    }
}

impl Deserialize for InventoryLocation {
    fn deserialize(deser: &mut Deserializer) -> DeserializeResult<Self> {
        let word = deser.take_word(true);
        if word == b"undefined" {
            return Ok(InventoryLocation::Undefined);
        } else if word == b"current_player" {
            return Ok(InventoryLocation::CurrentPlayer);
        } else if word.starts_with(b"player:") {
            return Ok(InventoryLocation::Player {
                name: std::str::from_utf8(&word[7..])?.to_string(),
            });
        } else if word.starts_with(b"nodemeta:") {
            let coords: Vec<&[u8]> = word[9..].split(|&ch| ch == b',').collect();
            if coords.len() != 3 {
                bail!("Corrupted nodemeta InventoryLocation");
            }
            let mut xyz = [0i16; 3];
            for (i, &n) in coords.iter().enumerate() {
                xyz[i] = stoi(n)?;
            }
            let pos = v3s16::new(xyz[0], xyz[1], xyz[2]);
            return Ok(InventoryLocation::NodeMeta { pos });
        } else if word.starts_with(b"detached:") {
            return Ok(InventoryLocation::Detached {
                name: std::str::from_utf8(&word[9..])?.to_string(),
            });
        } else {
            bail!("Unknown InventoryLocation: {:?}", word)
        }
    }
}
