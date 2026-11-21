<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP output stress and data-integrity matrix (K301)

Host-side evidence for the K265 FreeBSD TCP output/persist port. The crate
`tests/tcp-output-stress` drives the merged `raven-tcpip` stack over the deterministic
`raven-tcpip-sim` link and hashes every delivered stream. It makes no product change.

## Matrix

| Test | Stream / condition | Integrity check |
|---|---|---|
| `one_mib_both_ways_matches_hash` | 1 MiB A->B and 1 MiB B->A | FNV-1a of the delivered stream equals the sent payload |
| `sixteen_mib_both_ways_matches_hash` | 16 MiB A->B and 16 MiB B->A | same |
| `small_writes_both_ways_match_hash` | 2048 separate 1..8-byte writes each way | same |
| `zero_window_probe_reopen_matches_hash` | 2048-byte receiver buffer, 8 KiB write, persist probe, reopen | same, plus probe count |
| `retransmit_loss_matches_hash` | 1 MiB over a 5% loss link | same, plus retransmission count |
| `sack_loss_matches_hash` | 1 MiB over a 2% loss link with SACK negotiated | same, plus retransmission count |
| `persist_cycles_match_hash` | two fresh connections, closed peer window, probe/reopen | same, plus probe count per cycle |
| `a_one_byte_send_offset_corruption_is_detected` | one byte flipped at send offset 1234 | the hash of the delivered corrupted stream differs from the original |

The expected behaviour is FreeBSD `sys/netinet/tcp_output.c` (`tcp_default_output`,
`tcp_setpersist`) and `sys/netinet/tcp_timer.c` (`tcp_timer_persist`) at
`133344405f532bd54ff3629a77d4c4e062f01fdb`, as ported by K265. No donor code is copied.

## Run

```sh
timeout 180 cargo nextest run -p tcp-output-stress
timeout 420 make check-fast
```

The matrix uses a 10 ms simulated RTT and virtual time only; the 16 MiB both-ways case completes in
about two seconds of host time. Every transfer has a hard 120 s virtual-time stall guard.

## Negative checks

1. The one-byte corruption test proves the hash catches a send-offset change: the uncorrupted
   stream matches, the corrupted stream does not.
2. Re-introducing the K265 payload-start defect (`start = max(pipe, off)` instead of `start = off`)
   makes `one_mib_both_ways_matches_hash` fail; restoring `start = off` makes it pass. The mutation
   is temporary and the product file is restored byte-identically.

## Hash

FNV-1a 64-bit, computed over the delivered bytes in order:

```
hash = 14695981039346656037
for byte in stream: hash ^= byte; hash *= 1099511628211
```

The hash is stable across runs and hosts and is printed only indirectly through the assertion; a
mismatch identifies the scenario by name.
