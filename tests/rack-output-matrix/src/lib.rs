// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K343 host-only matrix over the merged RACK output/timer/state module.
//!
//! The 21 donor functions are the K330 group. The matrix calls every public
//! path with independent inputs and compares the applicable contracts with the
//! K320 TLP model and the K327 vtable model. It is test/evidence tooling only.

#![forbid(unsafe_code)]

/// One K315 matrix row: donor function, Rust mapping, named test, mutation.
pub const MATRIX: &[(&str, &str, &str, &str)] = &[
    (
        "rack_output",
        "RackOutput::rack_output",
        "rack_output_sends_new_data",
        "window/MSS changed -> test fails",
    ),
    (
        "tcp_rack_output",
        "RackOutput::tcp_rack_output",
        "tcp_rack_output_returns_candrop_on_closed",
        "return code changed -> test fails",
    ),
    (
        "rack_get_persists_timer_val",
        "RackOutput::rack_get_persists_timer_val",
        "persist_timer_is_bounded",
        "clamp removed -> test fails",
    ),
    (
        "rack_timer_start",
        "RackOutput::rack_timer_start",
        "timer_start_arms_and_sets_bits",
        "arm removed -> test fails",
    ),
    (
        "rack_start_hpts_timer",
        "RackOutput::rack_start_hpts_timer",
        "hpts_timer_arms_a_slot",
        "arm removed -> test fails",
    ),
    (
        "rack_timer_cancel",
        "RackOutput::rack_timer_cancel",
        "timer_cancel_clears_the_bits",
        "cancel removed -> test fails",
    ),
    (
        "rack_timer_audit",
        "RackOutput::rack_timer_audit",
        "timer_audit_returns_due_bits",
        "audit ignores time -> test fails",
    ),
    (
        "rack_remxt_tmr",
        "RackOutput::rack_remxt_tmr",
        "remxt_sets_override_and_backoff",
        "backoff removed -> test fails",
    ),
    (
        "rack_stopall",
        "RackOutput::rack_stopall",
        "stopall_clears_all_timers",
        "stopall no-op -> test fails",
    ),
    (
        "rack_stop_all_timers",
        "RackOutput::rack_stop_all_timers",
        "stop_all_timers_clears_bits",
        "stop no-op -> test fails",
    ),
    (
        "rack_enter_persist",
        "RackOutput::rack_enter_persist",
        "enter_persist_arms_once",
        "second enter accepted -> test fails",
    ),
    (
        "rack_exit_persist",
        "RackOutput::rack_exit_persist",
        "exit_persist_clears_once",
        "exit without enter accepted -> test fails",
    ),
    (
        "rack_timeout_delack",
        "RackOutput::rack_timeout_delack",
        "delack_sets_acknow_once",
        "TF_ACKNOW not set -> test fails",
    ),
    (
        "rack_timeout_rack",
        "RackOutput::rack_timeout_rack",
        "timeout_rack_requests_retransmit",
        "retransmit not requested -> test fails",
    ),
    (
        "rack_timeout_tlp",
        "RackOutput::rack_timeout_tlp",
        "timeout_tlp_caps_at_two_probes",
        "third probe accepted -> test fails",
    ),
    (
        "rack_timeout_rxt",
        "RackOutput::rack_timeout_rxt",
        "timeout_rxt_backs_off_and_closes",
        "close limit removed -> test fails",
    ),
    (
        "rack_timeout_persist",
        "RackOutput::rack_timeout_persist",
        "timeout_persist_probes_and_rearms",
        "probe removed -> test fails",
    ),
    (
        "rack_timeout_keepalive",
        "RackOutput::rack_timeout_keepalive",
        "keepalive_counts_and_closes",
        "probe cap removed -> test fails",
    ),
    (
        "rack_process_timers",
        "RackOutput::rack_process_timers",
        "process_timers_dispatches_in_donor_order",
        "order changed -> test fails",
    ),
    (
        "rack_set_state",
        "RackOutput::rack_set_state",
        "set_state_updates_timers",
        "invalid state accepted -> test fails",
    ),
    (
        "rack_handoff_ok",
        "RackOutput::rack_handoff_ok",
        "handoff_rejects_fin_and_closed",
        "FIN accepted -> test fails",
    ),
];

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
