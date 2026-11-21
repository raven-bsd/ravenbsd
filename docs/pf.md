<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Packet filter (`raven-pf`)

Status: K273 port of the FreeBSD pf userspace core with the `netd` frame hook, 2026-11-21
(rework round 1). Evidence tags:
**OBSERVED** (read or measured), **PROPOSED** (not implemented yet), **UNVERIFIED** (not
exercised on a target).

## 1. Why

`netd` owns the `net:` socket server and the `lo`/`eth0` stacks (`architecture/07-network.md`
section 1). The filter must sit on the ingress/egress path before TCP/IP processing. K273
implements the complete host-testable pf core: rules, state, NAT/redirect, tables, counters and
logging, IPv4 and IPv6, and wires it into `netd` at the `raven-tcpip` device boundary (section
12). The `pf-core` QEMU case passes with one allowed stateful flow and one blocked, logged flow.

## 2. Scope

Implemented (**OBSERVED**, host tests in `crates/raven-pf/src/*_ut.rs` and
`crates/raven-pf/tests/`):

* pf.conf-compatible parser for `pass`/`block`, `quick`, `in`/`out`, `on`, `inet`/`inet6`,
  `proto any|tcp|udp|icmp|icmp6`, `from`/`to` with IPv4/IPv6 addresses, CIDR, ports and
  table references, `keep state`/`no state`, `log`, `set default pass|block`;
* `table <name> { ... }` declarations with bounded membership matching;
* stateful TCP/UDP/ICMP/ICMPv6 filtering with the pf `PFTM_*` timeouts, forward/reverse
  orientation, interface binding and the donor two-key (`sk`/`nk`) NAT-aware lookup;
* single-line `nat` and `rdr` rules with `-> address`, `-> { pool }`, `port`, `static-port`
  and the `none|bitmask|random|source-hash|round-robin` pool kinds;
* `PFRES_*` reason counters, per-rule counters, bounded packet log and the `pf.h` constants /
  status block a pfctl client needs;
* fail-closed behaviour: a malformed rule, an unknown table, a failed translation or a full
  state table drops the packet and is counted/logged; wire input never panics;
* the `netd` ingress/egress hook: `/etc/pf.conf` loaded at start (malformed config refuses to
  start), one `Engine` per interface, `Block` drops the frame, `Decision.translation` rewrites
  addresses/ports and fixes the IPv4/TCP/UDP checksums, and the bounded log is drained to the
  serial log.

Deferred (documented limits):

* ALTQ/QoS, pfsync, authpf, OS fingerprinting, anchors, scrub/normalisation, tags, UID/GID
  rules, `synproxy`, source-node limits and `pfctl` ioctl transport (K274);
* address ranges (`PF_ADDR_RANGE`), `no-route`/`urpf-failed`/dynamic interface address kinds;
* cross-family `af-to` translation, ICMP/ICMPv6 identifier NAT, fragment reassembly and the
  fragment cache (a fragment creates no state and never matches a rule with a
  source/destination port condition; address-only rules still match);
* fragment reassembly/cache: a fragment creates no state and never matches a rule with a
  source/destination port condition (address-only rules still match).

## 3. Donor map

Pinned revision: `133344405f532bd54ff3629a77d4c4e062f01fdb` (`ravenbsd-resources/donors.lock`,
`freebsd`). Every file below is **BSD-2-Clause** (SPDX header verified in the file); literal
transpilation is permitted.

| Donor file | Functions used | Raven module |
|---|---|---|
| `sys/netpfil/pf/pf.h` | `PF_PASS..PF_RT`, `PF_OP_*`, `PF_POOL_*`, `PF_LOG_*`, `PF_ADDR_*`, `PFTM_*` + `*_VAL`, `PFRES_*`/`PFRES_NAMES`, `struct pf_addr`, `struct pf_status` | `src/abi.rs` |
| `sys/netpfil/pf/pf.c` | `pf_test` 11646, `pf_match_rule` 5959, `pf_test_rule` 6339, `pf_match_addr` 4916, `pf_match`/`pf_match_port` 4976, `pf_state_key_setup` 2059, `pf_find_state` 2233, `pf_create_state` 6631, `pf_state_expires` 2964, `pf_counters_inc` 11489, `pf_log_matches` 11628 | `src/ast.rs`, `src/engine.rs`, `src/state.rs`, `src/log.rs` |
| `sys/netpfil/pf/pf_lb.c` | `pf_match_translation` 282, `pf_get_sport` 296, `pf_map_addr` 623, `pf_get_translation` 1057, `pf_get_transaddr` 1096, `pf_get_transaddr_af` 1362 | `src/nat.rs` |
| `sys/netpfil/pf/pf_table.c` | `pfr_match_addr`, table entry bounds | `src/table.rs` |
| `sys/net/pfvar.h` | kernel-side state/key/rule structs, `PF_MISMATCHAW` (cross-check only) | — |

