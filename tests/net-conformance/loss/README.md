<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K323 TCP loss/reordering scenarios

`run.sh` prepares the K253 tap and RST filter, then runs six scenarios against one
net-conformance image. `run.py` reuses the K253 harness raw peer and writes one pcap per
scenario; `verify.py` re-reads a pcap and asserts the durable segment count/sequence facts.

## Matrix

| Scenario | Live assertion | Offline assertion |
|---|---|---|
| `single-loss` | first 44-byte segment dropped; one retransmit with the same seq/bytes | exactly 2 data frames, 1 retransmit |
| `multiple-loss` | first retransmit also dropped; a second, backed-off retransmit arrives | at least 3 data frames, 2 retransmits |
| `dup-ack` | three duplicate ACKs produce a retransmit | at least 2 data frames, 1 retransmit |
| `fast-retransmit` | the dup-ACK retransmit arrives in under 1 s | at least 2 data frames, 1 retransmit |
| `reordering` | correct ACK, stale ACK, correct ACK; no spurious segment | exactly 1 data frame |
| `rto` | the loss retransmit waits at least 1 s | exactly 2 data frames, 1 retransmit |

The fixture writes exactly one 44-byte segment per connection, so `multiple-loss` means two
consecutive losses of that segment, not two different segments. This is recorded as the fixture
limit, not a silent scope reduction.

## Observed run

Merged base `ba38fb70`, image SHA-256 `3824f48f950399d3738b53f6176d504c4868e6d3518718e8de012c4b3215cf8f`.

| Scenario | Result | pcap SHA-256 | Detail |
|---|---|---|---|
| single-loss | PASS | `8cfff7706a5f3b1d40145c174d96cf6913f64f14fa7b5542ab32fb228aa0c098` | `retransmit=yes` |
| multiple-loss | PASS | `555193a78402317626356489079c04b6419b880071cfa1483eff8c84f63f0bb8` | `retries=2` |
| dup-ack | PASS | `78c76478423e0a7795b44d299802757d183d5bea5d8ceb873b3426160887eaab` | `retransmit=yes` |
| fast-retransmit | PASS | `9d839c43535df9346091ac335a1d50e6114827e2cc6a087e4774914aca31d338` | `fast-retransmit=yes after 0.091s` |
| reordering | PASS | `970029fc69d2b885695bdbb66b8f3e2cf5a2622c9a328ecfffe870420054a61d` | `spurious-segment=no` |
| rto | PASS | `a5a807a5b4e4bed7aebda349b188a03237cfa177851d29eaf6794568af3241e4` | `retransmit=yes after 2.051s` |

## Commands

```sh
timeout 300 sh scripts/net-conformance.sh --scenario retransmit --target raven
timeout 240 sh tests/net-conformance/loss/run.sh --image build/qtest/net-conformance.img
timeout 15 python3 tests/net-conformance/loss/verify.py \
    --scenario single-loss --pcap /tmp/opencode/k323-pcaps/single-loss.pcap
```

The live runner exits non-zero if any scenario misses its segment count or timing. The offline
verifier exits non-zero on a missing retransmit, a changed payload or a duplicate spurious segment.
