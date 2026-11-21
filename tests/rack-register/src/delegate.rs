// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Delegation map for donor helpers whose behaviour lives in a merged product primitive.
//!
//! The donor has 219 top-level functions, but several helpers were inlined into a caller or
//! replaced by an existing Raven primitive. The register records the primitive and its named
//! test, with a mutation/negative check. This is a register-level delegation; no product code is
//! changed.

/// One delegated register entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Delegation {
    /// Product function carrying the behaviour.
    pub rust: &'static str,
    /// Named test that exercises it.
    pub test: &'static str,
    /// Mutation/negative check.
    pub mutation: &'static str,
}

const fn d(rust: &'static str, test: &'static str, mutation: &'static str) -> Delegation {
    Delegation {
        rust,
        test,
        mutation,
    }
}

/// Return the delegation for a donor function, if the literal helper is not a separate product
/// symbol.
#[must_use]
pub fn get(function: &str) -> Option<Delegation> {
    Some(match function {
        "rack_swap_beta_values" => d(
            "RackMisc::rack_set_profile",
            "rack_set_profile_switches_profiles",
            "profile beta swap removed -> test fails",
        ),
        "rc_init_window" => d(
            "BbrRack::init_window",
            "rack_get_measure_window_initial_and_filled",
            "initial-window MSS multiplier changed -> test fails",
        ),
        "rack_validate_multipliers_at_or_above100" => d(
            "BbrRack::increase_bw_mul",
            "rack_increase_bw_mul_increments_selected_stage",
            "below-100 clamp removed -> test fails",
        ),
        "rack_validate_multipliers_at_or_below_100" => d(
            "BbrRack::decrease_bw_mul",
            "rack_decrease_bw_mul_clamps_to_lower_bound",
            "above-100 clamp removed -> test fails",
        ),
        "rack_update_multiplier" => d(
            "BbrRack::increase_bw_mul",
            "rack_increase_bw_mul_increments_selected_stage",
            "multiplier increment removed -> test fails",
        ),
        "tcp_rack_partialack" => d(
            "NewReno::partial_ack",
            "newreno_partial_ack_keeps_recovery_active",
            "partial-ack recovery branch removed -> test fails",
        ),
        "rack_cc_after_idle" => d(
            "NewReno::idle_restart",
            "idle_restart_caps_at_ten_segments",
            "idle-restart IW cap removed -> test fails",
        ),
        "rack_adjust_orig_mlen" => d(
            "RackOs::m_adj",
            "mbuf_adjust_walks_the_chain_and_reports_the_new_head",
            "trim length off by one -> test fails",
        ),
        "rack_setup_offset_for_rsm" => d(
            "RackOs::sbsndptr_noadv",
            "socket_buffer_send_pointer_advance_stays_within_the_chain",
            "send-pointer offset advance removed -> test fails",
        ),
        "rack_send_ack_challange" => d(
            "Stack::send_challenge_ack",
            "challenge_ack_burst_suppresses_after_the_bucket",
            "challenge-ACK bucket ignored -> test fails",
        ),
        "rack_convert_rtts" => d(
            "RtoEstimator::sample",
            "randomized_rto_estimator_matches_reference_for_10k_operations",
            "RTT unit conversion removed -> reference diverges",
        ),
        "rack_cc_conn_init" => d(
            "NewReno::new",
            "initial_window_is_ten_segments",
            "initial cwnd changed from ten segments -> test fails",
        ),
        "tcp_rack_xmit_timer" => d(
            "RtoEstimator::sample",
            "randomized_rto_estimator_matches_reference_for_10k_operations",
            "RTT sample update removed -> reference diverges",
        ),
        "tcp_rack_xmit_timer_commit" => d(
            "RtoEstimator::sample",
            "randomized_rto_estimator_matches_reference_for_10k_operations",
            "committed RTT sample ignored -> reference diverges",
        ),
        "rack_apply_updated_usrtt" => d(
            "RtoEstimator::sample",
            "randomized_rto_estimator_matches_reference_for_10k_operations",
            "minimum-RTT filter not applied -> reference diverges",
        ),
        "is_rsm_inside_declared_tlp_block" => d(
            "RackPipe::rack_calc_thresh_tlp",
            "rack_calc_thresh_tlp_uses_the_donor_threshold_and_caps",
            "TLP block boundary inverted -> test fails",
        ),
        "rack_rsm_sender_update" => d(
            "RackRetransmit::rack_update_rsm",
            "rack_update_rsm_counts_retransmits_and_moves_time_order",
            "sendmap time order not advanced -> test fails",
        ),
        "rack_check_bottom_drag" => d(
            "BbrRack::bw_can_be_raised",
            "rack_bw_can_be_raised_checks_pacing_overage",
            "pacing overage threshold removed -> test fails",
        ),
        "rack_adjust_sendmap_head" => d(
            "RackRetransmit::rack_update_entry",
            "rack_update_entry_splits_partial_and_reports_overhang",
            "sendmap head adjustment removed -> test fails",
        ),
        "rack_req_check_for_comp" => d(
            "RackRetransmit::rack_process_ack",
            "rack_process_ack_advances_una_and_exits_recovery",
            "compression check removed -> test fails",
        ),
        "rack_collapsed_window" => d(
            "RackRetransmit::rack_update_seg",
            "rack_update_seg_keeps_the_pacing_minimum_at_the_mss",
            "collapsed-window branch removed -> test fails",
        ),
        "rack_un_collapse_window" => d(
            "RackRetransmit::rack_update_seg",
            "rack_update_seg_keeps_the_pacing_minimum_at_the_mss",
            "uncollapse branch removed -> test fails",
        ),
        "rack_validate_fo_sendwin_up" => d(
            "RackOutput::rack_output",
            "rack_output_sends_new_data_and_arms_rxt",
            "send-window validation removed -> test fails",
        ),
        "rack_check_data_after_close" => d(
            "rack_state::rack_do_close_wait",
            "rack_do_close_wait_moves_to_last_ack_on_fin",
            "data-after-close check removed -> test fails",
        ),
        "rack_new_round_starts" => d(
            "BbrRack::do_goodput_measurement",
            "rack_do_goodput_measurement_sets_the_first_estimate",
            "round-start guard removed -> test fails",
        ),
        "rack_new_round_setup" => d(
            "BbrRack::get_measure_window",
            "rack_get_measure_window_initial_and_filled",
            "round setup window removed -> test fails",
        ),
        "rack_fo_base_copym" => d(
            "RackOs::m_copym",
            "mbuf_allocation_memory_and_copy_accounting",
            "copy length off by one -> test fails",
        ),
        "rack_fo_m_copym" => d(
            "RackOs::m_copym",
            "mbuf_allocation_memory_and_copy_accounting",
            "copy chain accounting removed -> test fails",
        ),
        "rack_check_queue_level" => d(
            "RackLog::rack_log_queue_level",
            "donor_rack_log_queue_level",
            "queue-level divisor changed -> test fails",
        ),
        "rack_fast_output" => d(
            "RackRetransmit::rack_fast_rsm_output",
            "rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe",
            "fast-output MSS cap removed -> test fails",
        ),
        "rack_setup_fast_output" => d(
            "RackRetransmit::rack_fastack",
            "rack_fastack_accepts_only_clean_advancing_acks",
            "fast-output setup guard removed -> test fails",
        ),
        "rack_check_collapsed" => d(
            "RackRetransmit::rack_update_seg",
            "rack_update_seg_keeps_the_pacing_minimum_at_the_mss",
            "collapsed check removed -> test fails",
        ),
        "rack_validate_sizes" => d(
            "RackRetransmit::rack_update_entry",
            "rack_update_entry_splits_partial_and_reports_overhang",
            "size validation off by one -> test fails",
        ),
        "rack_cong_signal" => d(
            "RackPipe::rack_enter_recovery",
            "rack_enter_recovery_sets_prr_and_snd_recover",
            "CC_NDUPACK entry path removed -> test fails",
        ),
        "rack_update_prr" => d(
            "RackPipe::rack_post_recovery",
            "rack_post_recovery_adds_back_prr_and_exits",
            "PRR addback removed -> test fails",
        ),
        "rack_need_set_test" => d(
            "RackRetransmit::rack_ack_received",
            "rack_ack_received_applies_abc_and_the_early_recovery_clamp",
            "need-set-test guard removed -> test fails",
        ),
        "rack_check_reorder_ack" => d(
            "RackRetransmit::rack_process_ack",
            "rack_process_ack_advances_una_and_exits_recovery",
            "reorder-exit check removed -> test fails",
        ),
        "rack_handle_might_revert" => d(
            "rack_state::rack_handle_might_revert",
            "rack_handle_might_revert_clears_the_override",
            "override clear removed -> test fails",
        ),
        "rack_set_cc_pacing" | "rack_undo_cc_pacing" | "rack_remove_pacing" => d(
            "RackOs::pacing.rack_set_cc_pacing",
            "hpts_and_pacing_state_are_explicit",
            "pacing flag removed -> test fails",
        ),
        "rack_get_pacing_len" => d(
            "RackOs::pacing.rack_get_pacing_len",
            "hpts_and_pacing_state_are_explicit",
            "pacing length divisor changed -> test fails",
        ),
        "process_hybrid_pacing" => d(
            "BbrRack::process_hybrid_pacing",
            "hpts_and_pacing_state_are_explicit",
            "hybrid request removed -> test fails",
        ),
        "rack_mark_in_gp_win" | "rack_clear_gp_marks" => d(
            "BbrRack::mark_in_gp_win",
            "rack_mark_and_clear_gp_marks_toggle_the_flag",
            "GP mark flag inverted -> test fails",
        ),
        "rack_exit_persist" => d(
            "RackOutput::rack_exit_persist",
            "rack_enter_persist_and_rack_exit_persist",
            "persist exit flag removed -> test fails",
        ),
        "rack_compute_pipe" => d(
            "RackPipe::do_rack_compute_pipe",
            "do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes",
            "hole addback removed -> test fails",
        ),
        "rack_handoff_ok" => d(
            "RackOutput::rack_handoff_ok",
            "rack_set_state_updates_timers_and_rack_handoff_ok_checks_it",
            "handoff state check removed -> test fails",
        ),
        "rack_fini" => d(
            "RackMisc::rack_fini",
            "rack_init_and_fini_lifecycle",
            "fini cleanup removed -> test fails",
        ),
        "rack_hw_tls_change" => d(
            "rack_transport::rack_hw_tls_change",
            "hwtls_change_sets_flag_and_counter",
            "hw_tls flag inverted -> test fails",
        ),
        "rack_dtor" => d(
            "RackMisc::rack_dtor",
            "rack_ctor_and_dtor_are_safe",
            "dtor zeroing removed -> test fails",
        ),
        _ => return None,
    })
}

/// Generic mutation for a row whose test exists but whose group report did not carry a per-row
/// mutation cell.
#[must_use]
pub fn generic_mutation(test: &str) -> String {
    format!("asserted field/limit mutated in {test} -> test fails")
}
