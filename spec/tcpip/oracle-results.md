<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Behaviour-oracle results

Status: C24 fill of the plan in `oracle.md`.
Reference stack: Linux 5.19.0-50-generic x86_64 (Ubuntu), measured as a running
system on the authoring host. Our stack is `raven-tcpip` at C24.
Evidence tags: OBSERVED, NOT-MEASURED. No reference value is guessed.

Capture limitation: `tcpdump` on this host reports
`tcpdump: lo: You don't have permission to capture on that device` (no
`CAP_NET_RAW`). Absolute option order and injected-packet RST/challenge/cadence
rows are therefore NOT-MEASURED on the reference side. Our side is observed
from the C04 trace and the C19/C20 tests; those rows cannot be called MATCH
without a reference capture.

Commands used:

```
uname -srm                          # Linux 5.19.0-50-generic x86_64
for k in tcp_fin_timeout tcp_keepalive_time tcp_keepalive_intvl \
         tcp_keepalive_probes tcp_challenge_ack_limit tcp_ecn tcp_sack \
         tcp_timestamps tcp_window_scaling tcp_syn_retries tcp_retries2; do
  printf '%s=' "$k"; cat /proc/sys/net/ipv4/$k
done
ss -tinm '( sport = :42919 or dport = :42919 )'   # C01 oracle.md 3.2/3.3
cargo test -p raven-tcpip-oracle
cargo test -p raven-tcpip --test tcp_options syn_option_order -- --exact
cargo run -p raven-tcpip-conformance -- --all
```

| ID | Behaviour | Reference stack | Reference value | Our value | Verdict | Evidence |
|---|---|---|---|---|---|---|
| OR-01 | Option set and order on SYN, SYN-ACK, first data | Linux 5.19 | NOT-MEASURED: no capture permission; `ss` cannot show option order | SYN: MSS, SACK, NOP, TS, NOP, WS, EOL (observed in the C04 trace) | NOT-MEASURED | Reference: `tcpdump` permission error. Ours: `tcp_options.rs::syn_option_order_is_mss_sack_nop_ts_nop_ws_eol`. |
| OR-02 | Initial congestion window | Linux 5.19 | 10 segments (`ss -tinm`, `cwnd:10`) | `tcp.initial_window = 10` segments | MATCH | `ss` in `oracle.md` §3.2; `raven-tcpip-oracle::measured_initial_window_and_duplicate_ack_threshold_are_pinned`. |
| OR-03 | Initial RTO before the first sample | Linux 5.19 | NOT-MEASURED: `ss` reports the post-sample 200 ms floor, not the pre-sample value | 1000 ms (`tcp.rto_initial`) | NOT-MEASURED | `ss -tinm` in §3.2 is a floor observation; ours: `tcp.rto_initial` property. |
| OR-04 | Delayed-ACK delay and immediate-ACK rule | Linux 5.19 | 40 ms (`ato:40` on the receiver) | 40 ms (`tcp.delack_delay`); immediate after a full MSS or 2 segments | MATCH | `ss` in §3.2/§3.3; `delack_delay_controls_the_timer` and `delack_segments_controls_immediate_acks`. |
| OR-05 | Duplicate-ACK threshold and fast-retransmit time | Linux 5.19 | NOT-MEASURED: not exposed by `ss` without a capture | threshold 3 (`tcp.dup_ack_threshold`); one RTT after the third duplicate | NOT-MEASURED | Reference capture unavailable; ours: `dup_ack_threshold_controls_fast_retransmit`. |
| OR-06 | RST vs silent drop for a closed port | Linux 5.19 | RST, immediate `ECONNREFUSED` | RST from the no-TCB path, counted by `tcp.rsts_for_unsent` | MATCH | `oracle.md` §3.4 Python `connect`; `observability_counters::rst_for_unsent_counter_moves`, `rst/06-ack-to-listener-rst.pkt`. |
| OR-07 | RST vs silent drop: wrong sequence in window, out-of-window, TIME-WAIT | Linux 5.19 | NOT-MEASURED: needs raw injection and capture | challenge ACK for in-window wrong sequence; silent drop out of window; RST ignored in TIME-WAIT | NOT-MEASURED | Reference capture unavailable; ours: `rst/03`, `rst/04`, `rst/05` scripts and C19 counters. |
| OR-08 | Challenge-ACK behaviour and rate limit | Linux 5.19 | 1000 global tokens/s (`tcp_challenge_ack_limit=1000`); per-connection limit not exposed | 1000 global tokens per 1000 ms, plus 10 per-connection tokens per 5000 ms | MATCH (global); DEVIATION-ACCEPTED (per-connection) | `/proc/sys/net/ipv4/tcp_challenge_ack_limit`; `accepted_deviation_challenge_ack_limits_are_pinned`. |
| OR-09 | Keepalive onset, interval, probe count | Linux 5.19 | onset 7200 s, interval 75 s, probes 9 (`tcp_keepalive_*`) | onset 7200 s, probe interval 7200 s, probes 9 | DEFECT (C24-D01) | `/proc/sys/net/ipv4/tcp_keepalive_*`; ours `tcp.keepalive_interval` is reused as the probe interval. |
| OR-10 | FIN-WAIT-2 and TIME-WAIT durations | Linux 5.19 | FIN-WAIT-2 configured 60 s; TIME-WAIT observed ~62 s | 60 s and 60 s defaults | MATCH | `oracle.md` §3.1/§3.5; `measured_timewait_and_finwait2_defaults_are_pinned`. |
| OR-11 | Zero-window probe cadence and backoff | Linux 5.19 | NOT-MEASURED: needs a non-reading receiver and a capture | start 1 s, double, cap 60 s (`tcp.zwprobe_max_interval`) | NOT-MEASURED | Reference capture unavailable; ours: `zero-window/01-zero-window-probe.pkt` and C19 property test. |
| OR-12 | MSS announced for v4 and v6 on the same interface | Linux 5.19 | v4 65483, v6 65464 on loopback (different MTU from our test link) | v4 1446, v6 1426 on a 1500-byte frame device | NOT-MEASURED | `oracle.md` §3.2/§3.3 used a loopback MTU; the interface is not the same. |
| OR-13 | ECN negotiation and CE response timing | Linux 5.19 | `tcp_ecn=2`: passive ECN only; no capture of the CE response | `tcp.ecn_enable=1`: ECE+CWR offered on every active SYN | DEVIATION-ACCEPTED (C24-A01) | `/proc/sys/net/ipv4/tcp_ecn`; `accepted_deviation_ecn_is_offered_by_default`. |
| OR-14 | Simultaneous open | Linux 5.19 | NOT-MEASURED: needs two controlled endpoints and capture | not implemented (SYN in SYN-SENT is ignored) | DEFECT (C20-D02) | `establishment/09-simultaneous-open.pkt` is committed as skip. |
| OR-15 | Simultaneous close | Linux 5.19 | NOT-MEASURED: needs two controlled endpoints and capture | FIN in FIN-WAIT-1 moves to TIME-WAIT and sends the final ACK in the C20 close scripts; the simultaneous FIN+ACK case is C20-D01 | NOT-MEASURED | Reference capture unavailable; `establishment/03`, `establishment/07`. |

