// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K352 host-only audit of the merged RACK logging (K329, 43 functions) and
//! BBR/pacing (K328, 35 functions) donor groups.
//!
//! The audit checks the merged source for every mapping and named test, checks
//! the donor constants, and reports exact ownership/duplication discrepancies.
//! It is test/evidence tooling and changes no product source.

#![forbid(unsafe_code)]

/// K328 BBR/pacing group rows: donor function, Rust mapping, named test, mutation.
pub const BBR_ROWS: &[(&str, &str, &str, &str)] = &[
    (
        "rack_get_lt_bw",
        "BbrRack::get_lt_bw",
        "rack_get_lt_bw_uses_bytes_and_time",
        "time multiplication removed -> test fails",
    ),
    (
        "rack_set_cc_pacing",
        "RackOs::pacing.rack_set_cc_pacing",
        "hpts_and_pacing_state_are_explicit",
        "K314 pacing flag mutation",
    ),
    (
        "rack_undo_cc_pacing",
        "RackOs::pacing.rack_undo_cc_pacing",
        "hpts_and_pacing_state_are_explicit",
        "K314 pacing flag mutation",
    ),
    (
        "rack_remove_pacing",
        "RackOs::pacing.rack_remove_pacing",
        "hpts_and_pacing_state_are_explicit",
        "K314 pacing method mutation",
    ),
    (
        "rack_get_fixed_pacing_bw",
        "BbrRack::get_fixed_pacing_bw",
        "rack_get_fixed_pacing_bw_selects_recovery_ss_ca",
        "recovery bit ignored -> test fails",
    ),
    (
        "rack_compensate_for_linerate",
        "BbrRack::compensate_for_linerate",
        "rack_compensate_for_linerate_adds_headers",
        "header term removed -> test fails",
    ),
    (
        "rack_rate_cap_bw",
        "BbrRack::rate_cap_bw",
        "rack_rate_cap_bw_clamps_to_the_cap",
        "cap comparison inverted -> test fails",
    ),
    (
        "rack_get_gp_est",
        "BbrRack::get_gp_est",
        "rack_get_gp_est_initial_rate_and_lt_bw",
        "lt_bw/measurement mutation",
    ),
    (
        "rack_get_bw",
        "BbrRack::get_bw",
        "rack_get_bw_uses_fixed_rate_when_configured",
        "fixed-rate branch mutation",
    ),
    (
        "rack_get_output_gain",
        "BbrRack::get_output_gain",
        "rack_get_output_gain_uses_recovery_and_configured_rate",
        "gain branch mutation",
    ),
    (
        "rack_get_output_bw",
        "BbrRack::get_output_bw",
        "rack_get_output_bw_applies_gain_and_minimum",
        "gain/cap mutation",
    ),
    (
        "rack_get_measure_window",
        "BbrRack::get_measure_window",
        "rack_get_measure_window_initial_and_filled",
        "default window mutation",
    ),
    (
        "rack_enough_for_measurement",
        "BbrRack::enough_for_measurement",
        "rack_enough_for_measurement_quality_paths",
        "quality threshold mutation",
    ),
    (
        "rack_bw_can_be_raised",
        "BbrRack::bw_can_be_raised",
        "rack_bw_can_be_raised_checks_pacing_overage",
        "overage threshold mutation",
    ),
    (
        "rack_increase_bw_mul",
        "BbrRack::increase_bw_mul",
        "rack_increase_bw_mul_increments_selected_stage",
        "plus value mutation",
    ),
    (
        "rack_decrease_bw_mul",
        "BbrRack::decrease_bw_mul",
        "rack_decrease_bw_mul_clamps_to_lower_bound",
        "decrease constant mutation",
    ),
    (
        "rack_enter_probertt",
        "BbrRack::enter_probertt",
        "rack_enter_probertt_sets_the_state_and_target",
        "state flag mutation",
    ),
    (
        "rack_exit_probertt",
        "BbrRack::exit_probertt",
        "rack_exit_probertt_restores_the_multiplier",
        "restore mutation",
    ),
    (
        "rack_make_timely_judgement",
        "BbrRack::make_timely_judgement",
        "rack_make_timely_judgement_thresholds",
        "max multiplier mutation",
    ),
    (
        "rack_in_gp_window",
        "BbrRack::in_gp_window",
        "rack_in_gp_window_reads_the_tcp_flag",
        "flag read mutation",
    ),
    (
        "rack_mark_in_gp_win",
        "BbrRack::mark_in_gp_win",
        "rack_mark_and_clear_gp_marks_toggle_the_flag",
        "flag set mutation",
    ),
    (
        "rack_clear_gp_marks",
        "BbrRack::clear_gp_marks",
        "rack_mark_and_clear_gp_marks_toggle_the_flag",
        "flag clear mutation",
    ),
    (
        "rack_tend_gp_marks",
        "BbrRack::tend_gp_marks",
        "rack_tend_gp_marks_logs_the_window",
        "gput_ack mutation",
    ),
    (
        "rack_do_goodput_measurement",
        "BbrRack::do_goodput_measurement",
        "rack_do_goodput_measurement_sets_the_first_estimate",
        "gp_bw mutation",
    ),
    (
        "rack_set_dgp_hybrid_mode",
        "BbrRack::set_dgp_hybrid_mode",
        "rack_set_dgp_hybrid_mode_applies_the_cspr_cap",
        "cap mutation",
    ),
    (
        "rack_chk_req_and_hybrid_on_out",
        "BbrRack::chk_req_and_hybrid_on_out",
        "rack_chk_req_and_hybrid_on_out_extends_and_marks_first_send",
        "end_seq mutation",
    ),
    (
        "rack_gain_for_fastoutput",
        "BbrRack::gain_for_fastoutput",
        "rack_gain_for_fastoutput_expands_within_gating",
        "new_total mutation",
    ),
    (
        "rack_clear_rate_sample",
        "BbrRack::clear_rate_sample",
        "rack_clear_rate_sample_clears_the_filter",
        "filter flag mutation",
    ),
    (
        "rack_get_pacing_len",
        "RackOs::pacing.rack_get_pacing_len",
        "hpts_and_pacing_state_are_explicit",
        "K314 pacing length mutation",
    ),
    (
        "rack_arrive_at_discounted_rate",
        "BbrRack::arrive_at_discounted_rate",
        "rack_arrive_at_discounted_rate_halves_in_recovery",
        "half-rate mutation",
    ),
    (
        "pace_to_fill_cwnd",
        "BbrRack::pace_to_fill_cwnd",
        "pace_to_fill_cwnd_uses_the_last_rtt",
        "delay mutation",
    ),
    (
        "rack_get_pacing_delay",
        "BbrRack::get_pacing_delay",
        "rack_get_pacing_delay_old_method_is_positive",
        "msec conversion mutation",
    ),
    (
        "rack_start_gp_measurement",
        "BbrRack::start_gp_measurement",
        "rack_start_gp_measurement_sets_the_initial_window",
        "window mutation",
    ),
    (
        "rack_get_hpts_pacing_min_for_bw",
        "BbrRack::get_hpts_pacing_min_for_bw",
        "rack_get_hpts_pacing_min_for_bw_rounds_to_mss",
        "rounding mutation",
    ),
    (
        "process_hybrid_pacing",
        "BbrRack::process_hybrid_pacing",
        "program_hybrid_pacing_installs_the_request",
        "deadline/cap mutation",
    ),
];

