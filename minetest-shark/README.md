# Minetest-shark

Minetest proxy with detailed inspection of protocol

Example usage:
```
$ cargo install minetest-shark
```
```
# Listen on port 40000, forward to localhost port 30000, verbosity 1
$ mtshark -l 40000 -t 127.0.0.1:30000 -v
```
```
MinetestServer starting on 0.0.0.0:40000
MinetestServer started
MinetestServer accepted connection
[P1] New client connected from 127.0.0.1:34997
[1] C->S  Null
[1] C->S  Init
[1] S->C  Hello
[1] C->S  SrpBytesA
[1] S->C  SrpBytesSB
[1] C->S  SrpBytesM
[1] S->C  AuthAccept
[1] C->S  Init2
[1] S->C  Itemdef
[1] S->C  Nodedef
[1] S->C  AnnounceMedia
[1] S->C  DetachedInventory
[1] S->C  DetachedInventory
...
```

```
$ mtshark -l 40000 -t 127.0.0.1:30000 -vv
```
```
MinetestServer starting on 0.0.0.0:40000
MinetestServer started
MinetestServer accepted connection
[P1] New client connected from 127.0.0.1:56772
[1] C->S  Null(
    NullSpec,
)
[1] C->S  Init(
    InitSpec {
        serialization_ver_max: 29,
        supp_compr_modes: 0,
        min_net_proto_version: 37,
        max_net_proto_version: 41,
        player_name: "paradust",
    },
)
[1] S->C  Hello(
    HelloSpec {
        serialization_ver: 29,
        compression_mode: 0,
        proto_ver: 41,
        auth_mechs: AuthMechsBitset {
            legacy_password: false,
            srp: true,
            first_srp: false,
        },
        username_legacy: "paradust",
    },
)
[1] C->S  SrpBytesA(
    SrpBytesASpec {
        bytes_a: BinaryData16 {
            data: [
                164,
                91,
                54,
....
```

# Verbosity levels
```
default   Shows connects/disconnects only
-v        Command names
-vv       Command contents (except for bulk commands)
-vvv      Everything
```

