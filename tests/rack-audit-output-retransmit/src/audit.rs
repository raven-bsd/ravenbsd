// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Source/test audit for the merged K330 output/timer and K331 ACK/retransmit groups.
//!
//! The auditor is deliberately read-only: it parses the merged sources and unit tests and
//! records findings. It never edits product code; findings are reported to the group owners.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// The merged module that owns a function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Module {
    /// `rack_output.rs` (K330).
    Output,
    /// `rack_retransmit.rs` (K331).
    Retransmit,
}

impl Module {
    /// Source file name.
    #[must_use]
    pub const fn source(self) -> &'static str {
        match self {
            Self::Output => "rack_output.rs",
            Self::Retransmit => "rack_retransmit.rs",
        }
    }

    /// Unit-test file name.
    #[must_use]
    pub const fn tests(self) -> &'static str {
        match self {
            Self::Output => "rack_output_ut.rs",
            Self::Retransmit => "rack_retransmit_ut.rs",
        }
    }
}

/// One expected donor function and its merged mapping/test/mutation row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Expected {
    /// Donor function name.
    pub function: &'static str,
    /// Owning merged module.
    pub module: Module,
    /// Named unit test that must exist.
    pub test: &'static str,
    /// Mutation/negative description; empty values are findings.
    pub mutation: &'static str,
}

