// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Source and behavioral checks for the K332/K333 audit.

use raven_tcpip::{
    RackPipe, RackSegment, RackSendmap, RackStack, RackVtable, TCPS_FIN_WAIT_1, TH_ACK,
    TLP_USE_TWO_ONE, rack_do_fin_wait_1,
};

/// One audited donor function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionSpec {
    /// Donor name.
    pub name: &'static str,
    /// Source file that implements it.
    pub file: &'static str,
    /// Named test.
    pub test: &'static str,
    /// Test file.
    pub test_file: &'static str,
}

/// K332 state/handler functions (18).
pub const STATE_FUNCTIONS: &[FunctionSpec] = &[
    FunctionSpec {
        name: "rack_do_syn_sent",
        file: "rack_state.rs",
        test: "rack_do_syn_sent_establishes_on_syn_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_syn_recv",
        file: "rack_state.rs",
        test: "rack_do_syn_recv_establishes_on_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_established",
        file: "rack_state.rs",
        test: "rack_do_established_clears_the_rto_override",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_fastnewdata",
        file: "rack_state.rs",
        test: "rack_do_fastnewdata_requires_established",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_close_wait",
        file: "rack_state.rs",
        test: "rack_do_close_wait_moves_to_last_ack_on_fin",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_fin_wait_1",
        file: "rack_state.rs",
        test: "rack_do_fin_wait_1_moves_to_fin_wait_2_on_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_fin_wait_2",
        file: "rack_state.rs",
        test: "rack_do_fin_wait_2_moves_to_time_wait_on_fin",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_closing",
        file: "rack_state.rs",
        test: "rack_do_closing_moves_to_time_wait_on_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_lastack",
        file: "rack_state.rs",
        test: "rack_do_lastack_closes_on_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_segment_nounlock",
        file: "rack_state.rs",
        test: "rack_do_segment_nounlock_ignores_closed",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_segment",
        file: "rack_state.rs",
        test: "rack_do_segment_dispatches_by_state",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_process_data",
        file: "rack_state.rs",
        test: "rack_process_data_arms_delayed_ack",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_process_option",
        file: "rack_state.rs",
        test: "rack_process_option_routes_the_option",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_handle_delayed_ack",
        file: "rack_state.rs",
        test: "rack_handle_delayed_ack_arms_only_once",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_handle_might_revert",
        file: "rack_state.rs",
        test: "rack_handle_might_revert_clears_the_override",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_handle_probe_response",
        file: "rack_state.rs",
        test: "rack_handle_probe_response_opens_the_window",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_win_updates",
        file: "rack_state.rs",
        test: "rack_do_win_updates_changes_the_window_once",
        test_file: "rack_state_ut.rs",
    },
    FunctionSpec {
        name: "rack_do_compressed_ack_processing",
        file: "rack_state.rs",
        test: "rack_do_compressed_ack_processing_drains_the_queue",
        test_file: "rack_state_ut.rs",
    },
];

/// K333 pipe/lost/recovery donor functions (9 K315 rows).
pub const PIPE_FUNCTIONS: &[FunctionSpec] = &[
    FunctionSpec {
        name: "do_rack_compute_pipe",
        file: "rack_pipe.rs",
        test: "do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_compute_pipe",
        file: "rack_pipe.rs",
        test: "do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_mark_lost",
        file: "rack_pipe.rs",
        test: "rack_mark_lost_marks_sack_passed_entries_after_the_threshold",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_mark_nolonger_lost",
        file: "rack_pipe.rs",
        test: "rack_mark_nolonger_lost_clears_the_flag_and_decrements_the_count",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_check_recovery_mode",
        file: "rack_pipe.rs",
        test: "rack_check_recovery_mode_finds_an_overdue_unacked_segment",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_post_recovery",
        file: "rack_pipe.rs",
        test: "rack_post_recovery_adds_back_prr_and_exits",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_exit_recovery",
        file: "rack_pipe.rs",
        test: "rack_exit_recovery_clears_both_recovery_flags",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_calc_thresh_rack",
        file: "rack_pipe.rs",
        test: "rack_calc_thresh_rack_follows_standard_reorder_and_dsack_rules",
        test_file: "rack_pipe_ut.rs",
    },
    FunctionSpec {
        name: "rack_calc_thresh_tlp",
        file: "rack_pipe.rs",
        test: "rack_calc_thresh_tlp_uses_the_donor_threshold_and_caps",
        test_file: "rack_pipe_ut.rs",
    },
];

