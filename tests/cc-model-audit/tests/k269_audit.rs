// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Audit of the K269 CUBIC/RACK mapping rows against the merged product tree.
//!
//! The K269 report is still `WORKING`; this file records which claimed rows are actually present
//! on `origin/master` and which are unmerged findings. It reads product source, never edits it.

use std::fs;
use std::path::{Path, PathBuf};

fn workspace_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(relative)
}

fn read(relative: &str) -> String {
    fs::read_to_string(workspace_path(relative))
        .unwrap_or_else(|error| panic!("cannot read {relative}: {error}"))
}

fn contains_fn(source: &str, name: &str) -> bool {
    source.contains(&format!("fn {name}(")) || source.contains(&format!("fn {name}<"))
}

fn all_raven_tcpip_source() -> String {
    let root = workspace_path("crates/raven-tcpip");
    let mut text = String::new();
    let mut stack = vec![root];
    while let Some(path) = stack.pop() {
        let Ok(entries) = fs::read_dir(&path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|extension| extension == "rs")
                && let Ok(file) = fs::read_to_string(&path)
            {
                text.push_str(&file);
                text.push('\n');
            }
        }
    }
    text
}

/// The CUBIC test names claimed PASS in the K269 report's requirements trace.
const K269_CUBIC_TESTS: [&str; 18] = [
    "cubic_ack_received_resets_the_epoch_then_grows_on_the_curve",
    "cubic_slow_start_uses_abc_and_nsegs",
    "cubic_slow_start_waits_for_eight_rtt_samples",
    "cubic_k_matches_the_donor_reference",
    "cubic_cwnd_matches_the_donor_reference",
    "cubic_cong_signal_dupack_cuts_by_beta_and_marks_cong_event",
    "cubic_cong_signal_ecn_sets_cwnd_to_ssthresh",
    "cubic_cong_signal_rto_uses_beta_of_the_min_send_window",
    "cubic_rto_error_restores_the_saved_epoch",
    "cubic_get_ssthresh_applies_fast_convergence",
    "cubic_get_ssthresh_uses_newreno_inside_tf",
    "cubic_post_recovery_uses_the_pipe_floor_then_beta",
    "cubic_post_recovery_averages_the_epoch_rtt_and_resets",
    "cubic_record_rtt_tracks_minimum_and_epoch_sum",
    "cubic_after_idle_resets_the_restart_window_and_epoch",
    "cubic_newround_rotates_the_css_round",
    "cubic_newround_leaves_css_after_five_rounds",
    "cubic_rttsample_tracks_the_round_minimum",
];

/// CUBIC Rust paths claimed in the K269 donor-coverage table.
const K269_CUBIC_SYMBOLS: [&str; 11] = [
    "ack_received",
    "congestion",
    "post_recovery",
    "record_rtt",
    "rttsample",
    "get_ssthresh",
    "after_idle",
    "newround",
    "cubic_k",
    "cubic_cwnd",
    "tf_cwnd",
];

/// RACK test names claimed PASS in the K269 report's requirements trace.
const K269_RACK_TESTS: [&str; 22] = [
    "rack_sack_block_marks_data_and_samples_rtt",
    "rack_sack_passed_declares_loss_after_the_threshold",
    "rack_core_ack_declares_sack_loss_and_enters_recovery",
    "rack_update_entry_splits_a_partial_retransmission",
    "rack_update_entry_spans_multiple_ranges",
    "rack_core_post_recovery_uses_the_ssthresh_floor",
    "rack_cong_signal_enters_recovery_and_sets_prr",
    "rack_rto_exits_recovery_and_rto_error_restores_it",
    "rack_ecn_signal_sets_recover_beyond_snd_max",
    "rack_rtt_ignores_a_retransmit_faster_than_min_rtt",
    "rack_timestamp_echo_on_an_older_transmit_is_spurious",
    "rack_dsack_counts_and_grows_the_reordering_window",
    "rack_dsack_equal_to_retransmitted_bytes_marks_a_spurious_recovery",
    "rack_dsack_inside_a_tlp_is_not_counted_for_reordering",
    "rack_peer_reneging_clears_a_sacked_range",
    "rack_reneged_range_can_be_declared_lost_later",
    "rack_reorder_threshold_uses_srtt_dsack_and_the_ceiling",
    "rack_reorder_threshold_non_standard_and_fade",
    "rack_process_sack_block_splits_across_multiple_ranges",
    "rack_core_ack_retains_newer_ranges_and_exits_recovery",
    "rack_next_retransmit_requires_sack_pass_and_time",
    "rack_highest_unacked_skips_sacked_ranges",
];

