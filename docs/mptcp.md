<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Multipath TCP core (phase 1)

Status: **phase 1 protocol core**. `crates/raven-mptcp` is a standalone, `no_std`, host-testable
implementation of RFC 8684 v1 signalling. It does not touch `crates/raven-tcpip`, the kernel, the
release profiles or any feature flag; phase 2 (subflow integration) is a separate ticket.

## Scope

* `MP_CAPABLE` (RFC 8684 3.1): version 1, key exchange, checksum and deny-join flags, and the
  SYN/SYN-ACK/ACK state machine with key echo validation.
* `MP_JOIN` (3.2): receiver token, nonces, Address ID, backup flag, HMAC-SHA256 in both directions
  (leftmost 64 bits on SYN/ACK, leftmost 160 bits on the third ACK), with constant-time tag checks.
* `DSS` (3.3): Data ACK (32/64), data sequence mapping (32/64 DSN), DATA_FIN, infinite mapping and
  the TCP-style mapping checksum over the RFC pseudo-header.
* `ADD_ADDR`, `REMOVE_ADDR`, `MP_PRIO`, `MP_FAIL`, `MP_FASTCLOSE`, `MP_TCPRST` (3.4-3.7).
* A bounded, transport-independent path manager: announce/remove/priority/selection, with regular
  paths preferred over backup paths (3.3.8).
* SHA-256, HMAC-SHA256, token and IDSN derivations are first-party (FIPS 180-4 / RFC 2104).

The MPTCP option kind is 30. All lengths below include the two-byte kind/length header.

| Option | Subtype | Lengths implemented |
|---|---:|---|
| MP_CAPABLE | 0 | 4, 12, 20, 22, 24 |
| MP_JOIN | 1 | 12 (SYN), 16 (SYN/ACK), 24 (ACK) |
| DSS | 2 | 4 + optional ACK + optional mapping + optional checksum |
| ADD_ADDR | 3 | IPv4 8/10/16/18; IPv6 20/22/28/30 |
| REMOVE_ADDR | 4 | 4..=11 (1..=8 Address IDs) |
| MP_PRIO | 5 | 3 |
| MP_FAIL | 6 | 12 |
| MP_FASTCLOSE | 7 | 12 |
| MP_TCPRST | 8 | 4 |

## Decoder contract

`MptcpOption::parse` and `option::parse_options` are total over arbitrary bytes: a truncated,
unknown, reserved-bit or length-mismatched option returns `raven_mptcp::Error`; no input can make
them panic. `tests/mptcp/fuzz` is the cargo-fuzz target and `tests/mptcp/tests/fuzz_smoke.rs` runs
the same invariant deterministically on 10,000 arbitrary blocks.

The fuzz target found one defect during development: a three-byte DSS header indexed the missing
flags byte. The regression is
`option::tests::a_three_byte_dss_header_is_rejected_not_indexed` (input
`01 1e 03 20 ff ff 59`), covered by the unit and vector suites.

## Donor map