Deviation **D-pf-layout**: the pinned snapshot is a curated tree. Upstream `pf_state.c`,
`pf_nat.c`, `pf_rule.c`, `pf_addr.c`, `pf_log.c`, `pf_thread.c`, `pf_op.c`, `pf_var.h` and the
whole `usr.sbin/pfctl/**` tree are absent (`git cat-file -e` verified). Their logic is merged
into `pf.c` under OpenBSD-style lowercase `pf_*` names and into `pf_lb.c`. The table above maps
the actual locations. This is a donor-layout divergence only; no behaviour was guessed.

## 4. Syntax

```
# comment
set default block|pass
table <name> { <addr> [, <addr> ...] }

pass|block [quick] [in|out] [on <iface>] [inet|inet6] [proto <proto>]
    [from <addr> [port <spec>]] [to <addr> [port <spec>]]
    [keep state|no state] [log]

nat [on <iface>] [proto <proto>] from <addr> [port <spec>] to <addr> [port <spec>]
    -> <addr>|<{ <addr>, ... }> [port <port>|<low:high>]
    [none|bitmask|random|source-hash|round-robin] [static-port] [log]
rdr  (same shape; translates the destination)
```

* `<addr>`: `any`, `a.b.c.d`, `a.b.c.d/prefix` (`0..=32`), an IPv6 literal (`::`, `::1`,
  `2001:db8::/32`, `0..=128`) or `<table>`.
* `<spec>`: `any`, `N`, `A:B` (inclusive; `:B` means `0..B`, `A:` means A..65535), `<N`, `>N`,
  `!=N`.
* `<iface>`: `any` or `[A-Za-z0-9._-]{1,32}`. `<name>`: the same alphabet, 1..=32 characters.
* `nat` defaults to `out`, `rdr` to `in`. Tables may be declared after their first use.
* Keywords are case-insensitive; comments start at `#`; blank lines are ignored. Braces and
  commas are whitespace.
* A `pass` rule keeps state by default; `no state` disables it. A `block` rule never keeps
  state. A fragmented packet never matches a rule that names a source or destination
  port (donor `pf_match_rule()` `PF_VPROTO_FRAGMENT`), can still match an address-only
  rule, and creates no state. See the fragment rule below.

### Fragments and port rules

A `PF_VPROTO_FRAGMENT` rule match fails if either side names a port
(`PF_TEST_ATTRIB((r->src.port_op || r->dst.port_op), fail)` in FreeBSD
`sys/netpfil/pf/pf.c:5997-6002`). `raven-pf` implements the same rule in
`Rule::matches`: `pass in proto tcp from any to any port 80` cannot admit a
fragment; `pass in proto tcp from any to any` can. A fragment still creates no
state (`D-pf-frag-state`), and the fragment reassembly cache remains deferred.
* Limits: 4096 rules, 1024 NAT rules, 64 tables (1024 entries each), 16 pool addresses,
  512-byte lines, 32-byte interface/table names. A parse error names the 1-based line.

The fixtures are `tests/fixtures/pf/core.conf`, `core.tsv` and `invalid.conf`.

## 5. Matching and translation order

Donor order (`pf_test_rule()`): translation first, then the filter rules.

1. An existing connection state is checked **before** the rules, against the packet as it
   arrives. A translated state is found through its second key (`nk`), so reply traffic in the
   wire form matches too.
2. The first matching NAT/RDR rule is applied to a copy of the packet
   (`pf_get_translation()` + `pf_translate_compat()`); the filter rules see the translated view.
3. Every rule is tested in file order. The **last** matching rule wins; a matching `quick` rule
   stops the walk immediately.
4. If no rule matches, `set default` applies (`block` when absent).
5. On a passing rule with `keep state`, the state is created from the original tuple plus the
   translated tuple.

A rule matches when direction (if present), interface, address family, protocol, source
address/port and destination address/port all match. Port conditions apply only to TCP and UDP;
a port condition on ICMP never matches. Fragments skip the port/flag conditions (donor
`pf_match_rule()`).

