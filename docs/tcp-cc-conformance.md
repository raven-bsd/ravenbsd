<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# CUBIC/RACK packet-level scenarios (K289)

Packet-level CC scenarios over the K253 raw-peer harness. The guest runs a
test-only bulk sender (`tests/net-conformance/cc/bulk.c`, `/bin/tcpcc`); the
host peer controls ACKs and the receive window, so the scenarios observe
wire behaviour instead of trusting stack-internal counters.

## Scenarios

| Scenario | Wire assertion |
|---|---|
| `slow-start` | the initial flight grows across ACKed bursts: `1460, 14600, 16060, ...` for the 256 KiB transfer |
| `congestion-avoidance` | after a forced fast retransmit, two post-recovery bursts grow additively (`1460, 1460`), not by doubling |
| `loss-recovery` | one unacknowledged segment plus three duplicate ACKs causes a retransmit of the same sequence and bytes |
| `reordering` | a stale ACK below `snd_una` followed by the current ACK causes no spurious segment |

The guest sender reports `cc: listening port=23091`, `cc: accepted`, `cc:
sent=...` and `cc: closed`; the peer resets the connection after each assertion.

## Commands

```sh
timeout 600 sh tests/net-conformance/cc/build.sh
NETCONF_TAP=nctap1 CC_NO_BUILD=1 timeout 300 \
    sh tests/net-conformance/cc/run.sh --scenario slow-start
NETCONF_TAP=nctap1 CC_NO_BUILD=1 timeout 300 \
    sh tests/net-conformance/cc/run.sh --scenario congestion-avoidance
NETCONF_TAP=nctap1 CC_NO_BUILD=1 timeout 300 \
    sh tests/net-conformance/cc/run.sh --scenario loss-recovery
NETCONF_TAP=nctap1 CC_NO_BUILD=1 timeout 300 \
    sh tests/net-conformance/cc/run.sh --scenario reordering
```

The runner writes one pcap per scenario under `/tmp/opencode/k289-pcaps/` and
prints its SHA-256. `--tap` defaults to `NETCONF_TAP` or `nctap0`; use a
distinct tap when another lane is running the same harness.

## Results (2026-11-21, base `a8c7843e`)

| Scenario | Exit | Detail | pcap SHA-256 |
|---|---:|---|---|
| `slow-start` | 0 | `bursts=[1460, 14600, 5840, 16060, 32120, 14600]` | `5da02fe720b357ccb3ea950316d1d2d8a5a033af97c9c41f99b030f084a008f5` |
| `loss-recovery` | 0 | `missing=1460B retransmit=yes` | `7b7e6e7698459ee98d58b378c5e2f93a519b54891878f62bd413a994335798e0` |
| `congestion-avoidance` | 0 | `post-recovery bursts=1460,1460 additive-limit=yes` | `ae094cb8678428cf2d7a80eedc3346093582ec57c706e698dd3435a1fd9d4627` |
| `reordering` | 0 | `stale-ack=ignored spurious-segment=no` | `52cf4ee23a71c67085e5cbc6d05d3fdb2f870f4dec1528581cc48928df6bab8f` |

The required K253 retransmit scenario passes on both sides: the Raven target
`first=44B seq+0; retransmit=yes` (423.03 s including the full image build;
the first 300 s attempt expired in the userland build), and the Linux baseline
`first=44B seq+0; retransmit=yes` (0.53 s). The CC image above was built from
the current tree in 743.84 s.

## Donor reconciliation

Test-only; no donor code copied.

| Topic | FreeBSD/DragonFly (permissive reference) | Linux (outcome only) | Raven decision |
|---|---|---|---|
| CUBIC growth/recovery | `sys/netinet/cc/cc_cubic.c` (`cubic_ack_received`, `cubic_cong_signal`, `cubic_post_recovery`), `sys/netinet/tcp_stacks/rack.c` (RACK ACK/loss path) | `net/ipv4/tcp_cubic.c`; K263 Linux baselines | raw-frame assertions on burst growth and replay; no product/CC source edit |
| RACK loss/reordering | `rack.c` `rack_ack_received`, `rack_process_ack`, reordering window | `tcp_input.c` RACK outcome | duplicated ACK and stale ACK wire checks |
| bulk fixture | none | none | first-party no_std C sender under `tests/net-conformance/cc/` |

The guest fixture and runner are original test code. The Linux K263 baseline
cannot distinguish CUBIC from NewReno on a short single-flow transfer without CC
introspection, so the wire assertions use burst growth rather than an exact
CUBIC curve.