RFC 8684 is normative. RFC 6824 is consulted for v0 background; v1 only is implemented and a v0
MP_CAPABLE is rejected with `UnsupportedVersion` (fallback to TCP is the caller's decision).

| Source | Licence | Files/functions read | Used how |
|---|---|---|---|
| RFC 8684 | IETF text | 3.1 MP_CAPABLE, 3.2 MP_JOIN, 3.3 DSS/ACK/FIN, 3.4 ADD/REMOVE_ADDR, 3.5 FASTCLOSE, 3.6 MP_TCPRST, 3.7 MP_FAIL, 7.2 subtypes, 7.4 reason codes | normative wire and state behaviour; first-party Rust |
| Linux `net/mptcp/protocol.h` | GPL-2.0 | option lengths, subtype values, flag bit positions, HMAC length | read-only oracle; no code copied |
| Linux `net/mptcp/options.c` | GPL-2.0 | `mptcp_parse_option`, `mptcp_write_options`, DSS/ADD_ADDR layouts | read-only oracle; no code copied |
| Linux `net/mptcp/crypto.c` | GPL-2.0 | `mptcp_crypto_key_sha`, `mptcp_crypto_hmac_sha` ordering | read-only oracle; no code copied |
| Linux `net/mptcp/pm.c`, `pm_netlink.c` | GPL-2.0 | path announce/remove/priority behaviour | read-only oracle; no code copied |
| Linux `net/mptcp/protocol.c` | GPL-2.0 | connection send/receive queues and DATA_ACK behaviour | read-only oracle; no code copied |
| Linux `net/mptcp/sched.c` | GPL-2.0 | active/backup subflow selection | read-only oracle; no code copied |
| FreeBSD RACK | BSD-2 | send/receive hook shape only (`tcp_output.c`/`tcp_input.c` RACK paths) | later phase 2 structure, no code here |
| Illumos | CDDL | no MPTCP implementation | no code read or copied |

No donor code was copied or translated. The Linux files are GPL and were read for field
order/lengths only; the implementation is independent Rust from the RFC.

## Tests

```sh
timeout 300 cargo nextest run -p raven-mptcp
timeout 300 cargo nextest run -p raven-mptcp-vectors
RUSTUP_TOOLCHAIN=nightly timeout 180 cargo fuzz run mptcp_options -- -runs=10000
timeout 900 make check-fast
```

Byte-exact vectors live in `tests/mptcp/tests/vectors.rs`; unit tests live in each
`crates/raven-mptcp/src/*_ut.rs`. The vector table covers every implemented subtype, every prefix
of every vector must fail, and the fuzz replay checks accepted bytes re-encode canonically.

## Meta-socket simulation (MPTCP1c)

`meta::MetaSocket` is the connection-level data plane above the wire format. It is driven by the
object-safe `meta::Subflow` trait, so a mock transport can exercise every failure mode without a
kernel:

* the send buffer is keyed by data sequence number; `send` chunks by MSS within the configured
  send cap and `pump` assigns unsent ranges to the active path;
* one active path is chosen (first alive regular path, otherwise an alive backup). A backup
  carries no data while a regular path is alive; `set_backup`/`set_alive` re-run the scheduler;
* a dead path's unacknowledged ranges are requeued and sent once on the new active path;
  the receiver trims duplicate/overlapping DSN ranges, so the application sees each byte once;
* the receive side buffers out-of-order ranges within the connection window and memory cap,
  delivers in order, and tracks the connection Data ACK (`data_ack` = next expected DSN);
* `close` sends a DATA_FIN, `on_mp_fail` discards data at/after the failed DSN (keeping a valid
  prefix), and `on_fastclose` aborts and clears buffers;
* every queue is bounded by the constructor caps; a full send buffer is `WouldBlock`, an
  out-of-window range is `WindowViolation`, and over-cap receive memory is `MemoryLimit`.

Scenario matrix (`crates/raven-mptcp/src/meta_ut.rs`, 11 tests):

| Scenario | Assertion |
|---|---|
| backup while primary alive | only the regular subflow is used |
| out-of-order + duplicates | byte-exact stream, one copy, ACK advances |
| primary dies mid-stream | only unacked DSNs are reinjected on the backup, once |
| slow/erroring primary | range stays pending, backup takes it when retired |
| priority change | new primary carries subsequent data |
| simultaneous FIN | both peers see DATA_FIN, read EOF |
| MP_FAIL | data at/after the failed DSN is discarded, prefix kept |
| fastclose | connection closes and buffers clear |
| memory/window bounds | `WouldBlock`/`MemoryLimit`/`WindowViolation`, queue within cap |

The mock subflows live only in `meta_ut.rs`; `raven-tcpip` and the kernel are untouched. MPTCP2
adapts the real TCP subflow to the same `Subflow` trait.

## Transport bridge (MPTCP2a)

`raven-tcpip` now has an opt-in `mptcp` feature (`crates/raven-tcpip/src/mptcp_bridge.rs`). It
implements `raven_mptcp::Subflow` over the real plain-TCP `SubflowOps`:

* `StackSubflow<D>` wraps a shared `Rc<RefCell<Stack<D>>>` and a `SubflowId`; `send` queues on the
  real TCP socket, `readiness` calls the same live `SubflowOps::readiness` used by the TCP stack,
  and `close` closes the real subflow.
* `MetaSocket::pump` now re-reads each transport's live `closed` level before scheduling, so a real
  TCP subflow that died is requeued onto the backup without an explicit notification.
* The feature is off by default; without `--features mptcp` the plain TCP crate and its tests are
  unchanged.

Host proof (`crates/raven-tcpip/tests/mptcp_bridge.rs`, enabled with `--features mptcp,testutil`)
runs two real `raven-tcpip` TCP connections over the simulator:

| Test | Assertion |
|---|---|
| handshake + DSS + reorder | MP_CAPABLE keys exchange, 3000 bytes over the primary, mappings delivered in reverse order to a peer MetaSocket, byte-exact |
| failover | primary real TCP closed mid-stream; backup idle until then; only unacked DSN 1000/2000 re-injected, byte-exact, no duplicate |
| live readiness | writable before/after a peer write; readable without a cached mask |

```sh
timeout 300 cargo nextest run -p raven-tcpip --features mptcp,testutil --test mptcp_bridge
```

The transport carries the bytes; the host test passes the DSS mapping metadata to the peer
out-of-band. Putting MP_CAPABLE/DSS on the real wire and the `IPPROTO_MPTCP` socket API is
MPTCP2b/2c.

## Socket API (MPTCP2b)

`MptcpSocket` is the host-testable Linux-shaped socket surface (`crates/raven-tcpip/src/
mptcp_socket.rs`, feature `mptcp`):

| Item | API |
|---|---|
| protocol | `IPPROTO_MPTCP = 262` over `AF_INET` / `AF_INET6` |
| open | `MptcpSocket::open(stack, family, protocol, remote, local_if, peer_capable)` |
| stream | `send`, `recv`, `readiness`, `close` (same non-blocking/TCP shape) |
| fallback | `peer_capable = false` opens one plain TCP connection; `is_fallback()` |
| path add | `add_path(remote, local_if, backup) -> subflow_id` |
| path remove | `remove_path(subflow_id)` (requeues its unacked ranges) |
| priority | `set_backup(subflow_id, backup)` (unbackup is `false`) |
| query | `subflows() -> Vec<SubflowInfo>`: id, local/remote address/port, backup, bytes in flight |
| policy | failover default; `set_aggregation(true)` opt-in distributes across regular paths |
| readiness | live `SubflowOps::readiness` on the active path; no cached mask |

The names mirror the Linux path-manager `ADD_ADDR` / `REMOVE_ADDR` / `SET_FLAGS` / `GET_ADDR`
operations and the `MPTCP_PM_ATTR_SUBFLOWS` shape. The libc/netlink binding and the real-wire
MP_CAPABLE negotiation are MPTCP2c; this slice is in-process API and lifecycle.

Host tests (`crates/raven-tcpip/tests/mptcp_socket.rs`) cover creation, two paths, send/recv,
backup promotion, path removal, plain-peer fallback, live readiness and opt-in aggregation.

```sh
timeout 300 cargo nextest run -p raven-tcpip --features mptcp,testutil --test mptcp_socket
```

Receive reassembly across subflows is still the MPTCP2a MetaSocket path (DSS metadata out of
band); the API `recv` reads the active real subflow until the wire DSS mapping lands in 2c.

## Phase 2 boundary

Phase 2 owns the `raven-tcpip` subflow integration: hooking MP_CAPABLE/MP_JOIN into the TCP
handshake, mapping subflow sequence space to DSS, and scheduling use of multiple paths. This crate
exposes plain data types and state machines for that work; it has no sockets, timers, kernel calls
or feature gate, so the 0.1.0 release path is unchanged.
