# RACK BBR / pacing / timely / probe-RTT model

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only independent oracle for the BBR and pacing edge of the full K269 RACK port. The package is
`tests/rack-bbr-model` (`cargo nextest run -p rack-bbr-model`); it never edits product source.

## Model

| Case | Expected value/action | Test |
|---|---|---|
| Pacing rate | `bw * gain / 256`, startup gain 739 | `pacing_and_target_cwnd_vectors_match_integer_bbr_math` |
| Pacing delay | `len * 1e6 * 256 / (gain * bw)` useconds, min 1 | same test |
| Pacing length | `us * bw * gain / (1e6 * 256)` bytes, min 1 | same test |
| Target cwnd | BDP * cwnd gain, rounded to MSS, +2 quanta, min 4 MSS, capped | same test |
| Startup | high gain, 25% growth, three flat rounds -> Drain | `startup_drains_into_probe_bw_and_cycles_gains` |
| Drain | drain gain 88, leaves when flight <= BDP | same test |
| Probe-BW | eight-substate gains `[1.25, 0.75, 1, 1, 1, 1, 1, 1]` | same test |
| Timely | >=3x min RTT down, <=1.25x min RTT up, else RTT gradient; 80% decrease | `timely_thresholds_and_decrease_gain_match_the_donor` |
| Probe-RTT | every 10 s, 4 MSS for 200 ms, restore saved cwnd | `probe_rtt_enters_after_ten_seconds_caps_cwnd_and_restores` |
| Trace schema | `now,event,action,deadline,value` | `trace_format_is_the_shared_five_column_schema` |
| Randomized | 10 seeds x 1,000 = 10,000 trace steps | `ten_thousand_randomized_trace_steps_are_deterministic_and_bounded` |

The trace schema is the same five-column order used by the K319/K320 models. K320's fifth column
is `reo_wnd`; K326's fifth column carries the BBR value (pacing rate, cwnd or gain). This package
does not duplicate the RACK core model.

Random seeds: `0x32600001` ... `0x3260000a`.

## Donors

FreeBSD `sys/netinet/tcp_stacks/bbr.c` and `rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause), with the BBR/RACK-TLP drafts and
RFC 8985/6675. Illumos has no BBR module; Linux `net/ipv4/tcp_bbr.c` is read for outcome only. No
donor expression is copied.

## Running

```text
cargo nextest run -p rack-bbr-model
```