/// The 34 functions owned by K330 and K331.
pub const EXPECTED: [Expected; 34] = [
    Expected {
        function: "rack_output",
        module: Module::Output,
        test: "rack_output_sends_new_data_and_arms_rxt",
        mutation: "omit SND.MAX advance",
    },
    Expected {
        function: "tcp_rack_output",
        module: Module::Output,
        test: "tcp_rack_output_returns_default_ok_and_candrop",
        mutation: "closed state returns CANDROP",
    },
    Expected {
        function: "rack_timer_start",
        module: Module::Output,
        test: "rack_timer_start_and_rack_timer_cancel",
        mutation: "slot not returned",
    },
    Expected {
        function: "rack_timer_cancel",
        module: Module::Output,
        test: "rack_timer_start_and_rack_timer_cancel",
        mutation: "pending bit not cleared",
    },
    Expected {
        function: "rack_timer_audit",
        module: Module::Output,
        test: "rack_timer_audit_returns_due_bits_only",
        mutation: "early bits reported",
    },
    Expected {
        function: "rack_timeout_rack",
        module: Module::Output,
        test: "rack_timeout_rack_requests_retransmit",
        mutation: "r_must_retran not set",
    },
    Expected {
        function: "rack_timeout_tlp",
        module: Module::Output,
        test: "rack_timeout_tlp_is_bounded_to_two_probes",
        mutation: "third TLP probe allowed",
    },
    Expected {
        function: "rack_timeout_delack",
        module: Module::Output,
        test: "rack_timeout_delack_sets_acknow",
        mutation: "ACKNOW not set",
    },
    Expected {
        function: "rack_timeout_persist",
        module: Module::Output,
        test: "rack_timeout_persist_probes_and_rearms",
        mutation: "probe not sent",
    },
    Expected {
        function: "rack_timeout_keepalive",
        module: Module::Output,
        test: "rack_timeout_keepalive_counts_and_closes",
        mutation: "keepalive never exhausts",
    },
    Expected {
        function: "rack_timeout_rxt",
        module: Module::Output,
        test: "rack_timeout_rxt_backs_off_and_closes_at_the_limit",
        mutation: "RXT never closes",
    },
    Expected {
        function: "rack_remxt_tmr",
        module: Module::Output,
        test: "rack_remxt_tmr_sets_override_and_backoff",
        mutation: "backoff shift not moved",
    },
    Expected {
        function: "rack_stopall",
        module: Module::Output,
        test: "rack_stopall_refuses_future_timers",
        mutation: "future timer accepted",
    },
    Expected {
        function: "rack_stop_all_timers",
        module: Module::Output,
        test: "rack_stop_all_timers_reports_persist",
        mutation: "persist flag dropped",
    },
    Expected {
        function: "rack_set_state",
        module: Module::Output,
        test: "rack_set_state_updates_timers_and_rack_handoff_ok_checks_it",
        mutation: "closed state does not stop timers",
    },
    Expected {
        function: "rack_handoff_ok",
        module: Module::Output,
        test: "rack_set_state_updates_timers_and_rack_handoff_ok_checks_it",
        mutation: "FIN handoff accepted",
    },
    Expected {
        function: "rack_enter_persist",
        module: Module::Output,
        test: "rack_output_enters_persist_on_zero_window",
        mutation: "persist never entered",
    },
    Expected {
        function: "rack_exit_persist",
        module: Module::Output,
        test: "rack_enter_persist_and_rack_exit_persist",
        mutation: "second exit accepted",
    },
    Expected {
        function: "rack_start_hpts_timer",
        module: Module::Output,
        test: "rack_start_hpts_timer_arms_a_slot",
        mutation: "slot not armed",
    },
    Expected {
        function: "rack_process_timers",
        module: Module::Output,
        test: "rack_process_timers_dispatches_in_order",
        mutation: "dispatch order changed",
    },
    Expected {
        function: "rack_get_persists_timer_val",
        module: Module::Output,
        test: "rack_get_persists_timer_val_is_bounded",
        mutation: "clamp removed",
    },
    Expected {
        function: "rack_ack_received",
        module: Module::Retransmit,
        test: "rack_ack_received_applies_abc_and_the_early_recovery_clamp",
        mutation: "early clamp disabled",
    },
    Expected {
        function: "rack_process_ack",
        module: Module::Retransmit,
        test: "rack_process_ack_advances_una_and_exits_recovery",
        mutation: "finish_recovery removed",
    },
    Expected {
        function: "rack_update_entry",
        module: Module::Retransmit,
        test: "rack_update_entry_splits_partial_and_reports_overhang",
        mutation: "split returns None",
    },
    Expected {
        function: "rack_update_rsm",
        module: Module::Retransmit,
        test: "rack_update_rsm_counts_retransmits_and_moves_time_order",
        mutation: "r_rtr_cnt not incremented",
    },
    Expected {
        function: "rack_update_seg",
        module: Module::Retransmit,
        test: "rack_update_seg_keeps_the_pacing_minimum_at_the_mss",
        mutation: "pace minimum dropped",
    },
    Expected {
        function: "rack_update_rtt",
        module: Module::Retransmit,
        test: "rack_update_rtt_tracks_the_minimum_and_rejects_spurious_samples",
        mutation: "spurious sample accepted",
    },
    Expected {
        function: "rack_what_can_we_send",
        module: Module::Retransmit,
        test: "rack_what_can_we_send_is_limited_by_cwnd_rwnd_and_avail",
        mutation: "rwnd cap removed",
    },
    Expected {
        function: "rack_find_lowest_rsm",
        module: Module::Retransmit,
        test: "rack_find_lowest_rsm_skips_acked_entries",
        mutation: "ACKed entry selected",
    },
    Expected {
        function: "rack_find_high_nonack",
        module: Module::Retransmit,
        test: "rack_find_high_nonack_skips_acked_and_fin_segments",
        mutation: "FIN not skipped",
    },
    Expected {
        function: "rack_strike_dupack",
        module: Module::Retransmit,
        test: "rack_strike_dupack_retransmits_on_the_third_dupack",
        mutation: "threshold > 3",
    },
    Expected {
        function: "rack_fastack",
        module: Module::Retransmit,
        test: "rack_fastack_accepts_only_clean_advancing_acks",
        mutation: "recovery ACK accepted",
    },
    Expected {
        function: "rack_init_retransmit_value",
        module: Module::Retransmit,
        test: "rack_init_retransmit_value_selects_the_three_modes",
        mutation: "control modes swapped",
    },
    Expected {
        function: "rack_fast_rsm_output",
        module: Module::Retransmit,
        test: "rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe",
        mutation: "PRR not decremented",
    },
];