/// RACK Rust paths claimed in the K269 donor-coverage table.
const K269_RACK_SYMBOLS: [&str; 13] = [
    "ack_received",
    "process_ack",
    "update_entry",
    "update_rsm",
    "post_recovery",
    "cong_signal",
    "update_rtt",
    "note_dsack",
    "process_to_cumack",
    "reorder_threshold_us",
    "process_sack_block",
    "oldest_unacked_index",
    "highest_unacked",
];

#[test]
fn k269_cubic_rows_are_not_in_the_merged_tree() {
    let cc = read("crates/raven-tcpip/src/cc.rs");
    assert!(contains_fn(&cc, "loss_event"), "merged CUBIC loss_event");
    assert!(contains_fn(&cc, "advance"), "merged CUBIC advance");
    assert!(contains_fn(&cc, "rto_event"), "merged CUBIC rto_event");
    assert!(
        cc.contains("w_last_max"),
        "merged CUBIC fast-convergence state"
    );

    for symbol in K269_CUBIC_SYMBOLS {
        assert!(
            !contains_fn(&cc, symbol),
            "K359-F09: K269 claims Cubic::{symbol}, absent from merged cc.rs"
        );
    }

    let cc_ut = workspace_path("crates/raven-tcpip/src/cc_ut.rs");
    assert!(
        !cc_ut.exists(),
        "K359-F09: K269 claims tests in cc_ut.rs, which is not on origin/master"
    );
    let all = all_raven_tcpip_source();
    for test in K269_CUBIC_TESTS {
        assert!(
            !contains_fn(&all, test),
            "K359-F09: K269 claims PASS for {test}, absent from the merged tree"
        );
    }
}

#[test]
fn k269_rack_rows_are_not_in_the_merged_tree() {
    let recovery = read("crates/raven-tcpip/src/recovery.rs");
    assert!(contains_fn(&recovery, "on_sent"), "merged Rack::on_sent");
    assert!(contains_fn(&recovery, "is_lost"), "merged Rack::is_lost");
    assert!(contains_fn(&recovery, "on_ack"), "merged Rack::on_ack");
    assert!(
        contains_fn(&recovery, "is_sooner_than"),
        "merged TLP comparator"
    );

    for symbol in K269_RACK_SYMBOLS {
        assert!(
            !contains_fn(&recovery, symbol),
            "K359-F10: K269 claims Rack::{symbol}, absent from merged recovery.rs"
        );
    }

    let all = all_raven_tcpip_source();
    for test in K269_RACK_TESTS {
        assert!(
            !contains_fn(&all, test),
            "K359-F10: K269 claims PASS for {test}, absent from the merged tree"
        );
    }
    assert!(
        contains_fn(&all, "rack_declares_loss_after_the_window"),
        "the merged RACK detector has its own smaller test set"
    );
}

#[test]
fn k269_canonical_matrix_pass_rows_point_at_missing_rust_paths() {
    let docs = read("docs/rack-coverage.md");
    let mut pass_rows = Vec::new();
    for line in docs.lines().filter(|line| line.contains("| K269 |")) {
        if !line.contains("PASS") {
            continue;
        }
        let start = line.find('`').expect("function backtick");
        let end = line[start + 1..].find('`').expect("function close");
        pass_rows.push(line[start + 1..start + 1 + end].to_owned());
    }
    assert_eq!(
        pass_rows,
        ["rack_cong_signal", "rack_update_prr"],
        "the canonical K269 PASS rows"
    );

    let all = all_raven_tcpip_source();
    for symbol in ["cong_signal", "rack_update_prr"] {
        assert!(
            !contains_fn(&all, symbol),
            "K359-F11: docs/rack-coverage.md marks K269 {symbol} PASS, but the symbol is absent"
        );
    }
    for test in [
        "rack_cong_signal_enters_recovery_and_sets_prr",
        "rack_rto_exits_recovery_and_rto_error_restores_it",
        "rack_ecn_signal_sets_recover_beyond_snd_max",
        "rack_pipe_and_prr_follow_the_recovery_state",
        "rack_prr_low_pipe_allows_two_segments",
        "rack_update_prr_high_pipe_uses_the_recovery_fraction",
    ] {
        assert!(
            !contains_fn(&all, test),
            "K359-F11: the canonical K269 row names {test}, which is absent"
        );
    }
}
