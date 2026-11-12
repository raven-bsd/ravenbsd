<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# raven-tcpip: clean-room behaviour specification

Status: C01 spec wave, rev 1. Licence of this document: BSD-3-Clause.
This tree is prose, tables and test vectors only. It contains no executable code.

C32 update: smoltcp is retired. `raven-tcpip` is the only TCP/IP backend and the default of
`raven-netcore`; references to smoltcp in this tree (`features.md`, `research-log.md`,
`provenance.md`) are the historical C01 comparison and stay as written. The DHCPv4 client is
implemented (`crates/raven-tcpip/src/dhcp.rs`, adapted from smoltcp's 0BSD client).

## 1. What this specification is

`raven-tcpip` is a standalone, OS-independent TCP/IP stack: `no_std` + `alloc`,
`#![forbid(unsafe_code)]`, no threads, no wall clock, no driver. It sits behind
contract C15 in `tickets/CONTRACTS.md`:

```
application / libc
      |
  raven-netcore  (socket API, unchanged)
      |
  StackBackend trait (C25/C26)
      |
  raven-tcpip  <-- this specification
      |
  Device trait (frames in, frames out)
```

The stack is written clean-room. The **RFCs are normative**. This specification
turns the RFC requirements, plus explicitly marked local decisions, into a set
of rules and vectors that an implementer can follow without reading any other
TCP/IP implementation. Donor stacks were read only to learn which behaviours
and operational surfaces matter; no donor code, comment, identifier-as-written
or distinctive table layout appears here.

## 2. Authority and how to read a rule

The authority order is:

1. The RFC named for the behaviour (normative, unless a row is marked
   `DEVIATION`).
2. This specification (normative for `raven-tcpip`; it may narrow an RFC
   `SHOULD`/`MAY`, and it may mark a local choice where the RFC is silent).
3. `spec/tcpip/vectors/**` (executable arithmetic; normative for the values it
   covers).
4. Donor observation (informative only; never an architecture source, DR-29).

Every decision in this tree carries exactly one marker:

| Marker | Meaning |
|---|---|
| `REQUIRED` | The stack MUST implement it. A missing row is a defect. |
| `OPTIONAL` | Implemented only if the named ticket includes it; absence is not a defect. |
| `OUT-OF-SCOPE` | Not implemented by `raven-tcpip`; the reason is stated. |
| `UNSPECIFIED` | The RFC leaves the behaviour open; the following text states our choice and why. |
| `DEVIATION` | We knowingly differ from an RFC or from a widely deployed convention. The reason and the observable effect are stated. |
| `UNVERIFIED` | A comparison claim that could not be checked in the cited source. It must never become a requirement. |

`MUST`, `MUST NOT`, `SHOULD`, `SHOULD NOT` and `MAY` are used as in RFC 2119.
Where this document says "the stack" it means `raven-tcpip`; where it says
"the caller" it means `raven-netcore` or a test harness.

## 3. The clean-room role split

This split is the whole point of C01 and is not negotiable:

* **C01 (this ticket, author role).** The author may read illumos, FreeBSD,
  NetBSD and smoltcp source, man pages, headers and tests. The author writes
  prose, tables and vectors only. Nothing is copied into the tree; donor
  identifiers are not reproduced as identifiers, and donor table layouts are
  not reproduced. The author records what was read in `research-log.md`.
* **C02-C27 (implementer role).** Implementers read the RFCs named in their
  ticket and `spec/tcpip/**`, and **nothing else**. They must not open any
  third-party TCP/IP implementation, including smoltcp. If this specification
  is ambiguous or contradicts an RFC, the implementer writes a `QUESTION`
  report; they do not resolve it by reading donor code.
* **C24 (oracle role).** The comparison in C24 is made by **measuring running
  systems** (packet captures, socket observability) and never by reading a
  reference stack's source. This keeps the implementation session clean.

The result is BSD-3-Clause code. It is not a port, translation or paraphrase
of CDDL or any other donor.

## 4. Files

| File | Contents | Status |
|---|---|---|
| `README.md` | This file: scope, authority, role split, index. | SPEC |
| `state-machine.md` | TCP state x event table, RST policy, challenge ACK, TIME-WAIT, simultaneous open/close, timers. | SPEC |
| `features.md` | One row per feature: RFC, decision, owning ticket, verified smoltcp status with `file:line`. | SPEC |
| `observability.md` | Counter and tunable inventory with names, units, triggers, ranges, stability. | SPEC |
| `vectors/headers.toml` | Header parse/serialize vectors and malformed-input rejection reasons. | VECTORS |
| `vectors/headers.md` | Worked field and checksum arithmetic for the header vectors. | VECTORS |
| `vectors/rto.toml` | RTT sample sequences -> SRTT, RTTVAR, RTO, backoff. | VECTORS |
| `vectors/rto.md` | Fixed-point arithmetic and worked sequences. | VECTORS |
| `vectors/cwnd.toml` | ACK/loss sequences -> cwnd and ssthresh for NewReno and CUBIC. | VECTORS |
| `vectors/cwnd.md` | The exact congestion-control arithmetic and worked steps. | VECTORS |
| `vectors/sack.toml` | SACK scoreboard cases -> loss marks, Pipe, NextSeg. | VECTORS |
| `vectors/sack.md` | RFC 6675 definitions and worked cases. | VECTORS |
| `oracle.md` | C24 measurement plan, capture method, table to fill, and what was observed on the authoring host. | PLAN |
| `research-log.md` | Donor files opened and the questions they answered. No code excerpts. | PROVENANCE |
| `provenance.md` | Licence role, clean-room statement, verification steps for the reviewer. | PROVENANCE |

## 5. Conventions used throughout

* **Byte order.** Wire fields are big-endian. Host values are unsigned.
  `u32` sequence numbers wrap modulo 2^32; comparisons use the serial-number
  rule in `state-machine.md` section 2, never ordinary integer comparison.
* **Time.** All stack time is `i64` milliseconds (`Instant`) and `u64`
  milliseconds (`Duration`), per contract C15. Internally, arithmetic that
  needs sub-millisecond precision uses a stated fixed-point scale; the RTO and
  CUBIC vectors state theirs. No floating point is used anywhere in the stack.
* **Sizes.** Windows, queues and counters are byte counts unless stated.
  MSS values are bytes. Segment counts are converted at the API edge using the
  connection's effective MSS.
* **Counters.** `u64`, saturating? No: counters wrap modulo 2^64. A counter
  increment is one plain add; it never allocates, locks or returns an error.
* **Names.** Public counters and tunables use the `tcp.` namespace followed by
  `snake_case` (`tcp.active_opens`, `tcp.rto_min`). There is no leading
  underscore and no `tcp_tcp_` stutter. Rust identifiers in C19 are the same
  names with dots replaced by underscores.
* **Option order.** Our emission order is fixed by `state-machine.md` section
  9 and is part of the observable surface measured by C24. It is not inherited
  from any donor.
* **Vectors.** Each TOML file is self-describing; the sibling `.md` shows the
  arithmetic. A vector value is normative: an implementation that disagrees
  with a vector is wrong until the vector is corrected by a reviewed spec
  change.

## 6. Ticket map

The implementation is split across C02-C27. This table is the contract between
the specification and the tickets; a ticket that needs a behaviour not listed
here must ask for a spec change rather than invent it.

| Ticket | Area | Spec sections it consumes |
|---|---|---|
| C02 | Wire headers, options, checksums | `vectors/headers.*`, `features.md` wire rows |
| C03 | Stack skeleton, device, time, buffers, poll | `README.md` §5, C15, `observability.md` queue counters |
| C04 | Host simulator and trace recorder | `oracle.md` capture format, `state-machine.md` event names |
| C05 | ARP, IPv4, routing, fragmentation, reassembly | `features.md` L2/L3 rows, `vectors/headers.*` |
| C06 | ICMPv4 and UDP | `features.md` ICMP/UDP rows, `vectors/headers.*` |
| C07 | IPv6 ND, SLAAC, ICMPv6, UDPv6 | `features.md` IPv6 rows, `vectors/headers.*` |
| C08 | TCP state machine | `state-machine.md` in full |
| C09 | Data transfer, queues, SWS, Nagle, delayed ACK | `state-machine.md` §7, `features.md` data rows |
| C10 | RTO, RTT, timer set | `vectors/rto.*`, `features.md` RTO rows |
| C11 | Window scale, timestamps, PAWS, MSS | `features.md` option rows, `vectors/headers.toml` option cases |
| C12 | SACK scoreboard, recovery, DSACK | `vectors/sack.*`, `features.md` SACK rows |
| C13 | Congestion control: NewReno, CUBIC, PRR | `vectors/cwnd.*`, `features.md` congestion rows |
| C14 | RACK-TLP, spurious retransmission, ECN | `features.md` recovery/ECN rows, `state-machine.md` §8 |
| C15 | PMTUD and PLPMTUD | `features.md` PMTU rows, `state-machine.md` ICMP input rows |
| C16 | Listen path, SYN cookies, ISN | `state-machine.md` LISTEN/SYN-RECEIVED, `features.md` listen rows |
| C17 | Connection table, ports, TIME-WAIT at scale | `state-machine.md` §6, `observability.md` table counters |
| C18 | TCP MD5 and TCP-AO | `features.md` signature rows (OPTIONAL) |
| C19 | Observability and property table | `observability.md` in full |
| C20 | packetdrill conformance | `state-machine.md` row IDs, `vectors/**` |
| C21 | Fuzzing | `vectors/headers.toml` rejection reasons |
| C22 | Interop over TAP | `oracle.md` capture format, `features.md` REQUIRED rows |
| C23 | Performance baseline | `features.md` performance-relevant rows |
| C24 | Oracle comparison and deviation register | `oracle.md` in full |
| C25 | `StackBackend` trait | `README.md` §1, C15 |
| C26 | `RavenTcpipBackend` | C15 socket semantics, `features.md` |
| C27 | Default switch | C24 results, `features.md` deviation register |

## 7. What is deliberately not here

* No Rust, no code snippets that could be compiled, no donor source.
* No performance numbers presented as requirements; performance is C23's
  measurement, not a specification input.
* No implementation recipe for a specific data structure (for example, a
  specific hash table layout). Structures are constrained only where the
  contract requires it (bounded memory, O(1) classification, no peer-sized
  allocation).
* No claim about a third-party stack that is not backed by a `file:line` mark
  in `features.md`; unverified claims are marked `UNVERIFIED` and cannot become
  requirements.

## 8. How to change this specification

A change is a reviewed commit to `spec/tcpip/**` that names the affected row,
states the RFC basis (or `UNSPECIFIED`), updates the vectors if arithmetic is
affected, and records the decision in `provenance.md` if a clean-room boundary
is touched. C24 may append oracle results and accepted deviations to
`features.md`; it may not silently change a requirement.
