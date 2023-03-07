//! Audit
//!
//! When auditing is enabled, every deserialized Packet or Command is immediately
//! re-serialized, and the results compared byte-by-byte. Any difference is a
//! fatal error.
//!
//! This is useful during development, to verify that new ser/deser methods are correct.
//!
//! But it should not be enabled normally, because a malformed packet from a
//! broken/modified client will cause a crash.

use anyhow::bail;
use anyhow::Result;

use super::command::CommandRef;
use super::command::ToClientCommand;
use super::ser::Serialize;
use super::ser::VecSerializer;
use super::types::ProtocolContext;
use super::util::decompress_zlib;
use super::util::zstd_decompress;
use std::sync::atomic::AtomicBool;

static AUDIT_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn audit_on() {
    AUDIT_ENABLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn audit_command<Cmd: CommandRef>(context: ProtocolContext, orig: &[u8], command: &Cmd) {
    if !AUDIT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let mut ser = VecSerializer::new(context, 2 * orig.len());
    match Serialize::serialize(command, &mut ser) {
        Ok(_) => (),
        Err(err) => {
            println!("AUDIT: Reserialization failed");
            println!("AUDIT: ORIGINAL = {:?}", orig);
            println!("AUDIT: PARSED = {:?}", command);
            println!("ERR = {:?}", err);
            std::process::exit(1);
        }
    }
    let reser = ser.take();
    let reser = reser.as_slice();
    match audit_command_inner(context, orig, reser, command) {
        Ok(_) => (),
        Err(err) => {
            println!("AUDIT: Unknown error occurred auditing of command");
            println!("AUDIT: PARSED = {:?}", command);
            println!("AUDIT: ORIGINAL     = {:?}", orig);
            println!("AUDIT: RESERIALIZED = {:?}", reser);
            println!("ERR = {:?}", err);
            std::process::exit(1);
        }
    }
}

fn audit_command_inner<Cmd: CommandRef>(
    context: ProtocolContext,
    orig: &[u8],
    reser: &[u8],
    command: &Cmd,
) -> Result<()> {
    // zstd or zlib re-compression is not guaranteed to be the same,
    // so handle these separately.
    match command.toclient_ref() {
        Some(ToClientCommand::Blockdata(_)) => {
            if context.ser_fmt >= 29 {
                // Layout in format 29 and above:
                //
                //   command type: u16
                //   pos: v3s16, (6 bytes)
                //   datastring: ZStdCompressed<MapBlock>,
                //   network_specific_version: u8
                do_compare(
                    "BlockData prefix (ver>=29)",
                    &reser[..8],
                    &orig[..8],
                    command,
                );
                do_compare(
                    "BlockData suffix (ver>=29)",
                    &reser[reser.len() - 1..reser.len()],
                    &orig[orig.len() - 1..orig.len()],
                    command,
                );
                let reser = zstd_decompress_to_vec(&reser[8..reser.len() - 1])?;
                let orig = zstd_decompress_to_vec(&orig[8..orig.len() - 1])?;
                do_compare("Blockdata contents (ver>=29)", &reser, &orig, command);
            } else {
                // Layout in ver 28:
                //
                //   command type: u16         (2 bytes)
                //   pos: v3s16                (6 bytes)
                //   flags: u8                 (1 byte)
                //   lighting_complete: u16    (2 bytes)
                //   content_width: u8         (1 byte)
                //   param_width: u8           (1 byte)
                //   nodes: ZLibCompressed     (var size)
                //   metadata: ZLibCompressed  (var size)
                //   network_specific_version  (1 byte)
                do_compare(
                    "BlockData prefix (ver==28)",
                    &reser[..13],
                    &orig[..13],
                    command,
                );
                do_compare(
                    "BlockData suffix (ver==28)",
                    &reser[reser.len() - 1..],
                    &orig[orig.len() - 1..],
                    command,
                );

                let reser_contents = {
                    let (consumed1, nodes_raw) = decompress_zlib(&reser[13..])?;
                    let (consumed2, metadata_raw) = decompress_zlib(&reser[13 + consumed1..])?;
                    if 13 + consumed1 + consumed2 + 1 != reser.len() {
                        bail!("Reserialized command does not have the right size")
                    }
                    (nodes_raw, metadata_raw)
                };
                let orig_contents = {
                    let (consumed1, nodes_raw) = decompress_zlib(&orig[13..])?;
                    let (consumed2, metadata_raw) = decompress_zlib(&orig[13 + consumed1..])?;
                    if 13 + consumed1 + consumed2 + 1 != orig.len() {
                        bail!("Original command does not seem to have the right size")
                    }
                    (nodes_raw, metadata_raw)
                };
                do_compare(
                    "Uncompressed nodes (ver 28)",
                    &reser_contents.0,
                    &orig_contents.0,
                    command,
                );
                do_compare(
                    "Uncompressed node metadata (ver 28)",
                    &reser_contents.1,
                    &orig_contents.1,
                    command,
                );
            }
        }
        Some(ToClientCommand::NodemetaChanged(_))
        | Some(ToClientCommand::Itemdef(_))
        | Some(ToClientCommand::Nodedef(_)) => {
            // These contain a single zlib-compressed value.
            // The prefix is a u16 command type, followed by u32 zlib size.
            let reser = zlib_decompress_to_vec(&reser[6..]);
            let orig = zlib_decompress_to_vec(&orig[6..]);
            do_compare("zlib decompressed body", &reser, &orig, command);
        }
        _ => {
            do_compare("default", reser, orig, command);
        }
    };
    Ok(())
}

fn do_compare<Cmd: CommandRef>(what: &str, reser: &[u8], orig: &[u8], command: &Cmd) {
    if reser != orig {
        println!(
            "AUDIT: Mismatch between original and re-serialized ({})",
            what
        );
        println!("AUDIT: ORIGINAL     = {:?}", orig);
        println!("AUDIT: RESERIALIZED = {:?}", reser);
        println!("AUDIT: PARSED = {:?}", command);
        std::process::exit(1);
    }
}

fn zlib_decompress_to_vec(compressed: &[u8]) -> Vec<u8> {
    match miniz_oxide::inflate::decompress_to_vec_zlib(compressed) {
        Ok(uncompressed) => uncompressed,
        Err(_) => {
            println!("AUDIT: Decompression failed unexpectedly");
            std::process::exit(1);
        }
    }
}

fn zstd_decompress_to_vec(compressed: &[u8]) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    zstd_decompress(compressed, |chunk| {
        result.extend(chunk);
        Ok(())
    })?;
    Ok(result)
}
