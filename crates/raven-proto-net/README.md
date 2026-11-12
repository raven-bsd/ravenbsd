<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-proto-net

Wire contract between the `net:` scheme server (`netd`), its clients (through `ralibc`) and NIC
drivers. It exists so that `netd`, virtio-net and the other drivers can be written in parallel
against one versioned layout (`architecture/03-system-servers.md` section 2,
`architecture/07-network.md` sections 1 and 3).

The crate is `no_std`, has one dependency (`raven-abi` for errno values), forbids `unsafe`, never
allocates in an encoder and never panics while decoding. Every byte table below is pinned by a
golden test in `tests/golden.rs` or in the module tests.

## Rules

* All integers are **little-endian**. No implicit padding: every field is packed explicitly.
* Every message starts with `u16 version = 1`. A different version is rejected with
  `DecodeError::UnsupportedVersion`; it is never reinterpreted.
* Every length is bounded before it is used: scheme path 64 bytes, interface name 16 bytes,
  `READ`/`WRITE` payload and UDP datagram 65 535 bytes, `LISTEN` backlog 4096, ring entries 4096,
  shared region 16 MiB.
* Handles never travel inside the wire body. `OPEN` returns a kernel SchemeFile; `ACCEPT` returns
  a numeric socket id, and the client then opens `net:accept/<id>` to materialise that socket's
  SchemeFile. This is the ABI-v1 accept path because the kernel has no server-side SchemeFile
  create primitive (follow-up for the ABI/S25 contract). A `CALL` reply may carry an S25
  attachment, used by the readiness-event operation below.
* Accepted-socket path: `net:accept/<decimal-u32>`; the parser is
  `raven_proto_net::path::parse_accepted` and the prefix is
  `raven_proto_net::path::ACCEPT_PREFIX`.
* Errno values in responses are `raven-abi` errno numbers (Linux values, DR-13).
* Decoders consume exactly the whole input; trailing bytes are an error, so a short read is never
  silently accepted.

## Socket requests

Common header:

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `version` = 1 |
| 2 | 2 | `kind` |

Kinds: `1` OPEN, `2` READ, `3` WRITE, `4` CALL.

| Kind | Body after offset 4 |
|---|---|
| OPEN | `path_len: u16`, then `path_len` bytes of UTF-8 `net:` path (`1..=64`) |
| READ | `len: u32` (`0..=65535`) |
| WRITE | `flags: u8` (bit 0 = destination follows), `len: u32`, optional endpoint, then `len` bytes |
| CALL | `op: u16`, then the operation arguments below |

Endpoint record (6 bytes): `ip: [u8; 4]` then `port: u16`.

Example OPEN of `net:tcp/10.0.0.2:80`:

```text
01 00 01 00 13 00 6e 65 74 3a 74 63 70 2f 31 30 2e 30 2e 30 2e 32 3a 38 30
version  kind  len   n  e  t  :  t  c  p  /  1  0  .  0  .  0  .  2  :  8  0
```

Example WRITE of `dns` to `192.0.2.7:53`:

```text
01 00 03 00 01 03 00 00 00 c0 00 02 07 35 00 64 6e 73
version  kind  ep len        ip        port  d  n  s
```

### CALL operations

| Op | Name | Arguments after `op: u16` |
|---|---|---|
| 1 | BIND | endpoint (6) |
| 2 | LISTEN | `backlog: u16` (`1..=4096`) |
| 3 | ACCEPT | none; returns `SOCKET(id)` and the client opens `net:accept/<id>` |
| 4 | CONNECT | endpoint (6) |
| 5 | SHUTDOWN | `how: u8` (0 read, 1 write, 2 both) |
| 6 | GETSOCKNAME | none |
| 7 | GETPEERNAME | none |
| 8 | SETSOCKOPT | `opt: u16`, `value: i32` (non-negative) |
| 9 | GETSOCKOPT | `opt: u16` |
| 10 | EVENT | none; reply is `OK` inline data plus one S25 `UserEvent` attachment |

