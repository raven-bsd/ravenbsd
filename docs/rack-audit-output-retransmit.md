# RACK output/retransmit audit

SPDX-License-Identifier: BSD-3-Clause
Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

Host-only audit of the merged K330 output/timer and K331 ACK/retransmit groups. The package is
`tests/rack-audit-output-retransmit` (`cargo nextest run -p rack-audit-output-retransmit`); it reads
product sources/tests and never edits them.

## Audit scope

34 donor functions: K330's 21 output/timer functions and K331's 13 ACK/retransmit functions. Each
row carries the merged module, named unit test and mutation/negative check. The auditor checks the
Rust mapping, the test name, a non-empty mutation row and the donor constants.

## Findings (2026-11-21)

| Finding | Detail | Owner |
|---|---|---|
| wrong donor constant | `rack_retransmit.rs` `TCPS_CLOSING = 9`; FreeBSD `tcp_fsm.h` is 8 | K331 |
| wrong donor constant | `rack_retransmit.rs` `TCPS_LAST_ACK = 10`; FreeBSD is 9 | K331 |
| duplicate type | `TCPS_FIN_WAIT_1`, `TCPS_FIN_WAIT_2`, `TCPS_CLOSING`, `TCPS_LAST_ACK` are declared in both `rack_output.rs` and `rack_retransmit.rs` | K330/K331 |
| conflicting duplicate | duplicated `TCPS_CLOSING`/`TCPS_LAST_ACK` values disagree (8/9 vs 9/10) | K330/K331 |

All 34 functions have a Rust mapping, a named test and a non-empty mutation/negative row.

## Function matrix

| Function | Rust | Test | Mutation | Status |
|---|---|---|---|---|
| `rack_output` | `rack_output::rack_output` | `rack_output_sends_new_data_and_arms_rxt` | omit SND.MAX advance | PASS |
| `tcp_rack_output` | `rack_output::tcp_rack_output` | `tcp_rack_output_returns_default_ok_and_candrop` | closed state CANDROP | PASS |
| `rack_timer_start` | `rack_output::rack_timer_start` | `rack_timer_start_and_rack_timer_cancel` | slot not returned | PASS |
| `rack_timer_cancel` | `rack_output::rack_timer_cancel` | `rack_timer_start_and_rack_timer_cancel` | pending bit not cleared | PASS |
| `rack_timer_audit` | `rack_output::rack_timer_audit` | `rack_timer_audit_returns_due_bits_only` | early bits reported | PASS |
| `rack_timeout_rack` | `rack_output::rack_timeout_rack` | `rack_timeout_rack_requests_retransmit` | must_retran not set | PASS |
| `rack_timeout_tlp` | `rack_output::rack_timeout_tlp` | `rack_timeout_tlp_is_bounded_to_two_probes` | third probe allowed | PASS |
| `rack_timeout_delack` | `rack_output::rack_timeout_delack` | `rack_timeout_delack_sets_acknow` | ACKNOW not set | PASS |
| `rack_timeout_persist` | `rack_output::rack_timeout_persist` | `rack_timeout_persist_probes_and_rearms` | probe not sent | PASS |
| `rack_timeout_keepalive` | `rack_output::rack_timeout_keepalive` | `rack_timeout_keepalive_counts_and_closes` | never exhausts | PASS |
| `rack_timeout_rxt` | `rack_output::rack_timeout_rxt` | `rack_timeout_rxt_backs_off_and_closes_at_the_limit` | RXT never closes | PASS |
| `rack_remxt_tmr` | `rack_output::rack_remxt_tmr` | `rack_remxt_tmr_sets_override_and_backoff` | shift not moved | PASS |
| `rack_stopall` | `rack_output::rack_stopall` | `rack_stopall_refuses_future_timers` | future timer accepted | PASS |
| `rack_stop_all_timers` | `rack_output::rack_stop_all_timers` | `rack_stop_all_timers_reports_persist` | persist flag dropped | PASS |
| `rack_set_state` | `rack_output::rack_set_state` | `rack_set_state_updates_timers_and_rack_handoff_ok_checks_it` | closed timers not stopped | PASS |
| `rack_handoff_ok` | `rack_output::rack_handoff_ok` | `rack_set_state_updates_timers_and_rack_handoff_ok_checks_it` | FIN handoff accepted | PASS |
| `rack_enter_persist` | `rack_output::rack_enter_persist` | `rack_output_enters_persist_on_zero_window` | persist never entered | PASS |
| `rack_exit_persist` | `rack_output::rack_exit_persist` | `rack_enter_persist_and_rack_exit_persist` | second exit accepted | PASS |
| `rack_start_hpts_timer` | `rack_output::rack_start_hpts_timer` | `rack_start_hpts_timer_arms_a_slot` | slot not armed | PASS |
| `rack_process_timers` | `rack_output::rack_process_timers` | `rack_process_timers_dispatches_in_order` | dispatch order changed | PASS |
| `rack_get_persists_timer_val` | `rack_output::rack_get_persists_timer_val` | `rack_get_persists_timer_val_is_bounded` | clamp removed | PASS |
| `rack_ack_received` | `rack_retransmit::rack_ack_received` | `rack_ack_received_applies_abc_and_the_early_recovery_clamp` | early clamp disabled | PASS |
| `rack_process_ack` | `rack_retransmit::rack_process_ack` | `rack_process_ack_advances_una_and_exits_recovery` | finish_recovery removed | PASS |
| `rack_update_entry` | `rack_retransmit::rack_update_entry` | `rack_update_entry_splits_partial_and_reports_overhang` | split None | PASS |
| `rack_update_rsm` | `rack_retransmit::rack_update_rsm` | `rack_update_rsm_counts_retransmits_and_moves_time_order` | r_rtr_cnt not incremented | PASS |
| `rack_update_seg` | `rack_retransmit::rack_update_seg` | `rack_update_seg_keeps_the_pacing_minimum_at_the_mss` | pace minimum dropped | PASS |
| `rack_update_rtt` | `rack_retransmit::rack_update_rtt` | `rack_update_rtt_tracks_the_minimum_and_rejects_spurious_samples` | spurious sample accepted | PASS |
| `rack_what_can_we_send` | `rack_retransmit::rack_what_can_we_send` | `rack_what_can_we_send_is_limited_by_cwnd_rwnd_and_avail` | rwnd cap removed | PASS |
| `rack_find_lowest_rsm` | `rack_retransmit::rack_find_lowest_rsm` | `rack_find_lowest_rsm_skips_acked_entries` | ACKed entry selected | PASS |
| `rack_find_high_nonack` | `rack_retransmit::rack_find_high_nonack` | `rack_find_high_nonack_skips_acked_and_fin_segments` | FIN not skipped | PASS |
| `rack_strike_dupack` | `rack_retransmit::rack_strike_dupack` | `rack_strike_dupack_retransmits_on_the_third_dupack` | threshold > 3 | PASS |
| `rack_fastack` | `rack_retransmit::rack_fastack` | `rack_fastack_accepts_only_clean_advancing_acks` | recovery ACK accepted | PASS |
| `rack_init_retransmit_value` | `rack_retransmit::rack_init_retransmit_value` | `rack_init_retransmit_value_selects_the_three_modes` | control modes swapped | PASS |
| `rack_fast_rsm_output` | `rack_retransmit::rack_fast_rsm_output` | `rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe` | PRR not decremented | PASS |

## K315 row

The 34-row matrix is validated by the K315 checker. The group contributes no missing, duplicate,
unknown, empty or non-PASS row; the real-donor run still lists the other RACK groups.

## Running

```text
cargo nextest run -p rack-audit-output-retransmit
```
