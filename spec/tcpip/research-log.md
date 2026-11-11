<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Research log (C01 spec-author session)

Status: provenance record, not a specification. It lists the donor material
that was opened, the question each source answered, and the limits of the
reading. No donor code, comment or identifier is reproduced here.

## 1. Role and rule

C01 is the only ticket in the C1 wave allowed to read donor stacks. This
session read donor source to learn **observable behaviour and operational
surface**, not architecture for our stack. DR-29 keeps the RFCs normative.
The implementers (C02-C27) must not open any of these sources; they read the
RFCs and `spec/tcpip/**` only.

Donor checkouts and revisions (from `ravenbsd-resources/donors.lock`):

| Donor | Revision | Licence tier | Used for |
|---|---|---|---|
| illumos-gate | `586678ac1bcbd77644a6ec4dac7f79d676cfb4ec` | B (CDDL-1.0) | operational counters/tunables, TIME-WAIT structure, timer surface, SACK receiver behaviour |
| NetBSD src | `36eddebcc36cb0775aa2962005b9c05419a6ff01` | A (BSD-2/3) | counter list, sysctl surface, state-machine edge cases, SACK/time-wait code shape |
| smoltcp | `8b0e5c8598647fe92e2a1b87932d1932354fee0b` | A (0BSD) | exact feature comparison for `features.md`, with file:line |
| FreeBSD src | `133344405f532bd54ff3629a77d4c4e062f01fdb` | A (BSD-2) | **not available in this checkout**; see §4 |

## 2. illumos-gate

Base path:
`ravenbsd-resources/illumos-gate/usr/src/uts/common/inet/tcp/`

| File | What was read / the question answered |
|---|---|
| `tcp_stats.c` | The kstat/MIB surface: connection counters, per-connection counters, how the SNMP names map to the operational counters. Used to shape `observability.md` §2/§6. |
| `tcp_tunables.c` | The property-table shape: each entry has a name, type, min, max, default and unit; the time-wait, queue, RTO, delayed-ACK, duplicate-ACK, cwnd, MSS, keepalive, FIN-WAIT-2 and port-range families. Used to choose the inventory and plausible ranges; all names in our spec are ours. |
| `tcp_time_wait.c` | Detached TIME-WAIT handling, bucket-based expiry and the idea of a separate bounded structure rather than keeping full connections in the main table. Used for `state-machine.md` §10 and C17's shape (not its code). |
| `tcp_sack.c` | Receiver-side block insertion/merge behaviour and the block-count limit. Used to sanity-check our receiver block description in `sack.md` §6; no algorithm was transcribed. |
| `tcp_timers.c` | Keepalive, FIN-WAIT-2 flush and zero-window probe timer surface. Used to enumerate the timer rows in `state-machine.md` §11. |
| `tcp_input.c` | Selected reset, sequence-check, SYN-SENT and SYN-RCVD paths, and the existence of a challenge-ACK/RST policy. Used to cross-check the RST rows; RFC 9293/5961 remain normative. |

No illumos header, table layout or comment was copied. The property table
shape is a generic list-of-typed-entries idea; our field names, defaults and
stability markers are independent choices.

## 3. NetBSD src

Base path: `ravenbsd-resources/netbsd/sys/netinet/`

| File | What was read / the question answered |
|---|---|
| `tcp_var.h` | The `tcpstat` counter list and the sysctl object list: connection attempts/accepts, drops, RTT updates, delayed ACKs, retransmits, PAWS drops, listen/SYN-cache counters, ECN counters, window scaling/SACK/timestamp toggles, buffer and MSS knobs. Used to make `observability.md` complete beyond the MIB minimum. |
| `tcp_input.c` | Selected state-machine and duplicate-ACK/SACK/DSACK paths; the duplicate-ACK threshold variable. Used to check that our row set covers the awkward cases (DSACK, reneging, partial ACK). |
| `tcp_output.c` | Selected SACK retransmission and burst-control paths; no algorithm was transcribed. Used to confirm that `Pipe`/`HighRxt`/rescue are real operational concepts, not theoretical. |
| `tcp_subr.c` | Global SACK limits, TIME-WAIT/VTW limits and default toggles. Used for the "bounded memory" requirements and the range-cap idea. |
| `tcp_timer.c` | Retransmit, persist and keepalive timer structure. Used to check our timer row list and backoff semantics. |
| `tcp_sack.c` | SACK hole/scoreboard pool and its bounds. Used to confirm that a bounded range set is the right model. |
| `tcp_syncache.c` | SYN-cache/compressed-state approach and its counters. Used for C16's half-open/cookie requirements (the ticket owns the design; this only informed the operational surface). |
| `tcp_vtw.c` | Vestigial TIME-WAIT: why a compact record is separated from full connections and how it ages. Used for `state-machine.md` §10 and C17. |

## 4. FreeBSD

The checkout at `ravenbsd-resources/freebsd` in this workspace is sparse and
contains `sys/netpfil` (ipfilter, ipfw, pf) but **no `sys/netinet` tree**:

```
$ find ravenbsd-resources/freebsd -maxdepth 4 -type d -name netinet
ravenbsd-resources/freebsd/sys/netpfil/ipfilter/netinet
$ find ravenbsd-resources/freebsd -name 'tcp_input.c'
(no output)
```