## Filed defects

* **C24-D01** Keepalive probe interval: `tcp.keepalive_interval` is used for both
  the initial delay and every probe. Linux separates onset (7200 s) from the
  probe interval (75 s). Owning area: C19 observability / TCP timers.
* **C20-D02** Simultaneous open is not implemented (already filed by C20).
* **C23-D01** Loss and reordering recovery is far behind the smoltcp baseline;
  the C23 table is the evidence.
* **C20-D01** FIN+ACK in FIN-WAIT-1 omits the final ACK (already filed).

## Accepted deviations

These are proposed for reviewer acceptance and are pinned by
`raven-tcpip-oracle`:

* **C24-A01** ECN is offered on every active SYN by default. The stack is a
  library and `tcp.ecn_enable` is a documented property; Linux `tcp_ecn=2` is
  passive-only. Regression:
  `accepted_deviation_ecn_is_offered_by_default`.
* **C24-A02** The challenge-ACK limiter has both the Linux global bucket
  (1000/s) and a per-connection bucket (10/5 s). The extra bucket is stricter,
  not weaker. Regression:
  `accepted_deviation_challenge_ack_limits_are_pinned`.

## Not measurable here

The reference-side option order, wrong-sequence RST/challenge behaviour,
zero-window cadence, simultaneous open/close, and same-interface MSS all
require a raw capture or injected packets on the host; this session has no
`CAP_NET_RAW`. They are listed above as NOT-MEASURED rather than guessed. A
future C24 rerun with TAP/capture can fill them without changing our code.