/// Donor functions implemented outside the 27 assigned K315 rows.
pub const EXTRA_FUNCTIONS: &[FunctionSpec] = &[FunctionSpec {
    name: "rack_grab_rtt",
    file: "rack_pipe.rs",
    test: "rack_grab_rtt_prefers_the_rack_sample_then_srtt_then_initial",
    test_file: "rack_pipe_ut.rs",
}];

/// Exact values from `tcp_rack.h`, `tcp_var.h`, `tcp_fsm.h` and the rack globals.
pub const DONOR_CONSTANTS: &[(&str, i64)] = &[
    ("RACK_ACKED", 0x000001),
    ("RACK_TO_REXT", 0x000002),
    ("RACK_DEFERRED", 0x000004),
    ("RACK_OVERMAX", 0x000008),
    ("RACK_SACK_PASSED", 0x000010),
    ("RACK_WAS_SACKPASS", 0x000020),
    ("RACK_HAS_FIN", 0x000040),
    ("RACK_TLP", 0x000080),
    ("RACK_RWND_COLLAPSED", 0x000100),
    ("RACK_APP_LIMITED", 0x000200),
    ("RACK_WAS_ACKED", 0x000400),
    ("RACK_HAS_SYN", 0x000800),
    ("RACK_SENT_W_DSACK", 0x001000),
    ("RACK_SENT_SP", 0x002000),
    ("RACK_SENT_FP", 0x004000),
    ("RACK_HAD_PUSH", 0x008000),
    ("RACK_MUST_RXT", 0x010000),
    ("RACK_IN_GP_WIN", 0x020000),
    ("RACK_SHUFFLED", 0x040000),
    ("RACK_MERGED", 0x080000),
    ("RACK_PMTU_CHG", 0x100000),
    ("RACK_STRADDLE", 0x200000),
    ("RACK_WAS_LOST", 0x400000),
    ("RACK_IS_PCM", 0x800000),
    ("RACK_NUM_OF_RETRANS", 3),
    ("RACK_INITIAL_RTO", 1_000_000),
    ("TF_SENTFIN", 0x0000_0010),
    ("TF_FASTRECOVERY", 0x0010_0000),
    ("TF_WASFRECOVERY", 0x0020_0000),
    ("TF_CONGRECOVERY", 0x2000_0000),
    ("TF_WASCRECOVERY", 0x4000_0000),
    ("TF2_ECN_PERMIT", 0x0000_0020),
    ("TF2_ECN_SND_CWR", 0x0000_0040),
    ("TCPS_FIN_WAIT_1", 6),
    ("TCPS_CLOSING", 7),
    ("TCPS_LAST_ACK", 8),
    ("TCPS_FIN_WAIT_2", 9),
    ("TCPS_TIME_WAIT", 10),
    ("RACK_TLP_MIN", 10_000),
    ("RACK_RTO_MAX", 4_000_000),
    ("RACK_RTO_MIN", 30_000),
    ("RACK_DELAYED_ACK_TIME", 40_000),
    ("RACK_PRR_ADDBACKMAX", 2),
    ("RACK_REORDER_FADE_DEFAULT", 60_000_000),
    ("PACE_MAX_IP_BYTES", 65_536),
];

/// Finding class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The donor function is absent from the merged source.
    MissingFunction,
    /// The mapped named test is absent.
    MissingTest,
    /// Two donor functions share one test name.
    SharedTest,
    /// The same donor name is declared in more than one file.
    DuplicateConstant,
    /// A donor name has a value different from the donor.
    WrongConstant,
    /// A non-donor symbol is present (documented divergence or defect).
    NonDonorSymbol,
    /// A second send-map model exists beside the shared one.
    DuplicateSendmap,
    /// The merged arithmetic differs from the donor for an exact vector.
    BehaviourDivergence,
}

/// One audit finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Stable identifier.
    pub id: String,
    /// Finding class.
    pub kind: Kind,
    /// File or function.
    pub location: String,
    /// Exact evidence.
    pub detail: String,
}