Socket option ids (native protocol ids, not Linux `SOL_SOCKET` numbers):

| Id | Option | SETSOCKOPT | GETSOCKOPT |
|---|---|---|---|
| 1 | `SO_REUSEADDR` | yes | yes |
| 2 | `SO_RCVBUF` | yes | yes |
| 3 | `SO_SNDBUF` | yes | yes |
| 4 | `TCP_NODELAY` | yes | yes |
| 5 | `SO_ERROR` | no (read-only) | yes |

`SETSOCKOPT` with `SO_ERROR`, a negative value or an unknown id is rejected while decoding.

## Socket responses

| Offset | Size | Field |
|---|---|---|
| 0 | 2 | `version` = 1 |
| 2 | 2 | `status`: 0 on success, otherwise the `raven-abi` errno |
| 4 | 2 | `tag` |

| Tag | Body |
|---|---|
| 0 OK | none |
| 1 SOCKET | `id: u32` |
| 2 READ | `flags: u8` (bit 0 = source endpoint), `len: u32`, optional endpoint |
| 3 WRITE | `len: u32` |
| 4 ENDPOINT | endpoint (6) |
| 5 VALUE | `value: i32` |
| 6 ERR | none; `status` carries the errno and must be non-zero |

`EAGAIN` (11) is therefore `01 00 0b 00 06 00`. A tag other than ERR with a non-zero status, or
ERR with status 0, is malformed.
`EVENT` replies with `OK` followed by one S25 attachment pair `[u32 user_event][u32 rights]`; the
client registers the transferred UserEvent in an S26 event queue with `SIGNALED|EDGE` and waits
for readable/writable/accepted/closed transitions instead of polling.

## Interface control

The control channel is `net:`. Its requests use the same `version: u16`, `op: u16`, body shape.

| Op | Name | Body |
|---|---|---|
| 1 | LIST | none (one `Count`, then that many `Iface` records) |
| 2 | GET | name |
| 3 | SET_ADDRESS | name, `addr: [u8; 4]`, `prefix: u8` (`0..=32`) |
| 4 | SET_STATE | name, `up: u8` (0 or 1) |
| 5 | ROUTE_ADD | `dest: [u8; 4]`, `prefix: u8`, `gateway_flag: u8`, optional `gateway: [u8; 4]` |
| 6 | ROUTE_DEL | `dest: [u8; 4]`, `prefix: u8` |
| 7 | ARP_LIST | none (one `Count`, then that many `Arp` records) |

A name is `len: u8` (`1..=16`) followed by ASCII letters, digits, underscore, hyphen or dot. The
netmask is a prefix length, not a dotted mask. A default route is `dest 0.0.0.0`, `prefix 0`.

Example SET_ADDRESS `eth0` `10.0.0.2/24`:

```text
01 00 03 00 04 65 74 68 30 0a 00 00 02 18
version  op   len e  t  h  0  addr        /24
```

Responses use `version: u16`, `status: u16`, `tag: u16`, body:

| Tag | Body |
|---|---|
| 0 OK | none |
| 1 COUNT | `count: u16` |
| 2 IFACE | name, `addr: [u8;4]`, `prefix: u8`, `flags: u8`, `mtu: u16`, `mac: [u8;6]` |
| 3 ROUTE | `dest`, `prefix`, `gateway_flag`, optional gateway |
| 4 ARP | `ip: [u8;4]`, `mac: [u8;6]`, `flags: u8` |
| 5 ERR | none; `status` carries the errno |

Flags: interface `UP=0x1`, `LINK_UP=0x2`, `LOOPBACK=0x4`; ARP `COMPLETE=0x1`, `STATIC=0x2`.

## Driver rings

`netd` and a driver share one region created by the kernel (`dma_alloc`, S21b). The first 56 bytes
are the header:

