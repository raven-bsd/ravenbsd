<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# RACK donor-function union register

Maintained by K338. The register lists every top-level function in FreeBSD
`sys/netinet/tcp_stacks/rack.c` at
`133344405f532bd54ff3629a77d4c4e062f01fdb` (BSD-2-Clause), its owner, the Rust
mapping, the named test, the mutation/negative check and the current status.
No donor code was copied.

The pinned donor has **219** top-level functions. The K338 ticket says 218;
the parser and the pinned manifest both report 219, so the register keeps all
219 rows and records the one-row discrepancy rather than hiding a function.

Legend:

* `PASS` — merged mapping, named test and mutation check.
* `ASSIGNED Kxxx` — row is owned by a pending group ticket; mapping/test/
  mutation are gaps until that branch lands.

## Register

| # | Donor function (`rack.c` line) | Owner | Rust | Named test | Mutation/negative | Status |
|---:|---|---|---|---|---|---|
| 1 | `rack_get_lt_bw` (L579) | K328 | - | - | - | ASSIGNED K328 |
| 2 | `rack_swap_beta_values` (L599) | K334 | - | - | - | ASSIGNED K334 |
| 3 | `rack_set_cc_pacing` (L690) | K328 | - | - | - | ASSIGNED K328 |
| 4 | `rack_undo_cc_pacing` (L703) | K328 | - | - | - | ASSIGNED K328 |
| 5 | `rack_remove_pacing` (L716) | K328 | - | - | - | ASSIGNED K328 |
| 6 | `rack_log_gpset` (L733) | K329 | crate::rack_log::rack_log_gpset | donor_rack_log_gpset | event suppression -> test fails | PASS |
| 7 | `sysctl_rack_clear` (L770) | K329 | crate::rack_log::sysctl_rack_clear | donor_sysctl_rack_clear | event suppression -> test fails | PASS |
| 8 | `sysctl_rack_wma_divisor` (L846) | K329 | crate::rack_log::sysctl_rack_wma_divisor | donor_sysctl_rack_wma_divisor | event suppression -> test fails | PASS |
| 9 | `rack_init_sysctls` (L862) | K329 | crate::rack_log::rack_init_sysctls | donor_rack_init_sysctls | event suppression -> test fails | PASS |
| 10 | `rc_init_window` (L1873) | K334 | - | - | - | ASSIGNED K334 |
| 11 | `rack_get_fixed_pacing_bw` (L1880) | K328 | - | - | - | ASSIGNED K328 |
| 12 | `rack_log_hybrid_bw` (L1893) | K329 | crate::rack_log::rack_log_hybrid_bw | donor_rack_log_hybrid_bw | event suppression -> test fails | PASS |
| 13 | `rack_log_hybrid_sends` (L2015) | K329 | crate::rack_log::rack_log_hybrid_sends | donor_rack_log_hybrid_sends | event suppression -> test fails | PASS |
| 14 | `rack_compensate_for_linerate` (L2098) | K328 | - | - | - | ASSIGNED K328 |
| 15 | `rack_rate_cap_bw` (L2123) | K328 | - | - | - | ASSIGNED K328 |
| 16 | `rack_get_gp_est` (L2261) | K328 | - | - | - | ASSIGNED K328 |
| 17 | `rack_get_bw` (L2356) | K328 | - | - | - | ASSIGNED K328 |
| 18 | `rack_get_output_gain` (L2369) | K328 | - | - | - | ASSIGNED K328 |
| 19 | `rack_log_dsack_event` (L2405) | K329 | crate::rack_log::rack_log_dsack_event | donor_rack_log_dsack_event | event suppression -> test fails | PASS |
| 20 | `rack_log_hdwr_pacing` (L2447) | K329 | crate::rack_log::rack_log_hdwr_pacing | donor_rack_log_hdwr_pacing | event suppression -> test fails | PASS |
| 21 | `rack_get_output_bw` (L2497) | K328 | - | - | - | ASSIGNED K328 |
| 22 | `rack_log_retran_reason` (L2558) | K329 | crate::rack_log::rack_log_retran_reason | donor_rack_log_retran_reason | event suppression -> test fails | PASS |
| 23 | `rack_log_to_start` (L2601) | K329 | crate::rack_log::rack_log_to_start | donor_rack_log_to_start | event suppression -> test fails | PASS |
| 24 | `rack_log_to_event` (L2644) | K329 | crate::rack_log::rack_log_to_event | donor_rack_log_to_event | event suppression -> test fails | PASS |
| 25 | `rack_log_map_chg` (L2684) | K329 | crate::rack_log::rack_log_map_chg | donor_rack_log_map_chg | event suppression -> test fails | PASS |
| 26 | `rack_log_rtt_upd` (L2735) | K329 | crate::rack_log::rack_log_rtt_upd | donor_rack_log_rtt_upd | event suppression -> test fails | PASS |
| 27 | `rack_log_rtt_sample` (L2804) | K329 | crate::rack_log::rack_log_rtt_sample | donor_rack_log_rtt_sample | event suppression -> test fails | PASS |
| 28 | `rack_log_rtt_sample_calc` (L2853) | K329 | crate::rack_log::rack_log_rtt_sample_calc | donor_rack_log_rtt_sample_calc | event suppression -> test fails | PASS |
| 29 | `rack_log_rtt_sendmap` (L2880) | K329 | crate::rack_log::rack_log_rtt_sendmap | donor_rack_log_rtt_sendmap | event suppression -> test fails | PASS |
| 30 | `rack_log_progress_event` (L2907) | K329 | crate::rack_log::rack_log_progress_event | donor_rack_log_progress_event | event suppression -> test fails | PASS |
| 31 | `rack_log_type_bbrsnd` (L2937) | K329 | crate::rack_log::rack_log_type_bbrsnd | donor_rack_log_type_bbrsnd | event suppression -> test fails | PASS |
| 32 | `rack_log_doseg_done` (L2967) | K329 | crate::rack_log::rack_log_doseg_done | donor_rack_log_doseg_done | event suppression -> test fails | PASS |
| 33 | `rack_log_type_pacing_sizes` (L3015) | K329 | crate::rack_log::rack_log_type_pacing_sizes | donor_rack_log_type_pacing_sizes | event suppression -> test fails | PASS |
| 34 | `rack_log_type_just_return` (L3043) | K329 | crate::rack_log::rack_log_type_just_return | donor_rack_log_type_just_return | event suppression -> test fails | PASS |
| 35 | `rack_log_to_cancel` (L3080) | K329 | crate::rack_log::rack_log_to_cancel | donor_rack_log_to_cancel | event suppression -> test fails | PASS |
| 36 | `rack_log_alt_to_to_cancel` (L3120) | K329 | crate::rack_log::rack_log_alt_to_to_cancel | donor_rack_log_alt_to_to_cancel | event suppression -> test fails | PASS |
| 37 | `rack_log_to_processing` (L3149) | K329 | crate::rack_log::rack_log_to_processing | donor_rack_log_to_processing | event suppression -> test fails | PASS |
| 38 | `rack_log_to_prr` (L3179) | K329 | crate::rack_log::rack_log_to_prr | donor_rack_log_to_prr | event suppression -> test fails | PASS |
| 39 | `rack_counter_destroy` (L3212) | K329 | crate::rack_log::rack_counter_destroy | donor_rack_counter_destroy | event suppression -> test fails | PASS |
| 40 | `rack_alloc` (L3263) | K334 | - | - | - | ASSIGNED K334 |
| 41 | `rack_alloc_full_limit` (L3305) | K334 | - | - | - | ASSIGNED K334 |
| 42 | `rack_alloc_limit` (L3321) | K334 | - | - | - | ASSIGNED K334 |
| 43 | `rack_free_trim` (L3348) | K334 | - | - | - | ASSIGNED K334 |
| 44 | `rack_free` (L3366) | K334 | - | - | - | ASSIGNED K334 |
| 45 | `rack_get_measure_window` (L3403) | K328 | - | - | - | ASSIGNED K328 |
| 46 | `rack_enough_for_measurement` (L3481) | K328 | - | - | - | ASSIGNED K328 |
| 47 | `rack_log_timely` (L3552) | K329 | crate::rack_log::rack_log_timely | donor_rack_log_timely | event suppression -> test fails | PASS |
| 48 | `rack_bw_can_be_raised` (L3600) | K328 | - | - | - | ASSIGNED K328 |
| 49 | `rack_validate_multipliers_at_or_above100` (L3673) | K334 | - | - | - | ASSIGNED K334 |
| 50 | `rack_validate_multipliers_at_or_below_100` (L3693) | K334 | - | - | - | ASSIGNED K334 |
| 51 | `rack_increase_bw_mul` (L3704) | K328 | - | - | - | ASSIGNED K328 |
| 52 | `rack_get_decrease` (L3797) | K334 | - | - | - | ASSIGNED K334 |
| 53 | `rack_decrease_highrtt` (L3840) | K334 | - | - | - | ASSIGNED K334 |
| 54 | `rack_decrease_bw_mul` (L3877) | K328 | - | - | - | ASSIGNED K328 |
| 55 | `rack_log_rtt_shrinks` (L4000) | K329 | crate::rack_log::rack_log_rtt_shrinks | donor_rack_log_rtt_shrinks | event suppression -> test fails | PASS |
| 56 | `rack_set_prtt_target` (L4050) | K334 | - | - | - | ASSIGNED K334 |
| 57 | `rack_enter_probertt` (L4069) | K328 | - | - | - | ASSIGNED K328 |
| 58 | `rack_exit_probertt` (L4123) | K328 | - | - | - | ASSIGNED K328 |
| 59 | `rack_check_probe_rtt` (L4292) | K334 | - | - | - | ASSIGNED K334 |
| 60 | `rack_update_multiplier` (L4394) | K334 | - | - | - | ASSIGNED K334 |
| 61 | `rack_make_timely_judgement` (L4559) | K328 | - | - | - | ASSIGNED K328 |
| 62 | `rack_in_gp_window` (L4623) | K328 | - | - | - | ASSIGNED K328 |
| 63 | `rack_mark_in_gp_win` (L4660) | K328 | - | - | - | ASSIGNED K328 |
| 64 | `rack_clear_gp_marks` (L4677) | K328 | - | - | - | ASSIGNED K328 |
| 65 | `rack_tend_gp_marks` (L4695) | K328 | - | - | - | ASSIGNED K328 |
| 66 | `rack_log_gp_calc` (L4746) | K329 | crate::rack_log::rack_log_gp_calc | donor_rack_log_gp_calc | event suppression -> test fails | PASS |
| 67 | `rack_do_goodput_measurement` (L4773) | K328 | - | - | - | ASSIGNED K328 |
| 68 | `rack_ack_received` (L5348) | K331 | - | - | - | ASSIGNED K331 |
| 69 | `tcp_rack_partialack` (L5483) | K334 | - | - | - | ASSIGNED K334 |
| 70 | `rack_exit_recovery` (L5502) | K333 | - | - | - | ASSIGNED K333 |
| 71 | `rack_post_recovery` (L5511) | K333 | - | - | - | ASSIGNED K333 |
| 72 | `rack_cong_signal` (L5591) | K269 | Rack::cong_signal | rack_cong_signal_enters_recovery_and_sets_prr; rack_rto_exits_recovery_and_rto_error_restores_it; rack_ecn_signal_sets_recover_beyond_snd_max | recovery-state mutation | PASS |
| 73 | `rack_cc_after_idle` (L5721) | K334 | - | - | - | ASSIGNED K334 |
| 74 | `rack_find_lowest_rsm` (L5763) | K331 | - | - | - | ASSIGNED K331 |
| 75 | `rack_find_high_nonack` (L5783) | K331 | - | - | - | ASSIGNED K331 |
| 76 | `rack_calc_thresh_rack` (L5804) | K333 | - | - | - | ASSIGNED K333 |
| 77 | `rack_calc_thresh_tlp` (L5893) | K333 | - | - | - | ASSIGNED K333 |
| 78 | `rack_grab_rtt` (L5976) | K334 | - | - | - | ASSIGNED K334 |
| 79 | `rack_check_recovery_mode` (L5994) | K333 | - | - | - | ASSIGNED K333 |
| 80 | `rack_get_persists_timer_val` (L6035) | K334 | - | - | - | ASSIGNED K334 |
| 81 | `rack_timer_start` (L6050) | K330 | - | - | - | ASSIGNED K330 |
| 82 | `rack_enter_persist` (L6278) | K333 | - | - | - | ASSIGNED K333 |
| 83 | `rack_exit_persist` (L6324) | K333 | - | - | - | ASSIGNED K333 |
| 84 | `rack_log_hpts_diag` (L6388) | K329 | crate::rack_log::rack_log_hpts_diag | donor_rack_log_hpts_diag | event suppression -> test fails | PASS |
| 85 | `rack_log_wakeup` (L6423) | K329 | crate::rack_log::rack_log_wakeup | donor_rack_log_wakeup | event suppression -> test fails | PASS |
| 86 | `rack_start_hpts_timer` (L6445) | K334 | - | - | - | ASSIGNED K334 |
| 87 | `rack_mark_lost` (L6754) | K333 | - | - | - | ASSIGNED K333 |
| 88 | `rack_mark_nolonger_lost` (L6781) | K333 | - | - | - | ASSIGNED K333 |
| 89 | `rack_timeout_rack` (L6800) | K330 | - | - | - | ASSIGNED K330 |
| 90 | `rack_adjust_orig_mlen` (L6847) | K334 | - | - | - | ASSIGNED K334 |
| 91 | `rack_setup_offset_for_rsm` (L6892) | K334 | - | - | - | ASSIGNED K334 |
| 92 | `rack_clone_rsm` (L6946) | K334 | - | - | - | ASSIGNED K334 |
| 93 | `rack_merge_rsm` (L6996) | K334 | - | - | - | ASSIGNED K334 |
| 94 | `rack_timeout_tlp` (L7086) | K330 | - | - | - | ASSIGNED K330 |
| 95 | `rack_timeout_delack` (L7275) | K330 | - | - | - | ASSIGNED K330 |
| 96 | `rack_send_ack_challange` (L7287) | K334 | - | - | - | ASSIGNED K334 |
| 97 | `rack_timeout_persist` (L7321) | K330 | - | - | - | ASSIGNED K330 |
| 98 | `rack_timeout_keepalive` (L7394) | K330 | - | - | - | ASSIGNED K330 |
| 99 | `rack_remxt_tmr` (L7437) | K330 | - | - | - | ASSIGNED K330 |
| 100 | `rack_convert_rtts` (L7533) | K334 | - | - | - | ASSIGNED K334 |
| 101 | `rack_cc_conn_init` (L7546) | K334 | - | - | - | ASSIGNED K334 |
| 102 | `rack_timeout_rxt` (L7582) | K330 | - | - | - | ASSIGNED K330 |
| 103 | `rack_process_timers` (L7868) | K334 | - | - | - | ASSIGNED K334 |
| 104 | `rack_timer_cancel` (L7981) | K330 | - | - | - | ASSIGNED K330 |
| 105 | `rack_stopall` (L8018) | K330 | - | - | - | ASSIGNED K330 |
| 106 | `rack_stop_all_timers` (L8031) | K334 | - | - | - | ASSIGNED K334 |
| 107 | `rack_update_rsm` (L8047) | K331 | - | - | - | ASSIGNED K331 |
| 108 | `rack_update_entry` (L8120) | K331 | - | - | - | ASSIGNED K331 |
| 109 | `rack_log_output` (L8198) | K329 | crate::rack_log::rack_log_output | donor_rack_log_output | event suppression -> test fails | PASS |
| 110 | `tcp_rack_xmit_timer` (L8501) | K334 | - | - | - | ASSIGNED K334 |
| 111 | `tcp_rack_xmit_timer_commit` (L8569) | K334 | - | - | - | ASSIGNED K334 |
| 112 | `rack_apply_updated_usrtt` (L8727) | K334 | - | - | - | ASSIGNED K334 |
| 113 | `rack_update_rtt` (L8773) | K331 | - | - | - | ASSIGNED K331 |
| 114 | `rack_log_sack_passed` (L9021) | K329 | crate::rack_log::rack_log_sack_passed | donor_rack_log_sack_passed | event suppression -> test fails | PASS |
| 115 | `rack_need_set_test` (L9082) | K331 | crate::rack_retransmit::RackRetransmit::rack_need_set_test | rack_need_set_test_advances_the_goodput_window | remove the RACK_USE_END sequence update -> test fails (exit 100) | PASS |
| 116 | `is_rsm_inside_declared_tlp_block` (L9232) | K334 | - | - | - | ASSIGNED K334 |
| 117 | `rack_check_reorder_ack` (L9248) | K331 | crate::rack_retransmit::RackRetransmit::rack_check_reorder_ack | rack_check_reorder_ack_undoes_spurious_recovery | remove the can_exit recovery block -> test fails (exit 100) | PASS |
| 118 | `rack_proc_sack_blk` (L9282) | K336 | - | - | - | ASSIGNED K336 |
| 119 | `rack_peer_reneges` (L10055) | K336 | - | - | - | ASSIGNED K336 |
| 120 | `rack_rsm_sender_update` (L10091) | K334 | - | - | - | ASSIGNED K334 |
| 121 | `rack_process_to_cumack` (L10182) | K331 | - | - | - | ASSIGNED K331 |
| 122 | `rack_handle_might_revert` (L10543) | K336 | - | - | - | ASSIGNED K336 |
| 123 | `rack_note_dsack` (L10602) | K336 | - | - | - | ASSIGNED K336 |
| 124 | `do_rack_compute_pipe` (L10668) | K333 | - | - | - | ASSIGNED K333 |
| 125 | `rack_compute_pipe` (L10675) | K333 | - | - | - | ASSIGNED K333 |
| 126 | `rack_update_prr` (L10683) | K269 | Rack::rack_update_prr | rack_pipe_and_prr_follow_the_recovery_state; rack_prr_low_pipe_allows_two_segments; rack_update_prr_high_pipe_uses_the_recovery_fraction | PRR +1 mutation | PASS |
| 127 | `rack_log_ack` (L10747) | K329 | crate::rack_log::rack_log_ack | donor_rack_log_ack | event suppression -> test fails | PASS |
| 128 | `rack_strike_dupack` (L10991) | K331 | - | - | - | ASSIGNED K331 |
| 129 | `rack_check_bottom_drag` (L11040) | K334 | - | - | - | ASSIGNED K334 |
| 130 | `rack_log_hybrid` (L11155) | K329 | crate::rack_log::rack_log_hybrid | donor_rack_log_hybrid | event suppression -> test fails | PASS |
| 131 | `rack_set_dgp_hybrid_mode` (L11243) | K328 | - | - | - | ASSIGNED K328 |
| 132 | `rack_chk_req_and_hybrid_on_out` (L11416) | K328 | - | - | - | ASSIGNED K328 |
| 133 | `rack_gain_for_fastoutput` (L11459) | K328 | - | - | - | ASSIGNED K328 |
| 134 | `rack_adjust_sendmap_head` (L11499) | K334 | - | - | - | ASSIGNED K334 |
| 135 | `rack_req_check_for_comp` (L11597) | K334 | - | - | - | ASSIGNED K334 |
| 136 | `rack_process_ack` (L11690) | K331 | - | - | - | ASSIGNED K331 |
| 137 | `rack_log_collapse` (L11985) | K329 | crate::rack_log::rack_log_collapse | donor_rack_log_collapse | event suppression -> test fails | PASS |
| 138 | `rack_collapsed_window` (L12018) | K334 | - | - | - | ASSIGNED K334 |
| 139 | `rack_un_collapse_window` (L12038) | K334 | - | - | - | ASSIGNED K334 |
| 140 | `rack_handle_delayed_ack` (L12102) | K334 | - | - | - | ASSIGNED K334 |
| 141 | `rack_validate_fo_sendwin_up` (L12115) | K334 | - | - | - | ASSIGNED K334 |
| 142 | `rack_process_data` (L12155) | K332 | - | - | - | ASSIGNED K332 |
| 143 | `rack_do_fastnewdata` (L12446) | K332 | - | - | - | ASSIGNED K332 |
| 144 | `rack_fastack` (L12565) | K331 | - | - | - | ASSIGNED K331 |
| 145 | `rack_do_syn_sent` (L12771) | K332 | - | - | - | ASSIGNED K332 |
| 146 | `rack_do_syn_recv` (L12995) | K332 | - | - | - | ASSIGNED K332 |
| 147 | `rack_do_established` (L13209) | K332 | - | - | - | ASSIGNED K332 |
| 148 | `rack_do_close_wait` (L13338) | K332 | - | - | - | ASSIGNED K332 |
| 149 | `rack_check_data_after_close` (L13427) | K334 | - | - | - | ASSIGNED K334 |
| 150 | `rack_do_fin_wait_1` (L13461) | K332 | - | - | - | ASSIGNED K332 |
| 151 | `rack_do_closing` (L13583) | K332 | - | - | - | ASSIGNED K332 |
| 152 | `rack_do_lastack` (L13684) | K332 | - | - | - | ASSIGNED K332 |
| 153 | `rack_do_fin_wait_2` (L13786) | K332 | - | - | - | ASSIGNED K332 |
| 154 | `rack_clear_rate_sample` (L13883) | K328 | - | - | - | ASSIGNED K328 |
| 155 | `rack_set_pace_segments` (L13891) | K334 | - | - | - | ASSIGNED K334 |
| 156 | `rack_init_fsb_block` (L13979) | K334 | - | - | - | ASSIGNED K334 |
| 157 | `rack_init_fsb` (L14041) | K334 | - | - | - | ASSIGNED K334 |
| 158 | `rack_log_hystart_event` (L14062) | K329 | crate::rack_log::rack_log_hystart_event | donor_rack_log_hystart_event | event suppression -> test fails | PASS |
| 159 | `rack_deferred_init` (L14094) | K334 | - | - | - | ASSIGNED K334 |
| 160 | `rack_init_retransmit_value` (L14103) | K331 | - | - | - | ASSIGNED K331 |
| 161 | `rack_log_chg_info` (L14145) | K329 | crate::rack_log::rack_log_chg_info | donor_rack_log_chg_info | event suppression -> test fails | PASS |
| 162 | `rack_chg_query` (L14163) | K334 | - | - | - | ASSIGNED K334 |
| 163 | `rack_switch_failed` (L14258) | K334 | - | - | - | ASSIGNED K334 |
| 164 | `rack_init_outstanding` (L14306) | K334 | - | - | - | ASSIGNED K334 |
| 165 | `rack_init` (L14467) | K334 | - | - | - | ASSIGNED K334 |
| 166 | `rack_handoff_ok` (L14907) | K330 | - | - | - | ASSIGNED K330 |
| 167 | `rack_fini` (L14946) | K334 | - | - | - | ASSIGNED K334 |
| 168 | `rack_set_state` (L15065) | K330 | - | - | - | ASSIGNED K330 |
| 169 | `rack_timer_audit` (L15119) | K330 | - | - | - | ASSIGNED K330 |
| 170 | `rack_do_win_updates` (L15207) | K332 | - | - | - | ASSIGNED K332 |
| 171 | `rack_log_input_packet` (L15257) | K329 | crate::rack_log::rack_log_input_packet | donor_rack_log_input_packet | event suppression -> test fails | PASS |
| 172 | `rack_handle_probe_response` (L15405) | K334 | - | - | - | ASSIGNED K334 |
| 173 | `rack_new_round_starts` (L15459) | K334 | - | - | - | ASSIGNED K334 |
| 174 | `rack_log_pcm` (L15475) | K329 | crate::rack_log::rack_log_pcm | donor_rack_log_pcm | event suppression -> test fails | PASS |
| 175 | `rack_new_round_setup` (L15502) | K334 | - | - | - | ASSIGNED K334 |
| 176 | `rack_do_compressed_ack_processing` (L15587) | K332 | - | - | - | ASSIGNED K332 |
| 177 | `rack_do_segment_nounlock` (L16268) | K332 | - | - | - | ASSIGNED K332 |
| 178 | `rack_do_segment` (L16933) | K332 | - | - | - | ASSIGNED K332 |
| 179 | `tcp_rack_output` (L16957) | K330 | - | - | - | ASSIGNED K330 |
| 180 | `rack_log_pacing_delay_calc` (L17033) | K329 | crate::rack_log::rack_log_pacing_delay_calc | donor_rack_log_pacing_delay_calc | event suppression -> test fails | PASS |
| 181 | `rack_get_pacing_len` (L17124) | K328 | - | - | - | ASSIGNED K328 |
| 182 | `rack_arrive_at_discounted_rate` (L17159) | K328 | - | - | - | ASSIGNED K328 |
| 183 | `pace_to_fill_cwnd` (L17213) | K328 | - | - | - | ASSIGNED K328 |
| 184 | `rack_get_pacing_delay` (L17361) | K328 | - | - | - | ASSIGNED K328 |
| 185 | `rack_start_gp_measurement` (L17747) | K328 | - | - | - | ASSIGNED K328 |
| 186 | `rack_what_can_we_send` (L17985) | K331 | - | - | - | ASSIGNED K331 |
| 187 | `rack_log_fsb` (L18028) | K329 | crate::rack_log::rack_log_fsb | donor_rack_log_fsb | event suppression -> test fails | PASS |
| 188 | `rack_fo_base_copym` (L18062) | K334 | - | - | - | ASSIGNED K334 |
| 189 | `rack_fo_m_copym` (L18225) | K334 | - | - | - | ASSIGNED K334 |
| 190 | `rack_log_queue_level` (L18290) | K329 | crate::rack_log::rack_log_queue_level | donor_rack_log_queue_level | event suppression -> test fails | PASS |
| 191 | `rack_check_queue_level` (L18319) | K334 | - | - | - | ASSIGNED K334 |
| 192 | `rack_fast_rsm_output` (L18387) | K334 | - | - | - | ASSIGNED K334 |
| 193 | `rack_sndbuf_autoscale` (L18915) | K334 | - | - | - | ASSIGNED K334 |
| 194 | `rack_fast_output` (L18974) | K334 | - | - | - | ASSIGNED K334 |
| 195 | `rack_setup_fast_output` (L19475) | K334 | - | - | - | ASSIGNED K334 |
| 196 | `rack_get_hpts_pacing_min_for_bw` (L19510) | K328 | - | - | - | ASSIGNED K328 |
| 197 | `rack_check_collapsed` (L19522) | K334 | - | - | - | ASSIGNED K334 |
| 198 | `rack_validate_sizes` (L19572) | K334 | - | - | - | ASSIGNED K334 |
| 199 | `rack_output` (L19601) | K330 | - | - | - | ASSIGNED K330 |
| 200 | `rack_update_seg` (L22528) | K331 | - | - | - | ASSIGNED K331 |
| 201 | `rack_mtu_change` (L22539) | K334 | - | - | - | ASSIGNED K334 |
| 202 | `rack_set_dgp` (L22573) | K334 | - | - | - | ASSIGNED K334 |
| 203 | `rack_set_profile` (L22632) | K334 | - | - | - | ASSIGNED K334 |
| 204 | `rack_add_deferred_option` (L22742) | K334 | - | - | - | ASSIGNED K334 |
| 205 | `process_hybrid_pacing` (L22761) | K328 | - | - | - | ASSIGNED K328 |
| 206 | `rack_stack_information` (L22850) | K334 | - | - | - | ASSIGNED K334 |
| 207 | `rack_process_option` (L22860) | K332 | - | - | - | ASSIGNED K332 |
| 208 | `rack_inherit` (L23633) | K334 | - | - | - | ASSIGNED K334 |
| 209 | `rack_apply_deferred_options` (L24013) | K334 | - | - | - | ASSIGNED K334 |
| 210 | `rack_hw_tls_change` (L24028) | K335 | - | - | - | ASSIGNED K335 |
| 211 | `rack_pru_options` (L24041) | K334 | - | - | - | ASSIGNED K334 |
| 212 | `rack_wake_check` (L24049) | K335 | - | - | - | ASSIGNED K335 |
| 213 | `rack_set_sockopt` (L24107) | K334 | - | - | - | ASSIGNED K334 |
| 214 | `rack_fill_info` (L24295) | K334 | - | - | - | ASSIGNED K334 |
| 215 | `rack_get_sockopt` (L24351) | K334 | - | - | - | ASSIGNED K334 |
| 216 | `rack_ctloutput` (L24689) | K334 | - | - | - | ASSIGNED K334 |
| 217 | `rack_ctor` (L24708) | K334 | - | - | - | ASSIGNED K334 |
| 218 | `rack_dtor` (L24715) | K334 | - | - | - | ASSIGNED K334 |
| 219 | `tcp_addrack` (L24723) | K335 | - | - | - | ASSIGNED K335 |