/// The audit result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AuditReport {
    /// Findings in discovery order.
    pub findings: Vec<Finding>,
    /// Donor functions checked.
    pub functions_checked: usize,
    /// Named tests checked.
    pub tests_checked: usize,
    /// Donor constants checked.
    pub constants_checked: usize,
}

impl AuditReport {
    /// Findings of one class.
    #[must_use]
    pub fn of_kind(&self, kind: Kind) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.kind == kind)
            .collect()
    }

    /// Whether an exact finding id exists.
    #[must_use]
    pub fn has(&self, id: &str) -> bool {
        self.findings.iter().any(|finding| finding.id == id)
    }
}

/// Source texts the audit reads.
pub struct Sources<'a> {
    /// `rack_state.rs`.
    pub state: &'a str,
    /// `rack_pipe.rs`.
    pub pipe: &'a str,
    /// `rack_misc.rs`.
    pub misc: &'a str,
    /// `rack_retransmit.rs`.
    pub retransmit: &'a str,
    /// `rack_bbr.rs`.
    pub bbr: &'a str,
    /// `rack_output.rs`.
    pub output: &'a str,
    /// `rack_sack.rs`.
    pub sack: &'a str,
    /// `rack_state_ut.rs`.
    pub state_ut: &'a str,
    /// `rack_pipe_ut.rs`.
    pub pipe_ut: &'a str,
}

/// The merged product sources.
#[must_use]
pub fn live_sources() -> Sources<'static> {
    Sources {
        state: include_str!("../../../crates/raven-tcpip/src/rack_state.rs"),
        pipe: include_str!("../../../crates/raven-tcpip/src/rack_pipe.rs"),
        misc: include_str!("../../../crates/raven-tcpip/src/rack_misc.rs"),
        retransmit: include_str!("../../../crates/raven-tcpip/src/rack_retransmit.rs"),
        bbr: include_str!("../../../crates/raven-tcpip/src/rack_bbr.rs"),
        output: include_str!("../../../crates/raven-tcpip/src/rack_output.rs"),
        sack: include_str!("../../../crates/raven-tcpip/src/rack_sack.rs"),
        state_ut: include_str!("../../../crates/raven-tcpip/src/rack_state_ut.rs"),
        pipe_ut: include_str!("../../../crates/raven-tcpip/src/rack_pipe_ut.rs"),
    }
}

/// Audit the merged sources.
#[must_use]
pub fn audit() -> AuditReport {
    audit_sources(&live_sources())
}