/// K329 logging/sysctl group rows: donor function, Rust mapping, named test, mutation.
pub const LOG_ROWS: &[(&str, &str, &str, &str)] = &[
    (
        "rack_log_gpset",
        "crate::rack_log::rack_log_gpset",
        "donor_rack_log_gpset",
        "event suppression -> test fails",
    ),
    (
        "sysctl_rack_clear",
        "crate::rack_log::sysctl_rack_clear",
        "donor_sysctl_rack_clear",
        "event suppression -> test fails",
    ),
    (
        "sysctl_rack_wma_divisor",
        "crate::rack_log::sysctl_rack_wma_divisor",
        "donor_sysctl_rack_wma_divisor",
        "event suppression -> test fails",
    ),
    (
        "rack_init_sysctls",
        "crate::rack_log::rack_init_sysctls",
        "donor_rack_init_sysctls",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hybrid_bw",
        "crate::rack_log::rack_log_hybrid_bw",
        "donor_rack_log_hybrid_bw",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hybrid_sends",
        "crate::rack_log::rack_log_hybrid_sends",
        "donor_rack_log_hybrid_sends",
        "event suppression -> test fails",
    ),
    (
        "rack_log_dsack_event",
        "crate::rack_log::rack_log_dsack_event",
        "donor_rack_log_dsack_event",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hdwr_pacing",
        "crate::rack_log::rack_log_hdwr_pacing",
        "donor_rack_log_hdwr_pacing",
        "event suppression -> test fails",
    ),
    (
        "rack_log_retran_reason",
        "crate::rack_log::rack_log_retran_reason",
        "donor_rack_log_retran_reason",
        "event suppression -> test fails",
    ),
    (
        "rack_log_to_start",
        "crate::rack_log::rack_log_to_start",
        "donor_rack_log_to_start",
        "event suppression -> test fails",
    ),
    (
        "rack_log_to_event",
        "crate::rack_log::rack_log_to_event",
        "donor_rack_log_to_event",
        "event suppression -> test fails",
    ),
    (
        "rack_log_map_chg",
        "crate::rack_log::rack_log_map_chg",
        "donor_rack_log_map_chg",
        "event suppression -> test fails",
    ),
    (
        "rack_log_rtt_upd",
        "crate::rack_log::rack_log_rtt_upd",
        "donor_rack_log_rtt_upd",
        "event suppression -> test fails",
    ),
    (
        "rack_log_rtt_sample",
        "crate::rack_log::rack_log_rtt_sample",
        "donor_rack_log_rtt_sample",
        "event suppression -> test fails",
    ),
    (
        "rack_log_rtt_sample_calc",
        "crate::rack_log::rack_log_rtt_sample_calc",
        "donor_rack_log_rtt_sample_calc",
        "event suppression -> test fails",
    ),
    (
        "rack_log_rtt_sendmap",
        "crate::rack_log::rack_log_rtt_sendmap",
        "donor_rack_log_rtt_sendmap",
        "event suppression -> test fails",
    ),
    (
        "rack_log_progress_event",
        "crate::rack_log::rack_log_progress_event",
        "donor_rack_log_progress_event",
        "event suppression -> test fails",
    ),
    (
        "rack_log_type_bbrsnd",
        "crate::rack_log::rack_log_type_bbrsnd",
        "donor_rack_log_type_bbrsnd",
        "event suppression -> test fails",
    ),
    (
        "rack_log_doseg_done",
        "crate::rack_log::rack_log_doseg_done",
        "donor_rack_log_doseg_done",
        "event suppression -> test fails",
    ),
    (
        "rack_log_type_pacing_sizes",
        "crate::rack_log::rack_log_type_pacing_sizes",
        "donor_rack_log_type_pacing_sizes",
        "event suppression -> test fails",
    ),
    (
        "rack_log_type_just_return",
        "crate::rack_log::rack_log_type_just_return",
        "donor_rack_log_type_just_return",
        "event suppression -> test fails",
    ),
    (
        "rack_log_to_cancel",
        "crate::rack_log::rack_log_to_cancel",
        "donor_rack_log_to_cancel",
        "event suppression -> test fails",
    ),
    (
        "rack_log_alt_to_to_cancel",
        "crate::rack_log::rack_log_alt_to_to_cancel",
        "donor_rack_log_alt_to_to_cancel",
        "event suppression -> test fails",
    ),
    (
        "rack_log_to_processing",
        "crate::rack_log::rack_log_to_processing",
        "donor_rack_log_to_processing",
        "event suppression -> test fails",
    ),
    (
        "rack_log_to_prr",
        "crate::rack_log::rack_log_to_prr",
        "donor_rack_log_to_prr",
        "event suppression -> test fails",
    ),
    (
        "rack_counter_destroy",
        "crate::rack_log::rack_counter_destroy",
        "donor_rack_counter_destroy",
        "event suppression -> test fails",
    ),
    (
        "rack_log_timely",
        "crate::rack_log::rack_log_timely",
        "donor_rack_log_timely",
        "event suppression -> test fails",
    ),
    (
        "rack_log_rtt_shrinks",
        "crate::rack_log::rack_log_rtt_shrinks",
        "donor_rack_log_rtt_shrinks",
        "event suppression -> test fails",
    ),
    (
        "rack_log_gp_calc",
        "crate::rack_log::rack_log_gp_calc",
        "donor_rack_log_gp_calc",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hpts_diag",
        "crate::rack_log::rack_log_hpts_diag",
        "donor_rack_log_hpts_diag",
        "event suppression -> test fails",
    ),
    (
        "rack_log_wakeup",
        "crate::rack_log::rack_log_wakeup",
        "donor_rack_log_wakeup",
        "event suppression -> test fails",
    ),
    (
        "rack_log_output",
        "crate::rack_log::rack_log_output",
        "donor_rack_log_output",
        "event suppression -> test fails",
    ),
    (
        "rack_log_sack_passed",
        "crate::rack_log::rack_log_sack_passed",
        "donor_rack_log_sack_passed",
        "event suppression -> test fails",
    ),
    (
        "rack_log_ack",
        "crate::rack_log::rack_log_ack",
        "donor_rack_log_ack",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hybrid",
        "crate::rack_log::rack_log_hybrid",
        "donor_rack_log_hybrid",
        "event suppression -> test fails",
    ),
    (
        "rack_log_collapse",
        "crate::rack_log::rack_log_collapse",
        "donor_rack_log_collapse",
        "event suppression -> test fails",
    ),
    (
        "rack_log_hystart_event",
        "crate::rack_log::rack_log_hystart_event",
        "donor_rack_log_hystart_event",
        "event suppression -> test fails",
    ),
    (
        "rack_log_chg_info",
        "crate::rack_log::rack_log_chg_info",
        "donor_rack_log_chg_info",
        "event suppression -> test fails",
    ),
    (
        "rack_log_input_packet",
        "crate::rack_log::rack_log_input_packet",
        "donor_rack_log_input_packet",
        "event suppression -> test fails",
    ),
    (
        "rack_log_pcm",
        "crate::rack_log::rack_log_pcm",
        "donor_rack_log_pcm",
        "event suppression -> test fails",
    ),
    (
        "rack_log_pacing_delay_calc",
        "crate::rack_log::rack_log_pacing_delay_calc",
        "donor_rack_log_pacing_delay_calc",
        "event suppression -> test fails",
    ),
    (
        "rack_log_fsb",
        "crate::rack_log::rack_log_fsb",
        "donor_rack_log_fsb",
        "event suppression -> test fails",
    ),
    (
        "rack_log_queue_level",
        "crate::rack_log::rack_log_queue_level",
        "donor_rack_log_queue_level",
        "event suppression -> test fails",
    ),
];