/// One expected donor constant and its exact value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExpectedConstant {
    /// Owning module.
    pub module: Module,
    /// Constant name.
    pub name: &'static str,
    /// Exact donor value, with underscores removed.
    pub value: &'static str,
}

/// Donor constants for the output/retransmit group.
pub const CONSTANTS: [ExpectedConstant; 19] = [
    ExpectedConstant {
        module: Module::Output,
        name: "TLP_MAX_PROBES",
        value: "2",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "KEEP_MAX_PROBES",
        value: "9",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "RXT_MAX_SHIFT",
        value: "12",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "KEEPIDLE_US",
        value: "7200000",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "KEEPINTVL_US",
        value: "75000",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "PERSIST_MIN_US",
        value: "250000",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "PERSIST_MAX_US",
        value: "1000000",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "TCPS_FIN_WAIT_1",
        value: "6",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "TCPS_FIN_WAIT_2",
        value: "7",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "TCPS_CLOSING",
        value: "8",
    },
    ExpectedConstant {
        module: Module::Output,
        name: "TCPS_LAST_ACK",
        value: "9",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "RACK_NUM_OF_RETRANS",
        value: "3",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "DUP_ACK_THRESHOLD",
        value: "3",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "RACK_RTO_MIN",
        value: "30000",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "RACK_RTO_MAX",
        value: "4000000",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "TCPS_FIN_WAIT_1",
        value: "6",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "TCPS_FIN_WAIT_2",
        value: "7",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "TCPS_CLOSING",
        value: "8",
    },
    ExpectedConstant {
        module: Module::Retransmit,
        name: "TCPS_LAST_ACK",
        value: "9",
    },
];

/// A finding class.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindingKind {
    /// The Rust function is absent.
    MissingMapping,
    /// The named test is absent.
    MissingTest,
    /// The mutation column is empty/weak.
    EmptyMutation,
    /// The constant differs from the donor.
    WrongConstant,
    /// The same constant is defined by both modules.
    DuplicateConstant,
    /// Two duplicate constants have different values.
    Conflict,
}

/// One audit finding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Finding {
    /// Finding class.
    pub kind: FindingKind,
    /// Function or constant name.
    pub name: String,
    /// Human-readable detail.
    pub detail: String,
}

/// The full audit result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Audit {
    /// Findings.
    pub findings: Vec<Finding>,
    /// Functions with a Rust mapping.
    pub mapped: usize,
    /// Functions with a named test.
    pub tested: usize,
    /// Functions with a non-empty mutation row.
    pub mutated: usize,
}

impl Audit {
    /// Findings of one kind.
    #[must_use]
    pub fn of_kind(&self, kind: FindingKind) -> Vec<&Finding> {
        self.findings
            .iter()
            .filter(|finding| finding.kind == kind)
            .collect()
    }