| Offset | Size | Field | Writer |
|---|---|---|---|
| 0 | 8 | magic `RAVENNET` | creator |
| 8 | 2 | version = 1 | creator |
| 10 | 2 | header length = 56 | creator |
| 12 | 2 | `rx_count` | creator |
| 14 | 2 | `tx_count` | creator |
| 16 | 4 | `rx_prod` | driver |
| 20 | 4 | `rx_cons` | `netd` |
| 24 | 4 | `tx_prod` | `netd` |
| 28 | 4 | `tx_cons` | driver |
| 32 | 8 | `rx_doorbell` (UserEvent id, S26) | creator |
| 40 | 8 | `tx_doorbell` | creator |
| 48 | 4 | `pool_offset` | creator |
| 52 | 4 | `pool_len` | creator |

A descriptor is 16 bytes:

| Offset | Size | Field |
|---|---|---|
| 0 | 8 | `addr`: byte offset inside this region (not a process address) |
| 8 | 4 | `len`: valid bytes |
| 12 | 4 | `flags`: FIRST=0x1, LAST=0x2, CHECKSUM_OK=0x4, ERROR=0x8 |

Layout and bounds:

```text
rx descriptors at 56
tx descriptors at 56 + rx_count * 16
pool_offset  = align_up(56 + (rx_count + tx_count) * 16, 2048)
pool_len     = region_len - pool_offset   (a positive multiple of 2048)
buffer i     at pool_offset + i * 2048    (i < pool_len / 2048)
```

`rx_count` and `tx_count` are in `1..=4096`, `pool_len / 2048 >= 1`, and `region_len <= 16 MiB`.
A header whose magic, version, header length, offset alignment or bounds do not match is
rejected by `RingHeader::read_from` / `RingLayout::from_header`.

Indices are monotonic `u32` counters: slot = index % count, `used = prod.wrapping_sub(cons)`,
full when `used >= count`, empty when `prod == cons`. The region contains at least one buffer
beyond the rings; frames are split into FIRST/LAST runs when they span buffers.

### Memory ordering

Single producer, single consumer, one writer per index:

1. **Producer**: fill the buffer, write the descriptor, `fence(Release)`, store `prod` with a
   normal store, then signal the doorbell. The release fence makes the descriptor write visible
   before the index that publishes it.
2. **Consumer**: after waking on the doorbell, `fence(Acquire)`, read `prod`, read the
   descriptor, `fence(Release)`, store `cons`. The acquire fence makes the descriptor visible
   after the index; the release fence retires the reads before the slot is handed back.
3. Only the writer named in the table stores an index. No side writes another side's index,
   and the indices are never reset while the ring is live.
4. Doorbells are S26 user events: one `event_signal` after a batch, not one per descriptor.

`RingProducer` and `RingConsumer` implement exactly this contract. Because the crate forbids
`unsafe`, they read and write indices through ordinary byte loads/stores and use `fence`; a
component that needs Rust atomics (for example a future in-kernel ring) must provide the same
ordering around the same bytes. The helpers are not safe to share between two threads inside one
process without external synchronisation; the host test uses a mutex, and the two real users are
separate processes with separate mappings.

## Scheme paths

| Path | Meaning |
|---|---|
| `net:` | interface control channel |
| `net:tcp/<ipv4>:<port>` | TCP socket to a remote endpoint |
| `net:udp/<ipv4>:<port>` | UDP socket |

IPv4 octets are decimal `0..=255` without leading zeros; the port is `1..=65535`. Anything else
is a `PathError`. `OPEN` validates the path before it is accepted.

## Tests

```text
cargo test -p raven-proto-net
cargo clippy -p raven-proto-net --all-targets -- -D warnings
cargo build -p raven-proto-net --target x86_64-unknown-none --release
```

The suite covers golden bytes for every message family, round-trips, truncation, trailing bytes,
oversized lengths, reserved bits, 4,000 random inputs per decoder, ring layout/offset checks,
wrap-around, full/empty, and a two-thread producer/consumer run of 10,000 descriptors.
