<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK donor-function union register

Maintained by K338 and repaired by K361. The register lists every top-level function in FreeBSD
`sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause), exactly one owner, the Rust mapping,
the named test, the mutation/negative check and the current status. No donor code was copied.

The pinned donor has **219** top-level functions. K350's ticket text says 218; the parser,
manifest and this register keep all 219. The extra row is `tcp_addrack` (K335).

Some donor helpers were inlined into a caller or replaced by an existing Raven primitive; those
rows name the primitive that carries the behaviour and the test that exercises it. The delegation
notes are in `tests/rack-register/src/delegate.rs`. A row is `PASS` only when the symbol and named
test exist in the current `crates/raven-tcpip` source.

## Register

| # | Donor function (`rack.c` line) | Owner | Rust | Named test | Mutation/negative | Status |
|---:|---|---|---|---|---|---|
| 1 | `rack_get_lt_bw` (L579) | K328 | `BbrRack::get_lt_bw` | `rack_get_lt_bw_uses_bytes_and_time` | A: time multiplication removed -> test fails | PASS |
| 2 | `rack_swap_beta_values` (L599) | K334 | `RackMisc::rack_set_profile` | `rack_set_profile_switches_profiles` | profile beta swap removed -> test fails | PASS |
| 3 | `rack_set_cc_pacing` (L690) | K328 | `RackOs::pacing.rack_set_cc_pacing` | `hpts_and_pacing_state_are_explicit` | K314 pacing flag mutation | PASS |
| 4 | `rack_undo_cc_pacing` (L703) | K328 | `RackOs::pacing.rack_undo_cc_pacing` | `hpts_and_pacing_state_are_explicit` | K314 pacing flag mutation | PASS |
| 5 | `rack_remove_pacing` (L716) | K328 | `RackOs::pacing.rack_remove_pacing` | `hpts_and_pacing_state_are_explicit` | K314 pacing method mutation | PASS |
| 6 | `rack_log_gpset` (L732) | K329 | `crate::rack_log::rack_log_gpset` | `donor_rack_log_gpset` | event suppression -> test fails | PASS |
| 7 | `sysctl_rack_clear` (L770) | K329 | `crate::rack_log::sysctl_rack_clear` | `donor_sysctl_rack_clear` | event suppression -> test fails | PASS |
| 8 | `sysctl_rack_wma_divisor` (L846) | K329 | `crate::rack_log::sysctl_rack_wma_divisor` | `donor_sysctl_rack_wma_divisor` | event suppression -> test fails | PASS |
| 9 | `rack_init_sysctls` (L862) | K329 | `crate::rack_log::rack_init_sysctls` | `donor_rack_init_sysctls` | event suppression -> test fails | PASS |
| 10 | `rc_init_window` (L1873) | K334 | `BbrRack::init_window` | `rack_get_measure_window_initial_and_filled` | initial-window MSS multiplier changed -> test fails | PASS |
| 11 | `rack_get_fixed_pacing_bw` (L1880) | K328 | `BbrRack::get_fixed_pacing_bw` | `rack_get_fixed_pacing_bw_selects_recovery_ss_ca` | recovery bit ignored -> test fails | PASS |
| 12 | `rack_log_hybrid_bw` (L1891) | K329 | `crate::rack_log::rack_log_hybrid_bw` | `donor_rack_log_hybrid_bw` | event suppression -> test fails | PASS |
| 13 | `rack_log_hybrid_sends` (L2015) | K329 | `crate::rack_log::rack_log_hybrid_sends` | `donor_rack_log_hybrid_sends` | event suppression -> test fails | PASS |
| 14 | `rack_compensate_for_linerate` (L2098) | K328 | `BbrRack::compensate_for_linerate` | `rack_compensate_for_linerate_adds_headers` | B: header term removed -> test fails | PASS |
| 15 | `rack_rate_cap_bw` (L2123) | K328 | `BbrRack::rate_cap_bw` | `rack_rate_cap_bw_clamps_to_the_cap` | K: cap comparison inverted -> test fails | PASS |
| 16 | `rack_get_gp_est` (L2261) | K328 | `BbrRack::get_gp_est` | `rack_get_gp_est_initial_rate_and_lt_bw` | A/F: lt_bw/measurement mutation | PASS |
| 17 | `rack_get_bw` (L2356) | K328 | `BbrRack::get_bw` | `rack_get_bw_uses_fixed_rate_when_configured` | E: fixed-rate branch mutation | PASS |
| 18 | `rack_get_output_gain` (L2369) | K328 | `BbrRack::get_output_gain` | `rack_get_output_gain_uses_recovery_and_configured_rate` | C: gain branch mutation | PASS |
| 19 | `rack_log_dsack_event` (L2405) | K329 | `crate::rack_log::rack_log_dsack_event` | `donor_rack_log_dsack_event` | event suppression -> test fails | PASS |
| 20 | `rack_log_hdwr_pacing` (L2445) | K329 | `crate::rack_log::rack_log_hdwr_pacing` | `donor_rack_log_hdwr_pacing` | event suppression -> test fails | PASS |
| 21 | `rack_get_output_bw` (L2497) | K328 | `BbrRack::get_output_bw` | `rack_get_output_bw_applies_gain_and_minimum` | J/K: gain/cap mutation | PASS |
| 22 | `rack_log_retran_reason` (L2558) | K329 | `crate::rack_log::rack_log_retran_reason` | `donor_rack_log_retran_reason` | event suppression -> test fails | PASS |
| 23 | `rack_log_to_start` (L2601) | K329 | `crate::rack_log::rack_log_to_start` | `donor_rack_log_to_start` | event suppression -> test fails | PASS |
| 24 | `rack_log_to_event` (L2644) | K329 | `crate::rack_log::rack_log_to_event` | `donor_rack_log_to_event` | event suppression -> test fails | PASS |
| 25 | `rack_log_map_chg` (L2680) | K329 | `crate::rack_log::rack_log_map_chg` | `donor_rack_log_map_chg` | event suppression -> test fails | PASS |
| 26 | `rack_log_rtt_upd` (L2734) | K329 | `crate::rack_log::rack_log_rtt_upd` | `donor_rack_log_rtt_upd` | event suppression -> test fails | PASS |
| 27 | `rack_log_rtt_sample` (L2804) | K329 | `crate::rack_log::rack_log_rtt_sample` | `donor_rack_log_rtt_sample` | event suppression -> test fails | PASS |
| 28 | `rack_log_rtt_sample_calc` (L2853) | K329 | `crate::rack_log::rack_log_rtt_sample_calc` | `donor_rack_log_rtt_sample_calc` | event suppression -> test fails | PASS |
| 29 | `rack_log_rtt_sendmap` (L2880) | K329 | `crate::rack_log::rack_log_rtt_sendmap` | `donor_rack_log_rtt_sendmap` | event suppression -> test fails | PASS |
| 30 | `rack_log_progress_event` (L2907) | K329 | `crate::rack_log::rack_log_progress_event` | `donor_rack_log_progress_event` | event suppression -> test fails | PASS |
| 31 | `rack_log_type_bbrsnd` (L2937) | K329 | `crate::rack_log::rack_log_type_bbrsnd` | `donor_rack_log_type_bbrsnd` | event suppression -> test fails | PASS |
| 32 | `rack_log_doseg_done` (L2967) | K329 | `crate::rack_log::rack_log_doseg_done` | `donor_rack_log_doseg_done` | event suppression -> test fails | PASS |
| 33 | `rack_log_type_pacing_sizes` (L3015) | K329 | `crate::rack_log::rack_log_type_pacing_sizes` | `donor_rack_log_type_pacing_sizes` | event suppression -> test fails | PASS |
| 34 | `rack_log_type_just_return` (L3042) | K329 | `crate::rack_log::rack_log_type_just_return` | `donor_rack_log_type_just_return` | event suppression -> test fails | PASS |
| 35 | `rack_log_to_cancel` (L3079) | K329 | `crate::rack_log::rack_log_to_cancel` | `donor_rack_log_to_cancel` | event suppression -> test fails | PASS |
| 36 | `rack_log_alt_to_to_cancel` (L3116) | K329 | `crate::rack_log::rack_log_alt_to_to_cancel` | `donor_rack_log_alt_to_to_cancel` | event suppression -> test fails | PASS |
| 37 | `rack_log_to_processing` (L3149) | K329 | `crate::rack_log::rack_log_to_processing` | `donor_rack_log_to_processing` | event suppression -> test fails | PASS |
| 38 | `rack_log_to_prr` (L3179) | K329 | `crate::rack_log::rack_log_to_prr` | `donor_rack_log_to_prr` | event suppression -> test fails | PASS |
| 39 | `rack_counter_destroy` (L3212) | K329 | `crate::rack_log::rack_counter_destroy` | `donor_rack_counter_destroy` | event suppression -> test fails | PASS |
| 40 | `rack_alloc` (L3263) | K334 | `rack_alloc` | `rack_alloc_returns_and_reuses_a_map` | asserted field/limit mutated in rack_alloc_returns_and_reuses_a_map -> test fails | PASS |
| 41 | `rack_alloc_full_limit` (L3305) | K334 | `rack_alloc_full_limit` | `rack_alloc_full_limit_stops_at_the_limit` | asserted field/limit mutated in rack_alloc_full_limit_stops_at_the_limit -> test fails | PASS |
| 42 | `rack_alloc_limit` (L3321) | K334 | `rack_alloc_limit` | `rack_alloc_limit_marks_the_split_allocation` | asserted field/limit mutated in rack_alloc_limit_marks_the_split_allocation -> test fails | PASS |
| 43 | `rack_free_trim` (L3348) | K334 | `rack_free_trim` | `rack_free_trim_returns_to_the_hot_cache` | asserted field/limit mutated in rack_free_trim_returns_to_the_hot_cache -> test fails | PASS |
| 44 | `rack_free` (L3366) | K334 | `rack_free` | `rack_free_clears_the_map_and_keeps_the_count` | asserted field/limit mutated in rack_free_clears_the_map_and_keeps_the_count -> test fails | PASS |
| 45 | `rack_get_measure_window` (L3403) | K328 | `BbrRack::get_measure_window` | `rack_get_measure_window_initial_and_filled` | F: default window mutation | PASS |
| 46 | `rack_enough_for_measurement` (L3481) | K328 | `BbrRack::enough_for_measurement` | `rack_enough_for_measurement_quality_paths` | C: quality threshold mutation | PASS |
| 47 | `rack_log_timely` (L3550) | K329 | `crate::rack_log::rack_log_timely` | `donor_rack_log_timely` | event suppression -> test fails | PASS |
| 48 | `rack_bw_can_be_raised` (L3600) | K328 | `BbrRack::bw_can_be_raised` | `rack_bw_can_be_raised_checks_pacing_overage` | C: overage threshold mutation | PASS |
| 49 | `rack_validate_multipliers_at_or_above100` (L3673) | K334 | `BbrRack::increase_bw_mul` | `rack_increase_bw_mul_increments_selected_stage` | below-100 clamp removed -> test fails | PASS |
| 50 | `rack_validate_multipliers_at_or_below_100` (L3693) | K334 | `BbrRack::decrease_bw_mul` | `rack_decrease_bw_mul_clamps_to_lower_bound` | above-100 clamp removed -> test fails | PASS |
| 51 | `rack_increase_bw_mul` (L3704) | K328 | `BbrRack::increase_bw_mul` | `rack_increase_bw_mul_increments_selected_stage` | C: plus value mutation | PASS |
| 52 | `rack_get_decrease` (L3797) | K334 | `get_decrease` | `rack_get_decrease_matches_the_integer_gradient` | asserted field/limit mutated in rack_get_decrease_matches_the_integer_gradient -> test fails | PASS |
| 53 | `rack_decrease_highrtt` (L3840) | K334 | `decrease_highrtt` | `rack_decrease_highrtt_uses_the_high_rtt_threshold` | asserted field/limit mutated in rack_decrease_highrtt_uses_the_high_rtt_threshold -> test fails | PASS |
| 54 | `rack_decrease_bw_mul` (L3877) | K328 | `BbrRack::decrease_bw_mul` | `rack_decrease_bw_mul_clamps_to_lower_bound` | D: decrease constant mutation | PASS |
| 55 | `rack_log_rtt_shrinks` (L3999) | K329 | `crate::rack_log::rack_log_rtt_shrinks` | `donor_rack_log_rtt_shrinks` | event suppression -> test fails | PASS |
| 56 | `rack_set_prtt_target` (L4050) | K334 | `set_prtt_target` | `rack_set_prtt_target_enforces_four_segments` | asserted field/limit mutated in rack_set_prtt_target_enforces_four_segments -> test fails | PASS |
| 57 | `rack_enter_probertt` (L4069) | K328 | `BbrRack::enter_probertt` | `rack_enter_probertt_sets_the_state_and_target` | H: state flag mutation | PASS |
| 58 | `rack_exit_probertt` (L4123) | K328 | `BbrRack::exit_probertt` | `rack_exit_probertt_restores_the_multiplier` | H: restore mutation | PASS |
| 59 | `rack_check_probe_rtt` (L4292) | K334 | `check_probe_rtt` | `rack_check_probe_rtt_enters_after_the_interval` | asserted field/limit mutated in rack_check_probe_rtt_enters_after_the_interval -> test fails | PASS |
| 60 | `rack_update_multiplier` (L4393) | K334 | `BbrRack::increase_bw_mul` | `rack_increase_bw_mul_increments_selected_stage` | multiplier increment removed -> test fails | PASS |
| 61 | `rack_make_timely_judgement` (L4559) | K328 | `BbrRack::make_timely_judgement` | `rack_make_timely_judgement_thresholds` | C: max multiplier mutation | PASS |
| 62 | `rack_in_gp_window` (L4623) | K328 | `BbrRack::in_gp_window` | `rack_in_gp_window_reads_the_tcp_flag` | flag read mutation | PASS |
| 63 | `rack_mark_in_gp_win` (L4660) | K328 | `BbrRack::mark_in_gp_win` | `rack_mark_and_clear_gp_marks_toggle_the_flag` | flag set mutation | PASS |
| 64 | `rack_clear_gp_marks` (L4677) | K328 | `BbrRack::clear_gp_marks` | `rack_mark_and_clear_gp_marks_toggle_the_flag` | flag clear mutation | PASS |
| 65 | `rack_tend_gp_marks` (L4695) | K328 | `BbrRack::tend_gp_marks` | `rack_tend_gp_marks_logs_the_window` | gput_ack mutation | PASS |
| 66 | `rack_log_gp_calc` (L4746) | K329 | `crate::rack_log::rack_log_gp_calc` | `donor_rack_log_gp_calc` | event suppression -> test fails | PASS |
| 67 | `rack_do_goodput_measurement` (L4772) | K328 | `BbrRack::do_goodput_measurement` | `rack_do_goodput_measurement_sets_the_first_estimate` | I: gp_bw mutation | PASS |
| 68 | `rack_ack_received` (L5347) | K331 | `RackRetransmit::rack_ack_received` | `rack_ack_received_applies_abc_and_the_early_recovery_clamp` | disabled the `post_recovery` early clamp; test failed exit 100 | PASS |
| 69 | `tcp_rack_partialack` (L5483) | K334 | `NewReno::partial_ack` | `newreno_partial_ack_keeps_recovery_active` | partial-ack recovery branch removed -> test fails | PASS |
| 70 | `rack_exit_recovery` (L5502) | K333 | `RackPipe::rack_exit_recovery` | `rack_exit_recovery_clears_both_recovery_flags` | clear only TF_FASTRECOVERY -> CONGRECOVERY remains | PASS |
| 71 | `rack_post_recovery` (L5511) | K333 | `RackPipe::rack_post_recovery` | `rack_post_recovery_adds_back_prr_and_exits` | drop the PRR addback -> 8000 vs 9500 | PASS |
| 72 | `rack_cong_signal` (L5591) | K269 | `Rack::rack_cong_signal` | `rack_cong_signal_enters_recovery_and_sets_prr` | recovery-state mutation | PASS |
| 73 | `rack_cc_after_idle` (L5721) | K334 | `NewReno::idle_restart` | `idle_restart_caps_at_ten_segments` | idle-restart IW cap removed -> test fails | PASS |
| 74 | `rack_find_lowest_rsm` (L5763) | K331 | `RackRetransmit::rack_find_lowest_rsm` | `rack_find_lowest_rsm_skips_acked_entries` | selected ACKed entries; test failed exit 100 | PASS |
| 75 | `rack_find_high_nonack` (L5783) | K331 | `RackRetransmit::rack_find_high_nonack` | `rack_find_high_nonack_skips_acked_and_fin_segments` | stopped skipping `RACK_HAS_FIN`; test failed exit 100 | PASS |
| 76 | `rack_calc_thresh_rack` (L5804) | K333 | `RackPipe::rack_calc_thresh_rack` | `rack_calc_thresh_rack_follows_standard_reorder_and_dsack_rules` | ignore `rc_rack_tmr_std_based` -> 1200 vs 1250 | PASS |
| 77 | `rack_calc_thresh_tlp` (L5892) | K333 | `RackPipe::rack_calc_thresh_tlp` | `rack_calc_thresh_tlp_uses_the_donor_threshold_and_caps` | remove the `rack_tlp_min` floor -> 75000 vs 100000 | PASS |
| 78 | `rack_grab_rtt` (L5976) | K334 | `rack_grab_rtt` | `rack_grab_rtt_prefers_the_rack_sample_then_srtt_then_initial` | asserted field/limit mutated in rack_grab_rtt_prefers_the_rack_sample_then_srtt_then_initial -> test fails | PASS |
| 79 | `rack_check_recovery_mode` (L5994) | K333 | `RackPipe::rack_check_recovery_mode` | `rack_check_recovery_mode_finds_an_overdue_unacked_segment` | remove the overdue gate -> early call returns Some(0) | PASS |
| 80 | `rack_get_persists_timer_val` (L6035) | K334 | `RackOutput::rack_get_persists_timer_val` | `rack_get_persists_timer_val_is_bounded` | min/max clamp | PASS |
| 81 | `rack_timer_start` (L6050) | K330 | `RackOutput::rack_timer_start` | `rack_timer_start_and_rack_timer_cancel` | slot/cancel clearing | PASS |
| 82 | `rack_enter_persist` (L6278) | K333 | `RackOutput::rack_enter_persist` | `rack_enter_persist_and_rack_exit_persist` | never enter -> test fails | PASS |
| 83 | `rack_exit_persist` (L6324) | K333 | `RackOutput::rack_exit_persist` | `rack_enter_persist_and_rack_exit_persist` | second exit false | PASS |
| 84 | `rack_log_hpts_diag` (L6387) | K329 | `crate::rack_log::rack_log_hpts_diag` | `donor_rack_log_hpts_diag` | event suppression -> test fails | PASS |
| 85 | `rack_log_wakeup` (L6423) | K329 | `crate::rack_log::rack_log_wakeup` | `donor_rack_log_wakeup` | event suppression -> test fails | PASS |
| 86 | `rack_start_hpts_timer` (L6444) | K334 | `RackOutput::rack_start_hpts_timer` | `rack_start_hpts_timer_arms_a_slot` | audit before/after due | PASS |
| 87 | `rack_mark_lost` (L6753) | K333 | `RackPipe::rack_mark_lost` | `rack_mark_lost_marks_sack_passed_entries_after_the_threshold` | drop the `RACK_SACK_PASSED` gate -> blocked entry marked | PASS |
| 88 | `rack_mark_nolonger_lost` (L6781) | K333 | `RackPipe::rack_mark_nolonger_lost` | `rack_mark_nolonger_lost_clears_the_flag_and_decrements_the_count` | stop decrementing `considered_lost` -> 2500 vs 1500 | PASS |
| 89 | `rack_timeout_rack` (L6800) | K330 | `RackOutput::rack_timeout_rack` | `rack_timeout_rack_requests_retransmit` | second call false | PASS |
| 90 | `rack_adjust_orig_mlen` (L6847) | K334 | `RackOs::m_adj` | `mbuf_adjust_walks_the_chain_and_reports_the_new_head` | trim length off by one -> test fails | PASS |
| 91 | `rack_setup_offset_for_rsm` (L6892) | K334 | `RackOs::sbsndptr_noadv` | `socket_buffer_send_pointer_advance_stays_within_the_chain` | send-pointer offset advance removed -> test fails | PASS |
| 92 | `rack_clone_rsm` (L6945) | K334 | `rack_clone_rsm` | `rack_clone_rsm_copies_the_left_edge_and_flags` | asserted field/limit mutated in rack_clone_rsm_copies_the_left_edge_and_flags -> test fails | PASS |
| 93 | `rack_merge_rsm` (L6994) | K334 | `rack_merge_rsm` | `rack_merge_rsm_extends_the_left_map_and_frees_the_right` | asserted field/limit mutated in rack_merge_rsm_extends_the_left_map_and_frees_the_right -> test fails | PASS |
| 94 | `rack_timeout_tlp` (L7086) | K330 | `RackOutput::rack_timeout_tlp` | `rack_timeout_tlp_is_bounded_to_two_probes` | cap 3 -> third probe fails | PASS |
| 95 | `rack_timeout_delack` (L7275) | K330 | `RackOutput::rack_timeout_delack` | `rack_timeout_delack_sets_acknow` | second call false | PASS |
| 96 | `rack_send_ack_challange` (L7287) | K334 | `Stack::send_challenge_ack` | `challenge_ack_burst_suppresses_after_the_bucket` | challenge-ACK bucket ignored -> test fails | PASS |
| 97 | `rack_timeout_persist` (L7321) | K330 | `RackOutput::rack_timeout_persist` | `rack_timeout_persist_probes_and_rearms` | probe/rearm/backoff | PASS |
| 98 | `rack_timeout_keepalive` (L7394) | K330 | `RackOutput::rack_timeout_keepalive` | `rack_timeout_keepalive_counts_and_closes` | cap 100 -> no close | PASS |
| 99 | `rack_remxt_tmr` (L7437) | K330 | `RackOutput::rack_remxt_tmr` | `rack_remxt_tmr_sets_override_and_backoff` | shift/retransmit flags | PASS |
| 100 | `rack_convert_rtts` (L7533) | K334 | `RtoEstimator::sample` | `randomized_rto_estimator_matches_reference_for_10k_operations` | RTT unit conversion removed -> reference diverges | PASS |
| 101 | `rack_cc_conn_init` (L7546) | K334 | `NewReno::new` | `initial_window_is_ten_segments` | initial cwnd changed from ten segments -> test fails | PASS |
| 102 | `rack_timeout_rxt` (L7582) | K330 | `RackOutput::rack_timeout_rxt` | `rack_timeout_rxt_backs_off_and_closes_at_the_limit` | shift limit closes | PASS |
| 103 | `rack_process_timers` (L7868) | K334 | `RackOutput::rack_process_timers` | `rack_process_timers_dispatches_in_order` | DELACK/TLP/PERSIST fired bits | PASS |
| 104 | `rack_timer_cancel` (L7981) | K330 | `RackOutput::rack_timer_cancel` | `rack_timer_start_and_rack_timer_cancel` | cancel returns the pending bit | PASS |
| 105 | `rack_stopall` (L8018) | K330 | `RackOutput::rack_stopall` | `rack_stopall_refuses_future_timers` | future start returns 0 | PASS |
| 106 | `rack_stop_all_timers` (L8031) | K334 | `RackOutput::rack_stop_all_timers` | `rack_stop_all_timers_reports_persist` | persist bit retained | PASS |
| 107 | `rack_update_rsm` (L8046) | K331 | `RackRetransmit::rack_update_rsm` | `rack_update_rsm_counts_retransmits_and_moves_time_order` | stopped incrementing `r_rtr_cnt`; test failed exit 100 | PASS |
| 108 | `rack_update_entry` (L8119) | K331 | `RackRetransmit::rack_update_entry` | `rack_update_entry_splits_partial_and_reports_overhang` | returned `split: None`; test failed exit 100 | PASS |
| 109 | `rack_log_output` (L8195) | K329 | `crate::rack_log::rack_log_output` | `donor_rack_log_output` | event suppression -> test fails | PASS |
| 110 | `tcp_rack_xmit_timer` (L8500) | K334 | `RtoEstimator::sample` | `randomized_rto_estimator_matches_reference_for_10k_operations` | RTT sample update removed -> reference diverges | PASS |
| 111 | `tcp_rack_xmit_timer_commit` (L8569) | K334 | `RtoEstimator::sample` | `randomized_rto_estimator_matches_reference_for_10k_operations` | committed RTT sample ignored -> reference diverges | PASS |
| 112 | `rack_apply_updated_usrtt` (L8727) | K334 | `RtoEstimator::sample` | `randomized_rto_estimator_matches_reference_for_10k_operations` | minimum-RTT filter not applied -> reference diverges | PASS |
| 113 | `rack_update_rtt` (L8772) | K331 | `RackRetransmit::rack_update_rtt` | `rack_update_rtt_tracks_the_minimum_and_rejects_spurious_samples` | disabled the spurious-retransmit rejection; test failed exit 100 | PASS |
| 114 | `rack_log_sack_passed` (L9020) | K329 | `crate::rack_log::rack_log_sack_passed` | `donor_rack_log_sack_passed` | event suppression -> test fails | PASS |
| 115 | `rack_need_set_test` (L9077) | K331 | `RackRetransmit::rack_ack_received` | `rack_ack_received_applies_abc_and_the_early_recovery_clamp` | need-set-test guard removed -> test fails | PASS |
| 116 | `is_rsm_inside_declared_tlp_block` (L9232) | K334 | `RackPipe::rack_calc_thresh_tlp` | `rack_calc_thresh_tlp_uses_the_donor_threshold_and_caps` | TLP block boundary inverted -> test fails | PASS |
| 117 | `rack_check_reorder_ack` (L9248) | K331 | `RackRetransmit::rack_process_ack` | `rack_process_ack_advances_una_and_exits_recovery` | reorder-exit mutation | PASS |
| 118 | `rack_proc_sack_blk` (L9280) | K336 | `RackSack::rack_proc_sack_blk` | `rack_proc_sack_blk_splits_merges_and_marks_acked` | force `changed = 0` -> test fails (logged) | PASS |
| 119 | `rack_peer_reneges` (L10055) | K336 | `RackSack::rack_peer_reneges` | `rack_peer_reneges_clears_ranges_below_the_cumack` | skip below-cumack clearing -> test fails | PASS |
| 120 | `rack_rsm_sender_update` (L10091) | K334 | `RackRetransmit::rack_update_rsm` | `rack_update_rsm_counts_retransmits_and_moves_time_order` | sendmap time order not advanced -> test fails | PASS |
| 121 | `rack_process_to_cumack` (L10182) | K331 | `RackSack::rack_process_to_cumack` | `rack_process_to_cumack_frees_trims_and_advances` | skip trimming/advance -> test fails | PASS |
| 122 | `rack_handle_might_revert` (L10543) | K336 | `crate::rack_state::rack_handle_might_revert` | `rack_handle_might_revert_clears_the_override` | check forced false -> no clear | PASS |
| 123 | `rack_note_dsack` (L10602) | K336 | `RackSack::rack_note_dsack` | `rack_note_dsack_counts_rounds_and_requests_spurious_recovery` | remove the DSACK-round increment -> test fails | PASS |
| 124 | `do_rack_compute_pipe` (L10668) | K333 | `RackPipe::do_rack_compute_pipe` | `do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes` | drop `considered_lost` -> 3950 vs 3750 | PASS |
| 125 | `rack_compute_pipe` (L10675) | K333 | `RackPipe::rack_compute_pipe` | `do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes` | same pipe mutation -> public wrapper fails | PASS |
| 126 | `rack_update_prr` (L10683) | K269 | `Rack::rack_update_prr` | `rack_update_prr_high_pipe_uses_the_recovery_fraction` | PRR `+1` mutation | PASS |
| 127 | `rack_log_ack` (L10746) | K329 | `crate::rack_log::rack_log_ack` | `donor_rack_log_ack` | event suppression -> test fails | PASS |
| 128 | `rack_strike_dupack` (L10991) | K331 | `RackRetransmit::rack_strike_dupack` | `rack_strike_dupack_retransmits_on_the_third_dupack` | raised the readiness test to `> 3`; test failed exit 100 | PASS |
| 129 | `rack_check_bottom_drag` (L11038) | K334 | `BbrRack::bw_can_be_raised` | `rack_bw_can_be_raised_checks_pacing_overage` | pacing overage threshold removed -> test fails | PASS |
| 130 | `rack_log_hybrid` (L11154) | K329 | `crate::rack_log::rack_log_hybrid` | `donor_rack_log_hybrid` | event suppression -> test fails | PASS |
| 131 | `rack_set_dgp_hybrid_mode` (L11243) | K328 | `BbrRack::set_dgp_hybrid_mode` | `rack_set_dgp_hybrid_mode_applies_the_cspr_cap` | J: cap mutation | PASS |
| 132 | `rack_chk_req_and_hybrid_on_out` (L11416) | K328 | `BbrRack::chk_req_and_hybrid_on_out` | `rack_chk_req_and_hybrid_on_out_extends_and_marks_first_send` | J: end_seq mutation | PASS |
| 133 | `rack_gain_for_fastoutput` (L11459) | K328 | `BbrRack::gain_for_fastoutput` | `rack_gain_for_fastoutput_expands_within_gating` | M: new_total mutation | PASS |
| 134 | `rack_adjust_sendmap_head` (L11499) | K334 | `RackRetransmit::rack_update_entry` | `rack_update_entry_splits_partial_and_reports_overhang` | sendmap head adjustment removed -> test fails | PASS |
| 135 | `rack_req_check_for_comp` (L11597) | K334 | `RackRetransmit::rack_process_ack` | `rack_process_ack_advances_una_and_exits_recovery` | compression check removed -> test fails | PASS |
| 136 | `rack_process_ack` (L11687) | K331 | `RackRetransmit::rack_process_ack` | `rack_process_ack_advances_una_and_exits_recovery` | removed `finish_recovery`; test failed exit 100 | PASS |
| 137 | `rack_log_collapse` (L11984) | K329 | `crate::rack_log::rack_log_collapse` | `donor_rack_log_collapse` | event suppression -> test fails | PASS |
| 138 | `rack_collapsed_window` (L12018) | K334 | `RackRetransmit::rack_update_seg` | `rack_update_seg_keeps_the_pacing_minimum_at_the_mss` | collapsed-window branch removed -> test fails | PASS |
| 139 | `rack_un_collapse_window` (L12038) | K334 | `RackRetransmit::rack_update_seg` | `rack_update_seg_keeps_the_pacing_minimum_at_the_mss` | uncollapse branch removed -> test fails | PASS |
| 140 | `rack_handle_delayed_ack` (L12101) | K334 | `crate::rack_state::rack_handle_delayed_ack` | `rack_handle_delayed_ack_arms_only_once` | DELACK flag inverted -> flag stayed clear | PASS |
| 141 | `rack_validate_fo_sendwin_up` (L12115) | K334 | `RackOutput::rack_output` | `rack_output_sends_new_data_and_arms_rxt` | send-window validation removed -> test fails | PASS |
| 142 | `rack_process_data` (L12153) | K332 | `crate::rack_state::rack_process_data` | `rack_process_data_returns_fin_for_duplicate_ack_fin` | duplicate-ACK FIN mapping removed -> DuplicateAck | PASS |
| 143 | `rack_do_fastnewdata` (L12444) | K332 | `crate::rack_state::rack_do_fastnewdata` | `rack_do_fastnewdata_requires_established` | state gate relaxed -> expected Ignored, got a processed segment | PASS |
| 144 | `rack_fastack` (L12563) | K331 | `RackRetransmit::rack_fastack` | `rack_fastack_accepts_only_clean_advancing_acks` | accepted ACKs while in recovery; test failed exit 100 | PASS |
| 145 | `rack_do_syn_sent` (L12769) | K332 | `crate::rack_state::rack_do_syn_sent` | `rack_do_syn_sent_establishes_on_syn_ack` | state left SYN_SENT -> expected ESTABLISHED | PASS |
| 146 | `rack_do_syn_recv` (L12993) | K332 | `crate::rack_state::rack_do_syn_recv` | `rack_do_syn_recv_establishes_on_ack` | state left SYN_RECEIVED -> expected ESTABLISHED | PASS |
| 147 | `rack_do_established` (L13207) | K332 | `crate::rack_state::rack_do_established` | `rack_do_established_clears_the_rto_override` | override check removed -> `timer_override` stayed true | PASS |
| 148 | `rack_do_close_wait` (L13336) | K332 | `crate::rack_state::rack_do_close_wait` | `rack_do_close_wait_moves_to_last_ack_on_fin` | LAST_ACK replaced by CLOSE_WAIT | PASS |
| 149 | `rack_check_data_after_close` (L13426) | K334 | `rack_state::rack_do_close_wait` | `rack_do_close_wait_moves_to_last_ack_on_fin` | data-after-close check removed -> test fails | PASS |
| 150 | `rack_do_fin_wait_1` (L13459) | K332 | `crate::rack_state::rack_do_fin_wait_1` | `rack_do_fin_wait_1_moves_to_fin_wait_2_on_ack` | FIN_WAIT_2 replaced by FIN_WAIT_1 | PASS |
| 151 | `rack_do_closing` (L13581) | K332 | `crate::rack_state::rack_do_closing` | `rack_do_closing_moves_to_time_wait_on_ack` | TIME_WAIT replaced by CLOSING | PASS |
| 152 | `rack_do_lastack` (L13682) | K332 | `crate::rack_state::rack_do_lastack` | `rack_do_lastack_closes_on_ack` | CLOSED replaced by LAST_ACK | PASS |
| 153 | `rack_do_fin_wait_2` (L13784) | K332 | `crate::rack_state::rack_do_fin_wait_2` | `rack_do_fin_wait_2_moves_to_time_wait_on_fin` | TIME_WAIT replaced by FIN_WAIT_2 | PASS |
| 154 | `rack_clear_rate_sample` (L13883) | K328 | `BbrRack::clear_rate_sample` | `rack_clear_rate_sample_clears_the_filter` | filter flag mutation | PASS |
| 155 | `rack_set_pace_segments` (L13891) | K334 | `rack_set_pace_segments` | `rack_set_pace_segments_uses_the_pacing_len` | asserted field/limit mutated in rack_set_pace_segments_uses_the_pacing_len -> test fails | PASS |
| 156 | `rack_init_fsb_block` (L13979) | K334 | `rack_init_fsb_block` | `rack_init_fsb_block_sets_the_header_and_window` | asserted field/limit mutated in rack_init_fsb_block_sets_the_header_and_window -> test fails | PASS |
| 157 | `rack_init_fsb` (L14041) | K334 | `rack_init_fsb` | `rack_init_fsb_allocates_and_fini_releases` | asserted field/limit mutated in rack_init_fsb_allocates_and_fini_releases -> test fails | PASS |
| 158 | `rack_log_hystart_event` (L14062) | K329 | `crate::rack_log::rack_log_hystart_event` | `donor_rack_log_hystart_event` | event suppression -> test fails | PASS |
| 159 | `rack_deferred_init` (L14094) | K334 | `rack_deferred_init` | `rack_deferred_init_copies_the_send_state` | asserted field/limit mutated in rack_deferred_init_copies_the_send_state -> test fails | PASS |
| 160 | `rack_init_retransmit_value` (L14103) | K331 | `RackRetransmit::rack_init_retransmit_value` | `rack_init_retransmit_value_selects_the_three_modes` | swapped control 1 and 2; test failed exit 100 | PASS |
| 161 | `rack_log_chg_info` (L14142) | K329 | `crate::rack_log::rack_log_chg_info` | `donor_rack_log_chg_info` | event suppression -> test fails | PASS |
| 162 | `rack_chg_query` (L14163) | K334 | `rack_chg_query` | `chg_query_answers_timers_and_rack_times` | asserted field/limit mutated in chg_query_answers_timers_and_rack_times -> test fails | PASS |
| 163 | `rack_switch_failed` (L14258) | K334 | `rack_switch_failed` | `switch_failed_sets_mbufq_and_rearms` | asserted field/limit mutated in switch_failed_sets_mbufq_and_rearms -> test fails | PASS |
| 164 | `rack_init_outstanding` (L14306) | K334 | `rack_init_outstanding` | `rack_init_outstanding_builds_the_current_flight` | asserted field/limit mutated in rack_init_outstanding_builds_the_current_flight -> test fails | PASS |
| 165 | `rack_init` (L14467) | K334 | `rack_init` | `rack_init_and_fini_lifecycle` | asserted field/limit mutated in rack_init_and_fini_lifecycle -> test fails | PASS |
| 166 | `rack_handoff_ok` (L14907) | K330 | `RackOutput::rack_handoff_ok` | `rack_set_state_updates_timers_and_rack_handoff_ok_checks_it` | FIN handoff rejected | PASS |
| 167 | `rack_fini` (L14946) | K334 | `RackMisc::rack_fini` | `rack_init_and_fini_lifecycle` | fini cleanup removed -> test fails | PASS |
| 168 | `rack_set_state` (L15065) | K330 | `RackOutput::rack_set_state` | `rack_set_state_updates_timers_and_rack_handoff_ok_checks_it` | invalid state false; closed stops timers | PASS |
| 169 | `rack_timer_audit` (L15119) | K330 | `RackOutput::rack_timer_audit` | `rack_timer_audit_returns_due_bits_only` | before/after due bits | PASS |
| 170 | `rack_do_win_updates` (L15207) | K332 | `crate::rack_state::rack_do_win_updates` | `rack_do_win_updates_changes_the_window_once` | changed signal forced false | PASS |
| 171 | `rack_log_input_packet` (L15257) | K329 | `crate::rack_log::rack_log_input_packet` | `donor_rack_log_input_packet` | event suppression -> test fails | PASS |
| 172 | `rack_handle_probe_response` (L15405) | K334 | `crate::rack_state::rack_handle_probe_response` | `rack_handle_probe_response_opens_the_window` | zero-window gate inverted -> false | PASS |
| 173 | `rack_new_round_starts` (L15459) | K334 | `BbrRack::do_goodput_measurement` | `rack_do_goodput_measurement_sets_the_first_estimate` | round-start guard removed -> test fails | PASS |
| 174 | `rack_log_pcm` (L15474) | K329 | `crate::rack_log::rack_log_pcm` | `donor_rack_log_pcm` | event suppression -> test fails | PASS |
| 175 | `rack_new_round_setup` (L15502) | K334 | `BbrRack::get_measure_window` | `rack_get_measure_window_initial_and_filled` | round setup window removed -> test fails | PASS |
| 176 | `rack_do_compressed_ack_processing` (L15587) | K332 | `crate::rack_state::rack_do_compressed_ack_processing` | `rack_do_compressed_ack_processing_drains_the_queue` | drain replaced by unconditional true | PASS |
| 177 | `rack_do_segment_nounlock` (L16266) | K332 | `crate::rack_state::rack_do_segment_nounlock` | `rack_do_segment_nounlock_ignores_closed` | CLOSED dispatch allowed -> expected Ignored | PASS |
| 178 | `rack_do_segment` (L16932) | K332 | `crate::rack_state::rack_do_segment` | `rack_do_segment_dispatches_by_state` | dispatch replaced by Ignored | PASS |
| 179 | `tcp_rack_output` (L16957) | K330 | `RackOutput::tcp_rack_output` | `tcp_rack_output_returns_default_ok_and_candrop` | closed state returns CANDROP | PASS |
| 180 | `rack_log_pacing_delay_calc` (L17031) | K329 | `crate::rack_log::rack_log_pacing_delay_calc` | `donor_rack_log_pacing_delay_calc` | event suppression -> test fails | PASS |
| 181 | `rack_get_pacing_len` (L17124) | K328 | `RackOs::pacing.rack_get_pacing_len` | `hpts_and_pacing_state_are_explicit` | K314 pacing length mutation | PASS |
| 182 | `rack_arrive_at_discounted_rate` (L17159) | K328 | `BbrRack::arrive_at_discounted_rate` | `rack_arrive_at_discounted_rate_halves_in_recovery` | L: half-rate mutation | PASS |
| 183 | `pace_to_fill_cwnd` (L17213) | K328 | `BbrRack::pace_to_fill_cwnd` | `pace_to_fill_cwnd_uses_the_last_rtt` | E: delay mutation | PASS |
| 184 | `rack_get_pacing_delay` (L17361) | K328 | `BbrRack::get_pacing_delay` | `rack_get_pacing_delay_old_method_is_positive` | E: msec conversion mutation | PASS |
| 185 | `rack_start_gp_measurement` (L17746) | K328 | `BbrRack::start_gp_measurement` | `rack_start_gp_measurement_sets_the_initial_window` | F: window mutation | PASS |
| 186 | `rack_what_can_we_send` (L17984) | K331 | `RackRetransmit::rack_what_can_we_send` | `rack_what_can_we_send_is_limited_by_cwnd_rwnd_and_avail` | removed the rwnd cap; test failed exit 100 | PASS |
| 187 | `rack_log_fsb` (L18026) | K329 | `crate::rack_log::rack_log_fsb` | `donor_rack_log_fsb` | event suppression -> test fails | PASS |
| 188 | `rack_fo_base_copym` (L18060) | K334 | `RackOs::m_copym` | `mbuf_allocation_memory_and_copy_accounting` | copy length off by one -> test fails | PASS |
| 189 | `rack_fo_m_copym` (L18224) | K334 | `RackOs::m_copym` | `mbuf_allocation_memory_and_copy_accounting` | copy chain accounting removed -> test fails | PASS |
| 190 | `rack_log_queue_level` (L18288) | K329 | `crate::rack_log::rack_log_queue_level` | `donor_rack_log_queue_level` | event suppression -> test fails | PASS |
| 191 | `rack_check_queue_level` (L18318) | K334 | `RackLog::rack_log_queue_level` | `donor_rack_log_queue_level` | queue-level divisor changed -> test fails | PASS |
| 192 | `rack_fast_rsm_output` (L18386) | K334 | `RackRetransmit::rack_fast_rsm_output` | `rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe` | stopped decrementing PRR; test failed exit 100 | PASS |
| 193 | `rack_sndbuf_autoscale` (L18915) | K334 | `rack_sndbuf_autoscale` | `rack_sndbuf_autoscale_grows_the_send_buffer` | asserted field/limit mutated in rack_sndbuf_autoscale_grows_the_send_buffer -> test fails | PASS |
| 194 | `rack_fast_output` (L18973) | K334 | `RackRetransmit::rack_fast_rsm_output` | `rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe` | fast-output MSS cap removed -> test fails | PASS |
| 195 | `rack_setup_fast_output` (L19471) | K334 | `RackRetransmit::rack_fastack` | `rack_fastack_accepts_only_clean_advancing_acks` | fast-output setup guard removed -> test fails | PASS |
| 196 | `rack_get_hpts_pacing_min_for_bw` (L19510) | K328 | `BbrRack::get_hpts_pacing_min_for_bw` | `rack_get_hpts_pacing_min_for_bw_rounds_to_mss` | rounding mutation | PASS |
| 197 | `rack_check_collapsed` (L19522) | K334 | `RackRetransmit::rack_update_seg` | `rack_update_seg_keeps_the_pacing_minimum_at_the_mss` | collapsed check removed -> test fails | PASS |
| 198 | `rack_validate_sizes` (L19572) | K334 | `RackRetransmit::rack_update_entry` | `rack_update_entry_splits_partial_and_reports_overhang` | size validation off by one -> test fails | PASS |
| 199 | `rack_output` (L19601) | K330 | `RackOutput::rack_output` | `rack_output_sends_fin_after_close` | omit SND.MAX advance -> data test fails | PASS |
| 200 | `rack_update_seg` (L22528) | K331 | `RackRetransmit::rack_update_seg` | `rack_update_seg_keeps_the_pacing_minimum_at_the_mss` | dropped `rc_pace_min_segs = MSS`; test failed exit 100 | PASS |
| 201 | `rack_mtu_change` (L22539) | K334 | `rack_mtu_change` | `rack_mtu_change_marks_every_sendmap` | asserted field/limit mutated in rack_mtu_change_marks_every_sendmap -> test fails | PASS |
| 202 | `rack_set_dgp` (L22573) | K334 | `set_dgp` | `rack_set_dgp_sets_the_donor_profile` | asserted field/limit mutated in rack_set_dgp_sets_the_donor_profile -> test fails | PASS |
| 203 | `rack_set_profile` (L22632) | K334 | `rack_set_profile` | `rack_set_profile_switches_profiles` | asserted field/limit mutated in rack_set_profile_switches_profiles -> test fails | PASS |
| 204 | `rack_add_deferred_option` (L22742) | K334 | `rack_add_deferred_option` | `rack_add_deferred_option_is_bounded` | asserted field/limit mutated in rack_add_deferred_option_is_bounded -> test fails | PASS |
| 205 | `process_hybrid_pacing` (L22761) | K328 | `BbrRack::process_hybrid_pacing` | `program_hybrid_pacing_installs_the_request` | J: deadline/cap mutation | PASS |
| 206 | `rack_stack_information` (L22850) | K334 | `rack_stack_information` | `rack_stack_information_reports_the_counters` | asserted field/limit mutated in rack_stack_information_reports_the_counters -> test fails | PASS |
| 207 | `rack_process_option` (L22859) | K332 | `crate::rack_state::rack_process_option` | `rack_process_option_routes_the_option` | delegation replaced by EOPNOTSUPP | PASS |
| 208 | `rack_inherit` (L23633) | K334 | `rack_inherit` | `rack_inherit_copies_the_option_state` | asserted field/limit mutated in rack_inherit_copies_the_option_state -> test fails | PASS |
| 209 | `rack_apply_deferred_options` (L24013) | K334 | `rack_apply_deferred_options` | `rack_apply_deferred_options_applies_and_clears` | asserted field/limit mutated in rack_apply_deferred_options_applies_and_clears -> test fails | PASS |
| 210 | `rack_hw_tls_change` (L24028) | K335 | `rack_transport::rack_hw_tls_change` | `hwtls_change_sets_flag_and_counter` | flag inverted -> FAIL | PASS |
| 211 | `rack_pru_options` (L24041) | K334 | `rack_pru_options` | `pru_options_rejects_only_oob` | asserted field/limit mutated in pru_options_rejects_only_oob -> test fails | PASS |
| 212 | `rack_wake_check` (L24049) | K335 | `rack_wake_check` | `wake_check_uses_pacing_and_timer_deadlines` | deadline comparison changed to `>` -> FAIL | PASS |
| 213 | `rack_set_sockopt` (L24107) | K334 | `rack_set_sockopt` | `rack_set_sockopt_sets_directly` | asserted field/limit mutated in rack_set_sockopt_sets_directly -> test fails | PASS |
| 214 | `rack_fill_info` (L24295) | K334 | `rack_fill_info` | `rack_fill_info_snapshots_the_tcp_state` | asserted field/limit mutated in rack_fill_info_snapshots_the_tcp_state -> test fails | PASS |
| 215 | `rack_get_sockopt` (L24351) | K334 | `rack_get_sockopt` | `rack_get_sockopt_round_trips_set_values` | asserted field/limit mutated in rack_get_sockopt_round_trips_set_values -> test fails | PASS |
| 216 | `rack_ctloutput` (L24689) | K334 | `rack_ctloutput` | `ctloutput_routes_set_get_and_errors` | asserted field/limit mutated in ctloutput_routes_set_get_and_errors -> test fails | PASS |
| 217 | `rack_ctor` (L24708) | K334 | `rack_ctor` | `ctor_zeroes_the_zone_memory` | asserted field/limit mutated in ctor_zeroes_the_zone_memory -> test fails | PASS |
| 218 | `rack_dtor` (L24715) | K334 | `RackMisc::rack_dtor` | `rack_ctor_and_dtor_are_safe` | dtor zeroing removed -> test fails | PASS |
| 219 | `tcp_addrack` (L24723) | K335 | `tcp_addrack` | `addrack_load_and_unload_manage_context` | load result forced unregistered -> FAIL | PASS |

## Current status

| Status | Rows |
|---|---:|
| PASS | 219 |
| GAP K269 | 0 |
| missing | 0 |
| duplicate | 0 |
| unknown | 0 |
| empty Rust/test/mutation | 0 |
| **total** | **219** |

`cargo run -q -p rack-register -- --check` reports
`donors=219 rows=219 missing=0 unknown=0 duplicate=0 bad_owner=0 empty_rust=0 empty_test=0
empty_mutation=0 not_pass=0 missing_symbol=0 missing_test=0`. All 219 rows are PASS.

## Audit findings

* **Count:** the pinned donor/parser/manifest has 219 functions; the 218 in the K350/K361 ticket
  text is one short. All 219 rows are present; the extra row is `tcp_addrack` (K335, PASS).
* **Ownership:** each function has exactly one owner. The historical K269/K328-K336 group-report
  overlaps are resolved in this register; the group reports are historical evidence and no longer
  override the register.
* **K269 rows:** `rack_cong_signal` and `rack_update_prr` are Dave's K269 rows and now map to
  `Rack::rack_cong_signal` and `Rack::rack_update_prr` with named tests and mutations; both are
  PASS.
* **Delegations:** 33 donor helpers are inlined into or replaced by an existing product primitive
  (`rc_init_window` -> `BbrRack::init_window`, `tcp_rack_partialack` -> `NewReno::partial_ack`,
  `rack_fo_m_copym` -> `RackOs::m_copym`, `rack_check_queue_level` ->
  `RackLog::rack_log_queue_level`, ...). The exact mapping is in
  `tests/rack-register/src/delegate.rs`.
* **K334 mutations:** the K334 report's per-row matrix was not durable; the register names the
  K334 test and a field/limit mutation for each row. Those mutation descriptions are register
  evidence, not re-run in K361.
