<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP SYN cache

Status: **OBSERVED** for the module behaviour (unit tests); **PROPOSED** for the stack wiring.
The cache is the Rust port of FreeBSD `sys/netinet/tcp_syncache.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (K276). It keeps the embryonic connection state that
a listening socket would otherwise allocate as a full socket.

## Why

A SYN flood must not create one `TcpConnection` per spoofed source. The cache stores only the
four-tuple, IRS/ISS, the negotiated options and the retransmit timer. The final ACK promotes the
entry into a real connection; an RST or the retransmit budget removes it.

## Limits and timers

| Setting | Default | Meaning |
|---|---:|---|
| `buckets` | 512 | Hash rows (`TCP_SYNCACHE_HASHSIZE`) |
| `bucket_limit` | 30 | Entries per row (`TCP_SYNCACHE_BUCKETLIMIT`); the oldest is evicted |
| `cache_limit` | 15360 | Global entries (`buckets * bucket_limit`); the oldest is evicted |
| `initial_rto_ms` | 1000 | First SYN/ACK retransmit |
| `rto_min_ms` / `rto_max_ms` | 200 / 60000 | Clamp for the donor backoff |
| `max_retransmits` | 3 | `SYNCACHE_MAXREXMTS`; the entry expires after the next due timeout |

The retransmit delay uses the FreeBSD `tcp_backoff[]` multipliers `1, 2, 4, 8, 16, ...`. With the
defaults the first four retransmits are due at 1 s, 3 s, 7 s and 15 s; the fifth timeout expires
the entry. This matches the donor's `sc_rxmits > rexmt_limit` check, not a "three strikes" rule.

## Outcomes

* `insert` returns `Inserted`, `Duplicate` (timestamp refreshed, retransmit count reset) or
  `Rejected` when the global limit cannot make room.
* `promote` accepts only `th_ack == ISS + 1` and `th_seq` in `(IRS, IRS + WND]`. A negotiated
  timestamp is mandatory and must not go backwards. It returns the entry and the ACK window for
  the new `TcpConnection`.
* `abort` removes the entry only for an in-window RST at `IRS + 1`; a reset elsewhere in the
  window requests a challenge ACK so a spoofed RST cannot abort the handshake.
* `poll(now)` returns `Retransmit(key)` for each due entry and `Expired(key)` after the budget.

## Divergences from FreeBSD

* The caller supplies `iss`; FreeBSD computes it in `syncache_add` (syncookie or random). Raven's
  existing RFC 6528/cookie generator in `listen.rs` stays the single source.
* The module does not send packets or allocate the TCB. `promote` and `poll` return the actions;
  the caller owns output and socket creation.
* The module is declared in `lib.rs` but the `tcp_state` listen path still uses its older
  half-open queue. Connecting the two requires `tcp_state.rs`/`stack.rs`, which are outside K276's
  Owns; the cache API is ready for that wiring.
