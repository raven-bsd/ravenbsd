<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Performance baseline

Machine: Linux 5.19.0-50-generic x86_64, Rust 1.97.1 stable, release profile.
Method: `cargo run -p raven-tcpip-bench --release -- --all --output tests/tcpip/PERFORMANCE.md`.
Virtual-time metrics are exact and identical across runs; wall-clock metrics are medians
of five runs with the spread stated and are never asserted as thresholds.

smoltcp was retired in C32. Its columns are frozen measurements of smoltcp 0.14 taken
with the same harness, scenarios and seeds immediately before removal: once as the old
`raven-netcore` default (no congestion control) and once with `socket-tcp-reno`, the fair
congestion-controlled baseline the verdict is computed against (10% tolerance).

| Metric | raven-tcpip | smoltcp, no CC (frozen) | smoltcp + Reno (frozen, fair) | Verdict vs fair | Method |
|---|---|---|---|---|---|
| Goodput, lossless, 1 ms RTT | 29.41 MiB/s (34 ms) | 31.25 MiB/s (32 ms) | 18.52 MiB/s (54 ms) | better | virtual time, 1 MiB, 5 runs; wall median 1.0 ms, spread 1.6 ms |
| Goodput, lossless, 20 ms RTT | 2.70 MiB/s (370 ms) | 2.72 MiB/s (367 ms) | 1.25 MiB/s (801 ms) | better | virtual time, 1 MiB, 5 runs; wall median 1.2 ms, spread 0.3 ms |
| Goodput, lossless, 200 ms RTT | 0.27 MiB/s (3700 ms) | 0.28 MiB/s (3517 ms) | 0.13 MiB/s (7911 ms) | better | virtual time, 1 MiB, 5 runs; wall median 2.7 ms, spread 0.2 ms |
| Goodput, 1% loss, 20 ms RTT | 1.10 MiB/s (910 ms) | 2.07 MiB/s (484 ms) | 0.10 MiB/s (10181 ms) | better | virtual time, 1 MiB, 5 runs, via raven-netcore PairDevice, loss in per-mille |
| Goodput, 5% loss, 20 ms RTT | 0.29 MiB/s (3446 ms) | 0.17 MiB/s (5941 ms) | 0.01 MiB/s (70691 ms) | better | virtual time, 1 MiB, 5 runs, via raven-netcore PairDevice, loss in per-mille |
| Goodput, 10% reordering, 20 ms RTT | 2.04 MiB/s (490 ms) | 0.64 MiB/s (1567 ms) | 0.32 MiB/s (3151 ms) | better | virtual time, 1 MiB, 5 runs |
| CPU cost per MiB (wall time) | 1.14 ms/MiB | 0.68 ms/MiB | 1.06 ms/MiB | equal | median of 5, spread 0.04 ms; transfer loop only, references measured the same way on this host before removal |
| Time to first byte, 20 ms RTT | 60 ms virtual | 62 ms virtual | 62 ms virtual (not re-measured) | equal | virtual time, 5 identical runs |