/// Audit one source set.
#[must_use]
pub fn audit_sources(sources: &Sources<'_>) -> AuditReport {
    let mut report = AuditReport::default();
    let mut tests: Vec<(&str, &str)> = Vec::new();
    let all = chain(&[STATE_FUNCTIONS, PIPE_FUNCTIONS, EXTRA_FUNCTIONS]);
    for spec in &all {
        report.functions_checked += 1;
        if !has_function(source_for(sources, spec.file), spec.name) {
            report.findings.push(Finding {
                id: format!("missing-function:{}", spec.name),
                kind: Kind::MissingFunction,
                location: spec.file.to_string(),
                detail: format!("donor function `{}` is absent", spec.name),
            });
        }
        tests.push((spec.test, spec.name));
        if !has_function(test_source_for(sources, spec.test_file), spec.test) {
            report.findings.push(Finding {
                id: format!("missing-test:{}", spec.name),
                kind: Kind::MissingTest,
                location: spec.test_file.to_string(),
                detail: format!("named test `{}` is absent", spec.test),
            });
        }
        report.tests_checked += 1;
    }
    for (test, name) in &tests {
        let users: Vec<&str> = tests
            .iter()
            .filter(|(candidate, _)| candidate == test)
            .map(|(_, function)| *function)
            .collect();
        if users.len() > 1 && users.first() == Some(name) {
            report.findings.push(Finding {
                id: format!("shared-test:{test}"),
                kind: Kind::SharedTest,
                location: "rack_state_ut.rs/rack_pipe_ut.rs".to_string(),
                detail: format!("test `{test}` covers {users:?}"),
            });
        }
    }
    if has_function(sources.pipe, "rack_enter_recovery")
        && !all.iter().any(|spec| spec.name == "rack_enter_recovery")
    {
        report.findings.push(Finding {
            id: "non-donor:rack_enter_recovery".to_string(),
            kind: Kind::NonDonorSymbol,
            location: "rack_pipe.rs".to_string(),
            detail:
                "no rack.c symbol; models rack_cong_signal(CC_NDUPACK); documented in K333 report"
                    .to_string(),
        });
    }

    let constants = collect_constants(sources);
    report.constants_checked = constants.len();
    for (name, expected) in DONOR_CONSTANTS {
        for constant in constants
            .iter()
            .filter(|constant| constant.name.as_str() == *name)
        {
            if constant.value != *expected {
                report.findings.push(Finding {
                    id: format!("wrong:{name}"),
                    kind: Kind::WrongConstant,
                    location: format!("{}:{}", constant.file, constant.name),
                    detail: format!(
                        "donor {expected} (0x{expected:x}), merged {} (0x{:x})",
                        constant.value, constant.value
                    ),
                });
            }
        }
    }
    let mut seen: Vec<&str> = Vec::new();
    for constant in &constants {
        if seen.contains(&constant.name.as_str()) {
            continue;
        }
        seen.push(constant.name.as_str());
        let same: Vec<&Constant> = constants
            .iter()
            .filter(|candidate| candidate.name == constant.name)
            .collect();
        let files: Vec<&str> = same.iter().map(|candidate| candidate.file).collect();
        if files.len() > 1 {
            let values: Vec<i64> = same.iter().map(|candidate| candidate.value).collect();
            report.findings.push(Finding {
                id: format!("duplicate:{}", constant.name),
                kind: Kind::DuplicateConstant,
                location: files.join(","),
                detail: format!("declared {} times with values {values:?}", files.len()),
            });
        }
    }
    if sources.misc.contains("pub struct RackSendmap")
        && sources.retransmit.contains("pub struct RackSendMap")
    {
        report.findings.push(Finding {
            id: "duplicate-sendmap".to_string(),
            kind: Kind::DuplicateSendmap,
            location: "rack_misc.rs,rack_retransmit.rs".to_string(),
            detail: "RackSendmap and RackSendMap/Rsm model the same donor rack_sendmap".to_string(),
        });
    }
    let (donor, actual) = tlp_gap_vector();
    if donor != actual {
        report.findings.push(Finding {
            id: "tlp-gap-index".to_string(),
            kind: Kind::BehaviourDivergence,
            location: "rack_pipe.rs:rack_calc_thresh_tlp".to_string(),
            detail: format!(
                "donor inter_gap uses rsm[idx]-prev[nidx]; merged uses rsm[nidx]-prev[idx]: donor {donor}, merged {actual}"
            ),
        });
    }
    let (donor, actual) = fin_wait_vector();
    if donor != actual {
        report.findings.push(Finding {
            id: "fin-wait-without-fin".to_string(),
            kind: Kind::BehaviourDivergence,
            location: "rack_state.rs:rack_do_fin_wait_1".to_string(),
            detail: format!(
                "donor requires ourfinisacked (TF_SENTFIN+full ACK) before FIN_WAIT_2; merged moves on snd_una==snd_max: donor state {donor}, merged state {actual}"
            ),
        });
    }
    report
}

/// Donor and merged state for an ACK in FIN_WAIT_1 when no FIN was sent.
#[must_use]
pub fn fin_wait_vector() -> (u8, u8) {
    let mut stack = RackStack::new();
    if stack.tfb_tcp_fb_init().is_err() {
        return (TCPS_FIN_WAIT_1, TCPS_FIN_WAIT_1);
    }
    stack.tp.t_state = TCPS_FIN_WAIT_1;
    stack.tp.snd_una = 100;
    stack.tp.snd_nxt = 200;
    stack.tp.snd_max = 200;
    stack.tp.snd_cwnd = 10_000;
    stack.tp.t_maxseg = 1_000;
    stack.tp.t_flags = 0;
    stack.snd_wnd = 65_535;
    let segment = RackSegment {
        seq: 100,
        ack: 200,
        flags: TH_ACK,
        payload_len: 0,
        window: 65_535,
        ecn: 0,
    };
    let _ = rack_do_fin_wait_1(&mut stack, &segment);
    (TCPS_FIN_WAIT_1, stack.tp.t_state)
}