/// All 78 rows in the two-group slice.
pub fn matrix() -> impl Iterator<Item = (&'static str, &'static str, &'static str, &'static str)> {
    BBR_ROWS.iter().chain(LOG_ROWS.iter()).copied()
}

/// Donor constants and their expected source spelling.
pub const DONOR_CONSTANTS: &[&str] = &[
    "pub const BBR_UNIT: u32 = 256;",
    "pub const RACK_MIN_BW: u64 = 8_000;",
    "pub const MIN_GP_WIN: u32 = 6;",
    "pub const RACK_REQ_AVG: u32 = 3;",
    "pub const RACK_TIMELY_CNT_BOOST: u32 = 5;",
    "pub const RACK_INITIAL_RTO: u64 = 1_000_000;",
    "pub const PACE_MAX_IP_BYTES: u32 = 65_536;",
    "pub const RACK_DGP_PACING: u32 = 0x01;",
    "pub const RACK_REG_PACING: u32 = 0x02;",
    "pub const HYBRID_CU: u32 = 0x0001;",
    "pub const HYBRID_DTL: u32 = 0x0002;",
    "pub const HYBRID_CSPR: u32 = 0x0004;",
    "pub const HYBRID_H_MS: u32 = 0x0008;",
    "pub const HYBRID_ENABLE: u32 = 0x0010;",
    "pub const HYBRID_S_MSS: u32 = 0x0020;",
    "pub const HYBRID_SETMSS: u32 = 0x1000;",
    "pub const HYBRID_WASSET: u32 = 0x2000;",
    "pub const HYBRID_SENDTIME: u32 = 0x4000;",
    "pub const TRK_EMPTY: u32 = 0;",
    "pub const TRK_USED: u32 = 0x01;",
    "pub const TRK_FSND: u32 = 0x10;",
    "pub const TRK_LSND: u32 = 0x20;",
];

/// Donor defaults audited in the K328 tunables.
pub const DONOR_DEFAULTS: &[(&str, &str)] = &[
    (
        "probe_rtt_safety: 2_000_000",
        "probe_rtt_safety_val 2000000",
    ),
    (
        "min_probertt_hold: 200_000",
        "min_probertt_hold sysctl default 200000",
    ),
];

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