    /// Whether no finding was recorded.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// Audit in-memory sources; used by the real audit and the negative fixtures.
#[must_use]
pub fn audit_sources(
    output_source: &str,
    retransmit_source: &str,
    output_tests: &str,
    retransmit_tests: &str,
) -> Audit {
    let mut audit = Audit::default();
    for expected in EXPECTED {
        let source = match expected.module {
            Module::Output => output_source,
            Module::Retransmit => retransmit_source,
        };
        let tests = match expected.module {
            Module::Output => output_tests,
            Module::Retransmit => retransmit_tests,
        };
        if source.contains(&format!("fn {}(", expected.function)) {
            audit.mapped += 1;
        } else {
            audit.findings.push(Finding {
                kind: FindingKind::MissingMapping,
                name: expected.function.to_owned(),
                detail: format!(
                    "{} not found in {}",
                    expected.function,
                    expected.module.source()
                ),
            });
        }
        if tests.contains(&format!("fn {}(", expected.test)) {
            audit.tested += 1;
        } else {
            audit.findings.push(Finding {
                kind: FindingKind::MissingTest,
                name: expected.function.to_owned(),
                detail: format!("{} test {} not found", expected.function, expected.test),
            });
        }
        if is_weak_mutation(expected.mutation) {
            audit.findings.push(Finding {
                kind: FindingKind::EmptyMutation,
                name: expected.function.to_owned(),
                detail: format!("{} has no mutation/negative check", expected.function),
            });
        } else {
            audit.mutated += 1;
        }
    }

    let output_constants = constants(output_source);
    let retransmit_constants = constants(retransmit_source);
    for expected in CONSTANTS {
        let map = match expected.module {
            Module::Output => &output_constants,
            Module::Retransmit => &retransmit_constants,
        };
        match map.get(expected.name) {
            Some(actual) if actual == expected.value => {}
            Some(actual) => audit.findings.push(Finding {
                kind: FindingKind::WrongConstant,
                name: expected.name.to_owned(),
                detail: format!(
                    "{} is {} in {}, donor is {}",
                    expected.name,
                    actual,
                    expected.module.source(),
                    expected.value
                ),
            }),
            None => audit.findings.push(Finding {
                kind: FindingKind::WrongConstant,
                name: expected.name.to_owned(),
                detail: format!(
                    "{} missing from {}",
                    expected.name,
                    expected.module.source()
                ),
            }),
        }
    }

    let mut seen: BTreeMap<&str, (&str, &str)> = BTreeMap::new();
    for (module, map) in [
        (Module::Output, &output_constants),
        (Module::Retransmit, &retransmit_constants),
    ] {
        for (name, value) in map {
            if let Some((first_module, first_value)) = seen.get(name.as_str()) {
                audit.findings.push(Finding {
                    kind: FindingKind::DuplicateConstant,
                    name: name.clone(),
                    detail: format!(
                        "{} is defined in {} and {}",
                        name,
                        first_module,
                        module.source()
                    ),
                });
                if first_value != value {
                    audit.findings.push(Finding {
                        kind: FindingKind::Conflict,
                        name: name.clone(),
                        detail: format!(
                            "{} is {} in {} but {} in {}",
                            name,
                            first_value,
                            first_module,
                            value,
                            module.source()
                        ),
                    });
                }
            } else {
                seen.insert(name.as_str(), (module.source(), value.as_str()));
            }
        }
    }
    audit
}

fn is_weak_mutation(mutation: &str) -> bool {
    let mutation = mutation.trim();
    mutation.is_empty()
        || matches!(
            mutation.to_ascii_uppercase().as_str(),
            "-" | "GAP" | "NONE" | "N/A" | "TODO" | "UNMAPPED"
        )
}

fn constants(source: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in source.lines() {
        let line = line.trim();
        let rest = line
            .strip_prefix("pub const ")
            .or_else(|| line.strip_prefix("const "));
        let Some(rest) = rest else {
            continue;
        };
        let Some((name, tail)) = rest.split_once(':') else {
            continue;
        };
        let Some((_, value)) = tail.split_once('=') else {
            continue;
        };
        let value = value.split(';').next().unwrap_or_default().trim();
        map.insert(name.trim().to_owned(), value.replace(['_', ' '], ""));
    }
    map
}

/// Read the merged sources/tests under `root` and audit them.
#[must_use]
pub fn audit(root: &Path) -> Audit {
    let read = |name: &str| {
        std::fs::read_to_string(root.join("crates/raven-tcpip/src").join(name)).unwrap_or_default()
    };
    audit_sources(
        &read(Module::Output.source()),
        &read(Module::Retransmit.source()),
        &read(Module::Output.tests()),
        &read(Module::Retransmit.tests()),
    )
}

/// The expected function-name set, sorted and unique.
#[must_use]
pub fn function_names() -> BTreeSet<&'static str> {
    EXPECTED.iter().map(|expected| expected.function).collect()
}