/// Donor and merged values for the TLP TWO_ONE inter-packet-gap vector.
///
/// The donor uses `idx = rsm.r_rtr_cnt - 1` and `nidx = prsm.r_rtr_cnt - 1`,
/// compares `rsm[nidx] >= prsm[idx]`, then subtracts
/// `rsm[idx] - prsm[nidx]`. With the two entries below the donor threshold is
/// `srtt + srtt/1 + 4000 = 6000` µs.
#[must_use]
pub fn tlp_gap_vector() -> (u32, u32) {
    let mut pipe = RackPipe::new();
    pipe.stack.tp.t_maxseg = 1_000;
    pipe.stack.pace_min_segs = 1_000;
    pipe.stack.tp.t_rxtcur = 1_000_000;
    pipe.state.tlp_threshold = 1;
    pipe.state.tlp_threshold_use = TLP_USE_TWO_ONE;
    pipe.state.tlp_min = 0;
    pipe.state.rto_max = 4_000_000;
    let mut previous = RackSendmap::new(2_000, 3_000, 1_000);
    previous.rtr_cnt = 1;
    previous.tim_lastsent = [1_000, 0, 0];
    let mut current = RackSendmap::new(3_000, 4_000, 2_000);
    current.rtr_cnt = 2;
    current.tim_lastsent = [2_000, 5_000, 0];
    pipe.push_sendmap(previous);
    pipe.push_sendmap(current);
    (6_000, pipe.rack_calc_thresh_tlp(1, 1_000))
}

#[derive(Clone, Debug)]
struct Constant {
    file: &'static str,
    name: String,
    value: i64,
}

fn collect_constants(sources: &Sources<'_>) -> Vec<Constant> {
    let files = [
        ("rack_state.rs", sources.state),
        ("rack_pipe.rs", sources.pipe),
        ("rack_misc.rs", sources.misc),
        ("rack_retransmit.rs", sources.retransmit),
        ("rack_bbr.rs", sources.bbr),
        ("rack_output.rs", sources.output),
        ("rack_sack.rs", sources.sack),
    ];
    let mut constants = Vec::new();
    for (file, text) in files {
        for line in text.lines() {
            let Some(rest) = line.trim().strip_prefix("pub const ") else {
                continue;
            };
            let Some((name, rest)) = rest.split_once(':') else {
                continue;
            };
            let Some((_, value)) = rest.split_once('=') else {
                continue;
            };
            let Some(value) = parse_int(value) else {
                continue;
            };
            constants.push(Constant {
                file,
                name: name.trim().to_string(),
                value,
            });
        }
    }
    constants
}

fn parse_int(text: &str) -> Option<i64> {
    let text = text.split("//").next()?.trim().trim_end_matches(';').trim();
    let text = text.replace('_', "");
    if let Some(hex) = text.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()
    } else {
        text.parse().ok()
    }
}

fn has_function(text: &str, name: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start();
        line.starts_with(&format!("pub fn {name}(")) || line.starts_with(&format!("fn {name}("))
    })
}

fn source_for<'a>(sources: &Sources<'a>, file: &str) -> &'a str {
    match file {
        "rack_state.rs" => sources.state,
        "rack_pipe.rs" => sources.pipe,
        "rack_misc.rs" => sources.misc,
        "rack_retransmit.rs" => sources.retransmit,
        "rack_bbr.rs" => sources.bbr,
        "rack_output.rs" => sources.output,
        "rack_sack.rs" => sources.sack,
        _ => "",
    }
}

fn test_source_for<'a>(sources: &Sources<'a>, file: &str) -> &'a str {
    match file {
        "rack_state_ut.rs" => sources.state_ut,
        "rack_pipe_ut.rs" => sources.pipe_ut,
        _ => "",
    }
}

fn chain<'a>(groups: &[&'a [FunctionSpec]]) -> Vec<&'a FunctionSpec> {
    groups.iter().flat_map(|group| group.iter()).collect()
}
