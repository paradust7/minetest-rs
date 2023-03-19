# minetest-protocol
Pure Rust implementation of the Minetest protocol.

Supported functionality:

- Serialization &amp; deserialization of packets and commands
- Minetest commands as strongly-typed struct's and enums
- The peer protocol
    - Channels
    - Packet splitting &amp; split packet reconstruction
    - Reliable packet retries &amp; ACK tracking
    - peer_id tracking

This is a library and does not contain any programs. For an
example of how to use this library, see the `minetest-shark` crate.

# Work in progress

- Documentation is incomplete &amp; unreviewed.

- Reliable packet delivery transmission window size is fixed for now.

- Non-reliable split reconstruction timeout not enabled yet.