## Current status

| Status | Rows |
|---|---:|
| PASS (K329 43 + K269 2) | 45 |
| ASSIGNED K328 | 35 |
| ASSIGNED K330 | 15 |
| ASSIGNED K331 | 15 |
| ASSIGNED K332 | 15 |
| ASSIGNED K333 | 11 |
| ASSIGNED K334 | 76 |
| ASSIGNED K335 | 3 |
| ASSIGNED K336 | 4 |
| **total** | **219** |

The register is complete as a row inventory; the 174 `ASSIGNED` rows are the
missing mappings/tests/mutations until their owner branches land. The K315
checker reports them as empty Rust/test/mutation and not PASS, not as absent
rows.

## Audit findings

* **Count discrepancy:** ticket text says 218; the pinned donor parser and
  manifest both report 219. All 219 are registered.
* **K336 duplicate claims:** K336's report maps `rack_log_dsack_event` and
  `rack_log_sack_passed`, which K329 already owns and maps with tests. K336
  must drop those two rows from its final matrix; K338 keeps the K329 rows.
* **K336 ownership split:** K336's report also maps `rack_process_to_cumack`,
  which the canonical ownership matrix assigns to K331, and does not map
  `rack_handle_might_revert`, which the matrix assigns to K336. Resolve before
  marking K336 rows PASS.
