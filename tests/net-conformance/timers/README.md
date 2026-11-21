<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# K292 TCP timer scenarios

The timer matrix has two complementary layers:

| Layer | What it proves | Command |
|---|---|---|
| Host timer state machine | Exact RTO/backoff, keepalive probe timing/count and 2MSL deadlines in virtual milliseconds | `timeout 120 cargo nextest run -p tcp-timer-conformance` |
| Wire harness | RTO retransmits the same sequence and payload on the packet path | `timeout 300 sh scripts/net-conformance.sh --scenario retransmit --target raven` |

The state-machine tests use the K267 FreeBSD timer port (`tcp_timer_rexmt`, `tcp_timer_keep`,
`tcp_timer_2msl`) with explicit `now_ms` values; there is no wall-clock dependency.

## Scenarios

* `scenarios/retransmit.toml` — first data segment, withheld ACK, a retransmission with the same
  sequence and payload within 0.5–4 s, non-decreasing backoff, exact 44-byte delivery.
* `scenarios/keepalive.toml` — zero-length probe with `seq = snd_una-1` at `TP_KEEPIDLE`, repeats
  every `TP_KEEPINTVL`, `TP_KEEPCNT` probes, then `ETIMEDOUT` at `TP_KEEPIDLE + TP_MAXIDLE`.
* `scenarios/time-wait.toml` — final ACK and `TIME-WAIT`; a late SYN on the 4-tuple is dropped; the
  entry expires at `2*MSL`; FIN_WAIT_2 rearms at `TP_KEEPINTVL` until `TP_MAXIDLE`.

The K253 guest fixture holds a connection for 12 s and the stack defaults are `TP_KEEPIDLE` 7200 s
and `2*MSL` 60 s, so the keepalive and TIME-WAIT **wire** variants are narrowed in the TOML files.
The host state-machine scenarios set the short intervals explicitly and assert the exact timing and
segment counts; they are the deterministic proof, while the wire `retransmit` scenario is the
packet-level proof for RTO.
