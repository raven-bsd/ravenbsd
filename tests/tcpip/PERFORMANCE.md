<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Performance baseline

Machine: Linux 5.19.0-50-generic x86_64, Rust 1.97.1 stable, release profile.
Method: `cargo run -p raven-tcpip-bench --release -- --all --output tests/tcpip/PERFORMANCE.md`.
Virtual-time metrics are exact and identical across runs; wall-clock metrics are medians
of five runs with the spread stated and are never asserted as thresholds.

| Metric | raven-tcpip | smoltcp baseline | Verdict | Method |
|---|---|---|---|---|
| Goodput, lossless, 1 ms RTT | 20.41 MiB/s (49 ms virtual) | 31.25 MiB/s (32 ms virtual) | worse | virtual time, 1 MiB, 5 runs; wall median 11.0 ms, spread 1.3 ms |
| Goodput, lossless, 20 ms RTT | 1.49 MiB/s (670 ms virtual) | 2.72 MiB/s (367 ms virtual) | worse | virtual time, 1 MiB, 5 runs; wall median 12.0 ms, spread 1.0 ms |
| Goodput, lossless, 200 ms RTT | 0.15 MiB/s (6700 ms virtual) | 0.28 MiB/s (3517 ms virtual) | worse | virtual time, 1 MiB, 5 runs; wall median 17.0 ms, spread 0.8 ms |
| Goodput, 1% loss, 20 ms RTT | 0.08 MiB/s (12197 ms, retrans 97834 B) | 2.07 MiB/s (484 ms) | worse | virtual time, 1 MiB, 5 runs; loss in per-mille on the smoltcp side |
| Goodput, 5% loss, 20 ms RTT | 0.06 MiB/s (16744 ms, retrans 126192 B) | 0.17 MiB/s (5941 ms) | worse | virtual time, 1 MiB, 5 runs; loss in per-mille on the smoltcp side |
| Goodput, 10% reordering, 20 ms RTT | 0.07 MiB/s (13542 ms) | 0.64 MiB/s (1567 ms) | worse | virtual time, 1 MiB, 5 runs |
| CPU cost per MiB (wall time) | 11.7 ms/MiB | 5.0 ms/MiB | worse | median of 5, spread raven 0.1 ms / smoltcp 0.2 ms |
| Time to first byte, 20 ms RTT | 40 ms virtual | 62 ms virtual | equal/better | virtual time, 5 identical runs |
| Memory per idle connection | blocked: no stable per-socket accounting hook | blocked: no stable per-socket accounting hook | blocked | RSS before/after was not stable enough to publish; C24 can add a hook |
| Memory per active connection (1 KiB in flight) | blocked: same accounting hook | blocked: same accounting hook | blocked | not measured in this pass |
| Connection setup rate | blocked: wall-clock handshake loop not automated | blocked: wall-clock handshake loop not automated | blocked | deterministic handshake completion is covered by the conformance suite |
| Classification at 100 / 1000 / 10000 connections | blocked: C17 harness needs a release-mode adapter | blocked: same | blocked | C17 table tests cover the algorithm; wall-clock classification is not asserted |
| Recovery after one mid-transfer loss | blocked: no targetable single-drop hook | blocked: no retransmission counter | blocked | the loss goodput rows show the aggregate effect; C20 can add a single-drop script |

## Verdict

The native stack is compared on the same 1 MiB payload, RTT and loss inputs. Where a row
says `blocked`, the method is stated and no number is invented. The loss and reordering
numbers are the honest aggregate; the single-drop recovery and per-connection memory
hooks are follow-up work for C24.
