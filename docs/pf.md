<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Packet filter (`raven-pf`)

Status: B17 implementation, 2026-11-15. Evidence tags: **OBSERVED** (read or measured),
**PROPOSED** (not implemented yet), **UNVERIFIED** (not exercised on a target).

## 1. Why

`netd` owns the `net:` socket server and the `lo`/`eth0` stacks (`architecture/07-network.md`
section 1). M5 needs a firewall hook before the packet path can be hardened; B17 adds the
host-testable filter core now and keeps the `netd` hook behind a pure API seam. The QEMU
block/allow demo is **PROPOSED** for the follow-up ticket because `userland/servers/netd/**` is not
in B17's Owns.

## 2. Scope

Implemented:

* pf-like ruleset parser for `pass`/`block`, `quick`, `in`/`out`, `on`, `proto`, `from`/`to`
  with addresses, CIDR and ports, `keep state`/`no state`, `log`, and `set default pass|block`;
* stateless evaluation with pf's last-match rule and `quick` early exit;
* a bounded stateful connection table with pf timeouts, forward/reverse orientation and
  interface binding;
* per-rule evaluation/match/packet/byte counters and global pass/block/state counters;
* deterministic host tests and fixture rulesets/packets.

Not implemented (documented limits):

* IPv4 only (netd is IPv4-only in M5); no IPv6 addresses;
* no NAT/rdr, scrub/normalisation, anchors, tables, ALTQ/QoS, OS fingerprints, UID/GID rules,
  tags or `synproxy`;
* no packet logging side effect: `log` is a rule flag for `netd` to act on;
* no `netd` hook in this ticket (see section 7).

## 3. Syntax

```
# comment
set default block|pass
pass|block [quick] [in|out] [on <iface>] [proto any|tcp|udp|icmp]
    [from <addr> [port <spec>]] [to <addr> [port <spec>]]
    [keep state|no state] [log]
```

* `<addr>`: `any`, `a.b.c.d` or `a.b.c.d/prefix` (`0..=32`). Networks are normalised.
* `<spec>`: `any`, `N`, `A:B` (inclusive; `:B` means 0..B, `A:` means A..65535), `<N`, `>N`,
  `!=N`.
* `<iface>`: `any` or `[A-Za-z0-9._-]{1,32}`. No interface groups or wildcards.
* Keywords are case-insensitive; comments start at `#`; blank lines are ignored.
* A `pass` rule keeps state by default; `no state` disables it. A `block` rule never keeps
  state.
* Limits: 4096 rules, 512-byte lines, 32-byte interface names. A parse error names the 1-based
  line.

Examples (also in `tests/fixtures/b17-pf/`):

```
set default block
pass quick in on lo all
block in on eth0 proto udp from any to any
pass in on eth0 proto tcp from 10.0.0.0/8 to 192.0.2.0/24 port 443 keep state
pass out on eth0 proto any from any to any no state
```

## 4. Matching

1. An existing connection state is checked **before** the rules; a stateful pass therefore
   admits the reply even if a later block rule would match it.
2. Without a state, every rule is tested in file order. The **last** matching rule wins; a
   matching `quick` rule stops the walk immediately.
3. If no rule matches, `set default` applies (`block` when absent).

A rule matches when direction (if present), interface, protocol, source address/port and
destination address/port all match. Port conditions are applied only to TCP and UDP; a port
condition on an ICMP packet never matches. CIDR matching compares the masked address.

## 5. State table

The key is protocol, source/destination address, source/destination port, direction and an
optional interface binding (`on` names an interface). A packet whose endpoints are swapped and
whose direction is opposite is the reverse orientation of the same state.

Timeouts are the pf reference values (**OBSERVED** in OpenBSD `sys/net/pfvar.h`):

| state | seconds |
|---|---:|
| TCP first packet | 120 |
| TCP opening (SYN+ACK, no ACK) | 30 |
| TCP established | 86400 |
| TCP closing (half closed) | 900 |
| TCP both FINs | 45 |
| TCP RST | 90 |
| UDP first / single / multiple | 60 / 30 / 60 |
| ICMP first / error reply | 20 / 10 |
| other first / single / multiple | 60 / 30 / 60 |

TCP transitions: SYN → first; SYN+ACK → opening; ACK → established; FIN → fin-wait, both FINs →
closing; RST → closed. UDP becomes `multiple` after a reverse packet and `single` after a second
forward-only packet. The table is bounded (4096 by default, configurable); when full, the entry
with the earliest expiry is evicted. Expiry is computed from the caller-supplied monotonic clock,
so host tests are deterministic.

## 6. Counters and API seam

`Engine::evaluate(&Packet, now)` returns `Decision { verdict, rule, state, tcp }`.
`Engine::counters()` and `Engine::rule_stats(index)` return plain snapshots; the engine uses
relaxed atomics so a future `netd` loop can read them without locking.

`netd` will construct `Packet` from its stack event (direction, interface, protocol, endpoints,
ports, length, TCP flags) and drop the packet before forwarding on `Verdict::Block`. The engine
does not allocate on the input path beyond the state table operations; no `unsafe` is used.

## 7. netd hook follow-up

The QEMU block/allow demo and the `netd` integration are **PROPOSED** follow-up **B17a**
("netd packet-filter hook and QEMU block/allow demo"). It must add the hook in
`userland/servers/netd/**`, load `/etc/pf.conf` at start, and run a netd probe that shows one
blocked and one allowed flow. This ticket deliberately does not edit netd.

## 8. Provenance

This is a clean-room implementation; no donor code was copied, translated or closely paraphrased.
The following files were read for behaviour only (**OBSERVED**):

* OpenBSD `sys/net/pf.c`, `sys/net/pf_lb.c`, `sys/net/pf_ruleset.c`, `sys/net/pf_if.c`,
  `sys/net/pf_ioctl.c`, `sys/net/pf_norm.c`, `sys/net/pf_table.c`, `sys/net/pfvar.h`,
  `sys/net/pfvar_priv.h` (rule order/quick, state keys, timeouts, rule flags).
* NetBSD `usr.sbin/npf/npfctl/npf.conf.5`, `sys/net/npf/npf.c`, `sys/net/npf/npf_ruleset.c`,
  `sys/net/npf/npf_handler.c`, `sys/net/npf/npf_conn.c`, `sys/net/npf/npf_connkey.c`,
  `sys/net/npf/npf_conndb.c`, `sys/net/npf/npf_nat.c`, `sys/net/npf/npf_conf.c`,
  `sys/net/npf/npf.h` (rule inspection/final, stateful pass, connection keys).
* FreeBSD `sys/netpfil/pf/pf.c`, `sys/netpfil/pf/pf.h`, `sys/netpfil/pf/pf_if.c`,
  `sys/netpfil/pf/pf_ioctl.c`, `sys/netpfil/pf/pf_lb.c`, `sys/netpfil/pf/pf_norm.c`,
  `sys/netpfil/pf/pf_table.c` (rule matching, state keys, timeouts).

No `PROVENANCE.toml` import applies (clean-room). The upstream revisions are the pinned
`openbsd`, `netbsd` and `freebsd` entries in `ravenbsd-resources/donors.lock`.
