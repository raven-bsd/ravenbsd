<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP loss and reordering conformance

Status: K323, 2026-11-21. Evidence tags: **OBSERVED** (measured on the merged stack),
**PROPOSED** (defined but not observed), **UNVERIFIED**.

The loss scenarios live in `tests/net-conformance/loss/`. `run.sh` drives them against the K253
net-conformance guest; `run.py` reuses the raw peer and writes one pcap per scenario; `verify.py`
asserts the durable segment counts and repeated sequence/bytes from the pcap.

## 1. Matrix

| Scenario | What is proved | Observed detail |
|---|---|---|
| `single-loss` | one dropped 44-byte segment is retransmitted once, same seq/bytes | `retransmit=yes` |
| `multiple-loss` | a second consecutive drop produces a backed-off retransmit | `retries=2` |
| `dup-ack` | three duplicate ACKs produce a retransmit | `dup-acks=3; retransmit=yes` |
| `fast-retransmit` | the dup-ACK retransmit beats a full RTO | `fast-retransmit=yes after 0.091s` |
| `reordering` | a stale ACK below `snd_una` causes no spurious segment | `spurious-segment=no` |
| `rto` | the loss retransmit waits for the RTO | `retransmit=yes after 2.051s` |

The guest fixture writes exactly 44 bytes once per connection. `multiple-loss` therefore drops
the same segment twice (two consecutive losses), not two different segments. A multi-segment loss
scenario needs a multi-write fixture and remains outside this test-only ticket.

## 2. Exact wire shape

`ISN` is the client ISN; `SISN` the server ISN. The data segment is 44 bytes at `SISN+1`; an ACK
of `SISN+45` acknowledges it.

```text
single-loss / rto:
  server -> client  data       seq=SISN+1 len=44
  server -> client  retransmit seq=SISN+1 len=44   (same bytes; rto waits >= 1 s)

multiple-loss:
  server -> client  data       seq=SISN+1 len=44
  server -> client  retransmit seq=SISN+1 len=44   (first drop)
  server -> client  retransmit seq=SISN+1 len=44   (backoff)

dup-ack / fast-retransmit:
  server -> client  data       seq=SISN+1 len=44
  client -> server  3x ACK     ack=SISN+1 win=65535
  server -> client  retransmit seq=SISN+1 len=44   (fast path)

reordering:
  server -> client  data       seq=SISN+1 len=44
  client -> server  ACK        ack=SISN+45
  client -> server  ACK        ack=SISN+1          (stale)
  client -> server  ACK        ack=SISN+45
  no server data after the stale ACK
```

## 3. Observed pcaps

Merged base `ba38fb70`; net-conformance image SHA-256
`3824f48f950399d3738b53f6176d504c4868e6d3518718e8de012c4b3215cf8f`.

| Scenario | pcap SHA-256 |
|---|---|
| single-loss | `8cfff7706a5f3b1d40145c174d96cf6913f64f14fa7b5542ab32fb228aa0c098` |
| multiple-loss | `555193a78402317626356489079c04b6419b880071cfa1483eff8c84f63f0bb8` |
| dup-ack | `78c76478423e0a7795b44d299802757d183d5bea5d8ceb873b3426160887eaab` |
| fast-retransmit | `9d839c43535df9346091ac335a1d50e6114827e2cc6a087e4774914aca31d338` |
| reordering | `970029fc69d2b885695bdbb66b8f3e2cf5a2622c9a328ecfffe870420054a61d` |
| rto | `a5a807a5b4e4bed7aebda349b188a03237cfa177851d29eaf6794568af3241e4` |

The pcaps are generated under `/tmp/opencode/k323-pcaps/` and are not product files.

## 4. Commands

```sh
timeout 300 sh scripts/net-conformance.sh --scenario retransmit --target raven
timeout 240 sh tests/net-conformance/loss/run.sh --image build/qtest/net-conformance.img
timeout 15 python3 tests/net-conformance/loss/verify.py \
    --scenario single-loss --pcap /tmp/opencode/k323-pcaps/single-loss.pcap
```

## 5. Donor reconciliation

Test-only: the expectations follow FreeBSD `sys/netinet/tcp_timer.c` (`tcp_timer_rexmt`,
`TCPTV_MIN`, backoff) and `sys/netinet/tcp_input.c`/`tcp_output.c` (duplicate ACK, fast
retransmit, stale ACK) at `133344405f532bd54ff3629a77d4c4e062f01fdb`, with the K263 Linux
baseline as the outcome comparison. No donor code is copied.

## 6. Gaps

* The 44-byte single-write fixture limits `multiple-loss` to two consecutive losses of one
  segment; a true multi-segment loss needs a larger fixture and is not claimed here.
* The exact acceptance command rebuilds the userland and can exceed its 300 s cap on a cold cache.
  The scenario itself passes with `--no-build --image build/qtest/net-conformance.img`; the
  timeout attempts and the warm-cache result are recorded in the K323 report.
* `dup-ack` and `fast-retransmit` use the same segment; the latter adds the timing assertion
  (under one second) that distinguishes the fast path from the RTO path.