## 6. State table

The state stores both sides of a translated connection: `key` is the tuple as evaluated (donor
`sk`) and `peer` is the wire tuple after translation (donor `nk`). A packet whose endpoints are
swapped and whose direction is opposite is the reverse orientation. A reply on a translated
state returns the reverse rewrite in `Decision.translation`.

Timeouts are the donor `PFTM_*_VAL` defaults (`sys/netpfil/pf/pf.h`; the table is asserted by
`abi_ut.rs`):

| state | seconds |
|---|---:|
| TCP first packet | 120 |
| TCP opening (SYN+ACK, no ACK) | 30 |
| TCP established | 86400 |
| TCP closing (half closed) | 900 |
| TCP both FINs | 45 |
| TCP RST | 90 |
| UDP first / single / multiple | 60 / 30 / 60 |
| ICMP/ICMPv6 first / error reply | 20 / 10 |
| other first / single / multiple | 60 / 30 / 60 |
| fragment | 60 |

TCP transitions: SYN → first; SYN+ACK → opening; ACK → established; FIN → fin-wait, both FINs →
closing; RST → closed. UDP becomes `multiple` after a reverse packet and `single` after a second
forward-only packet.

**Fail-closed:** the table is bounded (4096 by default, configurable to `1..=4096`). When full,
`pf_create_state()` semantics apply — the packet is dropped with `PFRES_MAXSTATES` and logged;
a live connection is never evicted silently (deviation from B17, which evicted the earliest
expiry). Expiry uses the caller-supplied monotonic clock, so host tests are deterministic.

## 7. NAT and redirect

* Address selection follows `PF_POOL_*`: `none`/`bitmask` use the first pool address, `random`
  uses a deterministic xorshift of the packet tuple, `source-hash` an FNV-1a of the source
  address, `round-robin` a dedicated cursor.
* NAT source-port allocation reproduces the donor `pf_get_sport()` search over
  `[proxy_port.low, proxy_port.high]`, skipping ports owned by live states (the
  `pf_find_state_all_exists()` check). The default range is the donor
  `PF_NAT_PROXY_PORT_LOW..HIGH` = 50001..65535; `static-port` preserves the original port.
* Redirect destination-port allocation reproduces `pf_get_transaddr()`:

  ```
  div       = proxy_port[1] - proxy_port[0] + 1     (0 -> 1)
  tmp_nport = ((ndport - rule.dst.port[0]) % div) + proxy_port[0]
  ```

  A fixed `port N` maps to `N`; no pool `port` preserves the destination port.
* A matched translation that cannot allocate an address/port fails closed with
  `PFRES_MAPFAILED` (donor `PFRES_MAPFAILED` on `pf_get_sport()` failure).

Deviations: **D-pf-pool-bitmask** (no pool-mask option in 0.1.0, so a multi-address bitmask pool
uses the first address), **D-pf-icmp-nat** (ICMP/ICMPv6 identifiers are not rewritten),
**D-pf-af-to** (cross-family translation is refused as `NoAddress`).

## 8. Counters, logs and ABI

`CounterSnapshot` carries the directional pass/block counters, state counters and the
`PFRES_*` reason array (`PFRES_MAX = 17`). Explicit blocks count `PFRES_MATCH`; fail-closed
state-limit and translation failures count `PFRES_MAXSTATES` and `PFRES_MAPFAILED`.

`LogEntry` is a bounded ring (`MAX_LOG_ENTRIES = 256`, oldest dropped). A matched rule with
`log` records the packet; fail-closed drops always record (donor `PF_LOG_FORCE`). `netd` drains
the ring with `Engine::drain_logs()`.

`src/abi.rs` exposes the pf.h action/operation/pool/log/address/timeout/reason constants, the
`PfAddr` 16-byte address representation and a `PfStatus` snapshot (`Engine::status()`).
Deviation **D-pf-abi-ioctl**: `PFOP_*` ioctl numbers are not in the pinned tree (upstream
`pfctl` is absent), so the ioctl transport is deferred to K274.

## 9. Tables

`table <name> { ... }` creates a bounded `Table`. A rule references it by index
(`AddrSpec::Table`); matching is `pfr_match_addr` membership. Tables cannot contain tables
(recursion is refused at parse time and guarded at match time). Tables are read-only after
parse in 0.1.0; `Table::replace()` is the bounded mutation seam for K274 (`pfctl -t`).

