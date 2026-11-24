# RACK output/timer/HPTS model

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only independent oracle for RACK output/timer/HPTS scheduling in the full K269 port. The
package is `tests/rack-timer-model` (`cargo nextest run -p rack-timer-model`); it never edits
product source and it does not duplicate the K319 loss/reordering core or the K320 TLP/DSACK
thresholds.

## Model

| Case | Model expectation | Test |
|---|---|---|
| Slot conversion | 10 us slots, round up: 0 -> 0, 1..10 -> 1, 11 -> 2 | `slot_conversion_and_timeout_cap_are_exact` |
| Timeout cap | `rack_start_hpts_timer` cap `0x7ffffffe` us | same test |
| Queue order | ascending slot, FIFO within a slot | `schedule_orders_by_slot_then_fifo_within_a_slot` |
| Cancel | clear flags, invalidate the queued reference, no callback | `cancel_invalidates_the_queued_callback` |
| Output gates | CLOSED/LISTEN and an already-queued connection are refused | `output_refuses_closed_listen_and_already_queued` |
| Stop-all | `rack_stopall` blocks every future output | `stopall_blocks_future_output` |
| Persist | `rack_stop_all_timers` enters persist; `exit_persist` resumes | `stop_all_timers_enters_persist_until_exit` |
| Remxt | `rack_remxt_tmr` cancels and sets `r_timer_override`; output clears it | `remxt_cancels_and_sets_override_until_output` |
| Dispatch priority | DELACK, RACK, TLP, RXT, PERSIST, KEEP | `dispatch_priority_matches_rack_process_timers` |
| Early slot wakeup | reinsert the remaining time; never fire before the deadline | `softclock_reinserts_an_early_slot_instead_of_firing` |
| Zero delay | no timer selected; leave the connection off the wheel | `zero_delay_does_not_arm_a_timer` |
| Random traces | 10,000 seeds, deterministic replay and invariants | `randomized_10k_traces_are_deterministic_and_invariant` |
| Golden artifact | `traces/rack-timer-10k.tsv` matches the generator | `trace_artifact_matches_generation` |

## Trace format

`traces/rack-timer-10k.tsv` follows the K319 `rack-10k.tsv` convention: two comment header
lines, one row per seed `1..=10000`, tab-separated columns:

```text
seed  events  scheduled  refused  cancelled  fired  early  order  actions  digest
```

* `order`: comma-separated `id@deadline_us/fired_at_us`, or `-`.
* `early`: HPTS slot wakeups reinserted because the logical deadline had not arrived.
* `actions`: comma-separated `DELACK|RACK|TLP|RXT|PERSIST|KEEP`, or `-`.
* `digest`: FNV-1a 64-bit digest of the canonical event and callback sequence
  (`canonical_trace`), so an input or deadline change is visible.

Regenerate with:

```text
timeout 120 cargo run -q -p rack-timer-model -- \
  --generate tests/rack-timer-model/traces/rack-timer-10k.tsv
```

Current file: 10,000 rows, 52,386 scheduled, 110,840 refused, 31,592 cancelled,
20,794 fired callbacks, 15 early slot reinserts. SHA-256:
`64d1e2bacf8db5cc37001470e2474ac812ebc5fb5147b4b053f4fba0d4b97040`.

## Donor reconciliation

Test-only; the model is original Rust and no donor expression is copied. The exact FreeBSD
functions read for behaviour are `sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause): `rack_start_hpts_timer` 6444,
`rack_timer_start` 6050, `rack_timer_cancel` 7981, `rack_remxt_tmr` 7437, `rack_stopall` 8018,
`rack_stop_all_timers` 8031, `rack_process_timers` 7868, `rack_output` 19601, and the
`rack_timeout_*` bodies 6800/7086/7275/7321/7394/7582; `sys/netinet/tcp_hpts.c`:
`__tcp_hpts_insert` 823, `tcp_hpts_insert_internal` 505, `__tcp_hpts_remove` 580, `tcp_hptsi`
1084, `_tcp_hpts_softclock` 1567; `tcp_hpts.h` 30/85-92; `tcp_hpts_internal.h` 50/69.

Illumos `usr/src/uts/common/inet/tcp/tcp_timers.c` at `586678ac` (CDDL, behaviour only) has
`tcp_timeout` 136, `tcp_timer_handler` 190 and `tcp_timeout_cancel` 231: one callout id per
timeout, with no RACK HPTS wheel. The Linux `net/ipv4/tcp_output.c` and `net/ipv4/tcp_timer.c`
paths at `adc21867` are absent from this sparse checkout; Linux is outcome-only and was not used
to derive the model. RFC 8985 and RFC 6675 define the boundary: this model consumes the
caller-computed delay from K319/K320/K269 rather than restating the loss or TLP algorithms.

## Limits

* No loss marking, reordering-window growth, TLP threshold or retransmit selection; those are
  K319/K320/K269.
* No HPTS CPU fan-out, fixed ring wrap, `t_hpts_request`, pacing, delayed-ACK coalescing or
  keepalive fallback; the sparse wheel covers the timer deadline/order semantics.
* A zero delay means no timer was selected; HPTS slot 0 for pacing is outside this oracle.

These limits are deliberate: the oracle covers the timer/HPTS scheduling layer that the future
`Rack` implementation must match.
