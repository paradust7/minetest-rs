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

use super::command::CommandRef;
use super::command::ToClientCommand;
use super::ser::Serialize;
use super::ser::VecSerializer;
use std::sync::atomic::AtomicBool;

static AUDIT_ENABLED: AtomicBool = AtomicBool::new(false);

pub fn audit_on() {
    AUDIT_ENABLED.store(true, std::sync::atomic::Ordering::SeqCst);
}

pub fn audit_command<Cmd: CommandRef>(orig: &[u8], command: &Cmd) {
    if !AUDIT_ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let dir = command.direction();
    let mut ser = VecSerializer::new(dir, 2 * orig.len());
    match Serialize::serialize(command, &mut ser) {
        Ok(_) => (),
        Err(_) => {
            println!("AUDIT: Reserialization failed");
            println!("AUDIT: ORIGINAL = {:?}", orig);
            println!("AUDIT: PARSED = {:?}", command);
            std::process::exit(1);
        }
    }
    let reser = ser.take();
    let reser = reser.as_slice();

    // zstd or zlib re-compression is not guaranteed to be the same,
    // so handle these separately.
    match command.toclient_ref() {
        Some(ToClientCommand::Blockdata(_)) => {
            // Layout of raw binary:
            //   command type: u16
            //   pos: v3s16, (6 bytes)
            //   datastring: ZStdCompressed<MapBlock>,
            //   network_specific_version: u8
            do_compare(&reser[..8], &orig[..8], command);
            do_compare(
                &reser[reser.len() - 1..reser.len()],
                &orig[orig.len() - 1..orig.len()],
                command,
            );
            let reser = zstd_decompress(&reser[8..reser.len() - 1]);
            let orig = zstd_decompress(&orig[8..orig.len() - 1]);
            do_compare(&reser, &orig, command);
        }
        Some(ToClientCommand::NodemetaChanged(_))
        | Some(ToClientCommand::Itemdef(_))
        | Some(ToClientCommand::Nodedef(_)) => {
            // These contain a single zlib-compressed value.
            // The prefix is a u16 command type, followed by u32 zlib size.
            let reser = zlib_decompress(&reser[6..]);
            let orig = zlib_decompress(&orig[6..]);
            do_compare(&reser, &orig, command);
        }
        _ => {
            do_compare(reser, orig, command);
        }
    };
}

fn do_compare<Cmd: CommandRef>(reser: &[u8], orig: &[u8], command: &Cmd) {
    if reser != orig {
        println!("AUDIT: Mismatch between original and re-serialized");
        println!("AUDIT: ORIGINAL     = {:?}", orig);
        println!("AUDIT: RESERIALIZED = {:?}", reser);
        println!("AUDIT: PARSED = {:?}", command);
        std::process::exit(1);
    }
}

fn zlib_decompress(compressed: &[u8]) -> Vec<u8> {
    match miniz_oxide::inflate::decompress_to_vec_zlib(compressed) {
        Ok(uncompressed) => uncompressed,
        Err(_) => {
            println!("AUDIT: Decompression failed unexpectedly");
            std::process::exit(1);
        }
    }
}

fn zstd_decompress(compressed: &[u8]) -> Vec<u8> {
    let mut result: Vec<u8> = Vec::new();
    let _ = zstd_safe::decompress(&mut result, compressed);
    result
}
