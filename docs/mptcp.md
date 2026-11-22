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

## Phase 2 boundary

Phase 2 owns the `raven-tcpip` subflow integration: hooking MP_CAPABLE/MP_JOIN into the TCP
handshake, mapping subflow sequence space to DSS, and scheduling use of multiple paths. This crate
exposes plain data types and state machines for that work; it has no sockets, timers, kernel calls
or feature gate, so the 0.1.0 release path is unchanged.