## 10. API seam

`Engine::evaluate(&Packet, now)` returns
`Decision { verdict, rule, state, tcp, translation, reason }`.
`Engine::counters()`, `Engine::rule_stats(index)`, `Engine::states()`, `Engine::logs()`,
`Engine::drain_logs()` and `Engine::status()` return plain snapshots; the engine uses relaxed
atomics. No allocation happens on the input path beyond state/log-table operations and no
`unsafe` is used (`#![forbid(unsafe_code)]`).

## 11. Fail-closed matrix

| Condition | Result |
|---|---|
| malformed rule / unknown table / bound exceeded | parser error with line number; the ruleset is not installed |
| no rule matches | `set default` (block when absent) |
| state table full | `Block`, `PFRES_MAXSTATES`, forced log |
| translation cannot allocate | `Block`, `PFRES_MAPFAILED`, forced log |
| fragmented packet | never matches a rule with a source/destination port condition; address-only rules can match; no state |
| wire/random parser input | no panic (4000-case fuzz test) |
| `/etc/pf.conf` present but malformed | `netd` logs the line and exits 1 before registering `net:` |
| hook cannot express a translation (fragment/ICMPv6) | frame dropped, `translate-unsupported` logged |

## 12. netd hook (wired)

`userland/servers/netd/src/filter.rs` wraps the `raven-tcpip` [`Device`]:

* `FilterDevice::receive` parses each ingress Ethernet/IP frame, calls
  `Engine::evaluate(.., Direction::In, ..)` and either forwards it, drops it
  (`Verdict::Block`) or forwards a rewritten copy (`Decision.translation`);
* `FilterDevice::transmit` fills the egress frame, filters it the same way and drops or rewrites
  before the inner device queues it;
* one `Engine` (and one `Ruleset` clone) exists per interface: `lo` is wrapped by
  `Netd::with_filter`, `eth0` by `Netd::attach_nic_with_filter`; both share one log handle;
* `netd` loads `file:/etc/pf.conf` then `initfs:/etc/pf.conf` at start. Absent means no filter;
  malformed means `netd` prints `netd: pf config: <line>: <error>; refusing to start` and exits
  before registering `net:`;
* `netd` drains `Engine` log records after every poll and prints
  `netd: pf <pass|block> <in|out> <iface> <proto> <src>:<sport> -> <dst>:<dport> [reason=..]`;
* NAT/RDR rewrites the IPv4/IPv6 addresses and TCP/UDP ports and recomputes the IPv4 header and
  L4 checksums; packets whose checksum cannot be fixed safely (fragments, ICMPv6) are dropped
  fail-closed.

The profile lives in `profiles/pf.toml` + `profiles/pf.d/` (initfsd, pcid, devd, virtio-net,
netd, the `pf-probe` manifest and `/etc/pf.conf`). `tests/qtest/cases/pf-core.toml` runs it
under QEMU user networking with the host UDP echo on `10.0.2.2:47001`; the probe sends one
allowed flow (echoed, stateful reply admitted) and one blocked flow (logged on the serial
console). The case asserts the netd boot lines, both flows, the drop record and `init exited
(0)`; it forbids panics.

## 13. Provenance

* `src/abi.rs`: constants/struct shapes transcribed from FreeBSD `sys/netpfil/pf/pf.h`
  (BSD-2-Clause, revision above); recorded in `PROVENANCE.toml`, tier A, modified
  (namespaced, Rust types, no kernel-only fields).
* `src/nat.rs`: the `pf_get_sport()` search and `pf_get_transaddr()` redirect-port formula are
  transcribed from `sys/netpfil/pf/pf_lb.c` (BSD-2-Clause); recorded in `PROVENANCE.toml`,
  tier A, modified (Rust, bounded vectors, deterministic cursors, no mbuf/pool plumbing).
* `userland/servers/netd/src/filter.rs` is original Raven plumbing at the `Device` boundary; no
  donor file describes this userspace hook, so no new provenance entry applies (the pf
  algorithms it calls are covered above).
* `src/ast.rs`, `src/address.rs`, `src/table.rs`, `src/state.rs`, `src/engine.rs`,
  `src/parser.rs`, `src/log.rs`: semantic ports of the donor functions named in section 3,
  written in Rust without kernel structs, mbufs, locks or routing hooks; no literal donor text
  beyond the algorithms cited above.
