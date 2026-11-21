<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# TCP timer conformance (K292)

The timer matrix proves the K267 FreeBSD timer state machine at exact virtual times and the RTO
retransmission on the K253/K275 wire harness.

## Matrix

| Scenario | Layer | Assertion | Command |
|---|---|---|---|
| RTO/backoff | host state machine | first retransmit at the 1 s callout, same sequence as `snd_una`, non-decreasing backoff, 12 retransmits then `ETIMEDOUT` | `cargo nextest run -p tcp-timer-conformance rto_retransmits_same_sequence_with_backoff_before_drop` |
| RTO on the wire | K253 harness | a data segment with the withheld ACK is retransmitted with the same sequence/payload | `scripts/net-conformance.sh --scenario retransmit --target raven` |
| Keepalive | host state machine | probes at `TP_KEEPIDLE` 1000 ms and every `TP_KEEPINTVL` 500 ms, `TP_KEEPCNT` 3 probes, `ETIMEDOUT` at 2500 ms | `cargo nextest run -p tcp-timer-conformance keepalive_probes_at_keepidle_and_keepintvl_then_drops_after_maxidle` |
| Keepalive off | host state machine | no probe; `TT_KEEP` rearms at `TP_KEEPIDLE` | `cargo nextest run -p tcp-timer-conformance keepalive_off_rearms_without_a_probe` |
| 2MSL | host state machine | `TIME-WAIT` closes at `2*MSL`; `FIN_WAIT_2` rearms at `TP_KEEPINTVL` until `TP_MAXIDLE` | `cargo nextest run -p tcp-timer-conformance two_msl_closes_time_wait_at_the_armed_deadline` |

No wall-clock dependency: every deadline is an explicit `now_ms` input. Every stream/segment
assertion is a count, sequence, payload or virtual deadline.

## Wire narrowing

The K253 guest fixture holds one connection for 12 s. The timer defaults are `TP_KEEPIDLE` 7200 s
and `2*MSL` 60 s, so a wire keepalive or 2MSL scenario cannot complete inside the harness window
without a fixture that sets short socket options. `tests/net-conformance/timers/scenarios/`
records the exact expected packets and marks those two wire variants `narrowed`; the host
state-machine tests cover the same behaviour with the short intervals set explicitly. The RTO
scenario is not narrowed and is the packet-level acceptance.

## Run

```sh
timeout 120 cargo nextest run -p tcp-timer-conformance
timeout 300 sh scripts/net-conformance.sh --scenario retransmit --target raven
timeout 420 make check-fast
```

## Donor

FreeBSD `sys/netinet/tcp_timer.c` at `133344405f532bd54ff3629a77d4c4e062f01fdb`
(`tcp_timer_rexmt`, `tcp_timer_keep`, `tcp_timer_2msl`), already ported by K267. Test-only; no donor
code copied.