* **K335 ownership overlap:** K335's donor table names `rack_ctloutput`,
  `rack_pru_options` and `rack_switch_failed`; the canonical matrix assigns
  those to K334. K335 owns only `tcp_addrack`, `rack_hw_tls_change` and
  `rack_wake_check` in this register.
* **K269 stale claims:** the older 20-row K269 table names `rack_ack_received`,
  `rack_process_ack`, `rack_update_entry`, `rack_update_rsm`, `rack_update_rtt`
  and others that the canonical matrix assigns to K331/K333 and that are not in
  the merged tree. Only `rack_cong_signal` and `rack_update_prr` are PASS here.
* **K329/K334 `rack_init_sysctls`:** an earlier ownership table assigned it to
  K334; the merged K329 code owns it and the register marks K329 PASS. K334
  must not duplicate it.

## Reproduce

```text
cargo run -q -p rack-coverage -- \
  --donor /tmp/opencode/freebsd-rack.c \
  --matrix docs/rack-coverage.md \
  --manifest tests/rack-coverage/fixtures/rack-functions.txt
```

Current result: donor 219, matrix 219, manifest matches; **0 missing, 0 duplicate,
0 unknown**, 174 empty Rust, 174 empty test, 174 empty mutation, 174 not PASS;
`result: FAIL` until the assigned groups land. The mutation checks are in the
K338 report.