Therefore FreeBSD TCP source could not be read in this session. This is
recorded as a limitation, not hidden: the illumos and NetBSD reading covers
the same operational questions, and the ticket's donor list is a permission,
not a requirement to read every donor. If the reviewer wants a FreeBSD
cross-check, `make update` in `ravenbsd-resources` (owner action) will fetch
the missing subtree, and a later review pass can compare the same questions
without changing the spec's normative content.

## 5. smoltcp

Every comparison in `features.md` carries a `file:line` from this checkout.
The main files opened:

| File | What was checked |
|---|---|
| `README.md` | Its feature claims, used only as a starting point; several are coarser than the code and are corrected in `features.md` §1. |
| `src/socket/tcp.rs` | State enum and transitions, listen/connect/close/abort, MSS and window-scale negotiation, timestamp fields, SACK generation, RTO estimator, fast retransmit, delayed ACK, Nagle, keepalive, zero-window probe, TIME-WAIT timer, challenge ACK, absence of PAWS/scoreboard/PRR/RACK/TLP/SWS/backlog/ISN-hash. |
| `src/wire/tcp.rs` | Header/option parse and emit, checksums, ECN flag accessors, urgent-pointer parsing. |
| `src/socket/tcp/congestion.rs`, `congestion/reno.rs`, `congestion/cubic.rs` | The two controllers, their defaults, the Reno fast-recovery shape, the CUBIC use of `f64`. |
| `src/socket/tcp/congestion/no_control.rs` | The "no congestion control" option, for the decision table. |
| `src/iface/socket_set.rs`, `src/iface/interface/tcp.rs` | Linear socket dispatch (the scalability gap). |
| `src/iface/neighbor.rs` | ARP cache lifetime and request rate limit. |
| `src/iface/slaac.rs`, `src/iface/interface/ipv6.rs` | ND/SLAAC surface. |
| `src/wire/udp.rs` | The family-independent zero-checksum acceptance (the IPv6 gap). |
| `src/wire/ipv4.rs`, `src/iface/fragmentation.rs` | Fragment fields and reassembly feature gating. |
| `src/wire/ndisc.rs` | ND message parse surface. |
| `src/storage/assembler.rs` (referenced by README) | The bounded hole count claim; not traced in detail, marked UNVERIFIED where relevant. |

## 6. What was deliberately not done

* No donor code was copied into `spec/tcpip/**`.
* No donor file was translated line by line; donor reading was used to answer
  "does a mature stack do this, and what does an operator see?".
* No reference stack was run to produce a vector. The vectors are constructed
  from RFC field layouts and integer arithmetic; `headers.md`, `rto.md`,
  `cwnd.md` and `sack.md` show the work.
* No smoltcp limitation is stated without a `file:line`; the few claims that
  could not be traced are marked `UNVERIFIED` in `features.md` §12.
* No FreeBSD TCP claim is made, because the subtree was absent.

## 7. Research questions that shaped the spec

1. Which counters does an operator actually use? -> MIB-style set + the
   internal counters that explain a stuck connection (reassembly timeout,
   ACK for unsent, zero-window drop, RST for unsent, timer fires).
2. Which tunables need names and ranges? -> time-wait, two listen queues, RTO
   triplet, delayed ACK, duplicate-ACK threshold, cwnd cap, FIN-WAIT-2,
   keepalive, MSS triplet, buffer defaults/caps, port range, ISN strength,
   congestion selection; plus the challenge-ACK bucket and PMTU/PLPMTUD knobs
   that our own tickets require.
3. How does a mature stack keep TIME-WAIT from consuming the main table? ->
   a separate bounded structure with bucket/queue expiry. Our spec keeps the
   behaviour (separate table, O(1) expiry, recycle oldest) but not the data
   structure name or layout.
4. What does a SYN flood do to a naive listener? -> a full socket per
   half-open connection exhausts the pool; the fix is a small half-open
   record plus cookies. This is C16's requirement, informed by the operational
   question, not copied.
5. Which SACK edge cases matter? -> duplicate (DSACK), reneging, range cap,
   rescue retransmission, Pipe double-counting. The vectors pin all five
   `NextSeg` rules.
6. Which RTT estimator details are easy to get wrong? -> first-sample
   initialisation, integer rounding, the 1 s RFC floor vs a configured floor,
   backoff cap, Karn. The RTO vectors pin each.
7. Which CUBIC details are easy to get wrong? -> beta, fast convergence,
   `K` from `w_max`, concave vs convex, TCP-friendly `W_est`, idle exclusion,
   integer cube root. The CUBIC vectors pin each.

## 8. Reviewer verification

The reviewer can verify the clean-room claim by:

1. searching `spec/tcpip/**` for donor identifiers and distinctive comments
   (there should be none beyond generic protocol terms);
2. comparing our prose to the donor files listed above and confirming it is a
   functional description, not a paraphrase of a source file;
3. checking that every smoltcp row in `features.md` has a `file:line` that
   actually supports the claim;
4. checking that C02-C27 tickets say "read no third-party stack" and that
   their reports repeat it;
5. confirming there is no `[[import]]` entry in `PROVENANCE.toml` for
   `raven-tcpip` (see `provenance.md`).
