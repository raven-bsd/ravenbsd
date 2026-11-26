// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 218/218 acceptance audit.
//!
//! The audit requires a canonical owner row, a Rust mapping that exists in the current
//! `raven-tcpip` source, a named `#[test]` and a mutation/negative check. It also re-runs the
//! K337 replay and reports the first mismatch per class. A row is never accepted on report text
//! alone.

use std::collections::{BTreeMap, BTreeSet};

use rack_coverage::{Function, Report as K315Report, Row, check};
use rack_replay::replay::{ClassReport, ReplayReport};

use crate::register::RegisterRow;
use crate::reports::GroupRow;
use crate::source::SourceIndex;

/// Ticket acceptance denominator. The pinned donor is 219; the delta is reported, not hidden.
pub const TICKET_TARGET: usize = 218;

/// One audited donor row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RowAudit {
    /// Donor line.
    pub number: usize,
    /// Donor function.
    pub function: String,
    /// Owning ticket.
    pub owner: String,
    /// Rust mapping found/claimed.
    pub rust: String,
    /// Named test found/claimed.
    pub test: String,
    /// Mutation found/claimed.
    pub mutation: String,
    /// Register status (`PASS` or `GAP K269`).
    pub status: String,
    /// The Rust mapping exists in the product source.
    pub mapping_found: bool,
    /// A named test exists in the product source.
    pub test_found: bool,
    /// A per-row mutation/negative check is recorded.
    pub mutation_present: bool,
    /// Group reports that claim this function.
    pub groups: Vec<String>,
    /// All four acceptance facts hold.
    pub pass: bool,
}

/// One acceptance gap.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gap {
    /// Donor function.
    pub function: String,
    /// Owning ticket.
    pub owner: String,
    /// Missing pieces.
    pub reasons: Vec<String>,
}

/// Complete acceptance result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptanceReport {
    /// Pinned donor count.
    pub donor_total: usize,
    /// Canonical register rows.
    pub register_total: usize,
    /// Ticket denominator.
    pub ticket_target: usize,
    /// Rows accepted.
    pub accepted: usize,
    /// Exact row status.
    pub rows: Vec<RowAudit>,
    /// Donor functions with no register row.
    pub missing: Vec<String>,
    /// Functions claimed by more than one group with a substantive mapping.
    pub duplicates: Vec<String>,
    /// Register rows not in the pinned donor.
    pub unknown: Vec<String>,
    /// Per-row gaps.
    pub gaps: Vec<Gap>,
    /// K315 checker result over the canonical register text.
    pub k315: K315Report,
    /// K337 replay matrix.
    pub replay: ReplayReport,
}

impl AcceptanceReport {
    /// A row is accepted only when the owner, mapping, test and mutation all exist.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty()
            && self.duplicates.is_empty()
            && self.unknown.is_empty()
            && self.accepted == self.donor_total
            && self.k315.empty_rust.is_empty()
            && self.k315.empty_test.is_empty()
            && self.k315.empty_mutation.is_empty()
            && self.k315.not_pass.is_empty()
    }

    /// Render the summary, owner split, gaps and replay matrix.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = format!(
            "acceptance: donors={} register={} target={} accepted={} gaps={} missing={} \
             duplicate={} unknown={}\n",
            self.donor_total,
            self.register_total,
            self.ticket_target,
            self.accepted,
            self.gaps.len(),
            self.missing.len(),
            self.duplicates.len(),
            self.unknown.len()
        );
        text.push_str(&format!(
            "k315: missing={} duplicate={} unknown={} empty_rust={} empty_test={} \
             empty_mutation={} not_pass={}\n",
            self.k315.missing.len(),
            self.k315.duplicates.len(),
            self.k315.unknown.len(),
            self.k315.empty_rust.len(),
            self.k315.empty_test.len(),
            self.k315.empty_mutation.len(),
            self.k315.not_pass.len()
        ));
        if self.donor_total != self.ticket_target {
            text.push_str(&format!(
                "target note: ticket text says {} but the pinned donor/manifest has {}; the \
                 extra row is reported, not dropped\n",
                self.ticket_target, self.donor_total
            ));
            if let Some(last) = self.rows.last() {
                let view = self.accepted - usize::from(last.pass);
                text.push_str(&format!(
                    "218-row view: {}/{} if the one-row discrepancy excludes {} ({})\n",
                    view,
                    self.ticket_target,
                    last.function,
                    if last.pass { "PASS" } else { "not PASS" }
                ));
            }
        }
        text.push_str(&owner_summary(&self.rows));
        for gap in &self.gaps {
            text.push_str(&format!(
                "gap {} ({}): {}\n",
                gap.function,
                gap.owner,
                gap.reasons.join(", ")
            ));
        }
        for duplicate in &self.duplicates {
            text.push_str(&format!("duplicate: {duplicate}\n"));
        }
        for unknown in &self.unknown {
            text.push_str(&format!("unknown: {unknown}\n"));
        }
        text.push_str("\nreplay:\n");
        text.push_str(&self.replay.render());
        text
    }

    /// Render the exact per-row status.
    #[must_use]
    pub fn render_rows(&self) -> String {
        let mut text = String::from(
            "row\tfunction\towner\tstatus\trust\ttest\tmutation\tmapping\ttest_ok\tmut_ok\tpass\n",
        );
        for row in &self.rows {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                row.number,
                row.function,
                row.owner,
                row.status,
                row.rust,
                row.test,
                row.mutation,
                row.mapping_found,
                row.test_found,
                row.mutation_present,
                row.pass
            ));
        }
        text
    }
}

/// Run the acceptance audit.
#[must_use]
pub fn run(
    donor: &[Function],
    register: &[RegisterRow],
    source: &SourceIndex,
    group_rows: &[GroupRow],
) -> AcceptanceReport {
    let donor_names: BTreeSet<&str> = donor
        .iter()
        .map(|function| function.name.as_str())
        .collect();
    let register_names: BTreeSet<&str> = register.iter().map(|row| row.function.as_str()).collect();
    let missing: Vec<String> = donor
        .iter()
        .filter(|function| !register_names.contains(function.name.as_str()))
        .map(|function| function.name.clone())
        .collect();
    let unknown: Vec<String> = register
        .iter()
        .filter(|row| !donor_names.contains(row.function.as_str()))
        .map(|row| row.function.clone())
        .collect();

    // The register is the reconciled union. Group reports are historical inputs and may only
    // fill functions that are absent from the register; they never override or duplicate a
    // canonical row.
    let mut by_function: BTreeMap<&str, Vec<&GroupRow>> = BTreeMap::new();
    for row in group_rows {
        if !register_names.contains(row.function.as_str()) {
            by_function
                .entry(row.function.as_str())
                .or_default()
                .push(row);
        }
    }

    let mut register_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for row in register {
        *register_counts.entry(row.function.as_str()).or_default() += 1;
    }
    let mut duplicates: Vec<String> = register_counts
        .iter()
        .filter(|(_, count)| **count > 1)
        .map(|(name, _)| (*name).to_owned())
        .collect();
    duplicates.sort();

    let mut audits = Vec::new();
    for row in register {
        let claims: Vec<GroupRow> = by_function
            .get(row.function.as_str())
            .map(|rows| rows.iter().map(|row| (*row).clone()).collect())
            .unwrap_or_default();
        let owner_id = row.owner.split_whitespace().next().unwrap_or_default();
        let owner_claims: Vec<&GroupRow> = claims
            .iter()
            .filter(|claim| claim.group == owner_id)
            .collect();
        let other_claims: Vec<&GroupRow> = claims
            .iter()
            .filter(|claim| claim.group != owner_id)
            .collect();
        let existing =
            |claim: &&GroupRow| !placeholder(&claim.rust) && source.symbols_exist(&claim.rust);
        // Prefer the canonical owner's row. If the owner's historical `Rack::*` name no longer
        // exists, accept another group's merged symbol for the same donor function. K269's
        // historical matrix is excluded as a fallback because it still carries the pre-merge
        // `Rack::*` names. K334 has no coverage table, so its register row remains the source
        // of truth unless another group has the actual merged symbol.
        let fallback = || {
            other_claims
                .iter()
                .copied()
                .find(|claim| claim.group != "K269" && existing(claim))
                .or_else(|| {
                    other_claims
                        .iter()
                        .copied()
                        .find(|claim| claim.group != "K269" && !placeholder(&claim.rust))
                })
        };
        let chosen: Option<&GroupRow> = if owner_claims.is_empty() {
            fallback()
        } else {
            owner_claims
                .iter()
                .copied()
                .find(existing)
                .or_else(fallback)
                .or_else(|| owner_claims.first().copied())
        };
        let claim_rust = chosen.map(|claim| claim.rust.as_str()).unwrap_or_default();
        let claim_test = chosen.map(|claim| claim.test.as_str()).unwrap_or_default();
        let claim_mutation = chosen
            .map(|claim| claim.mutation.as_str())
            .unwrap_or_default();
        // The group report is newer than the canonical owner map; prefer its mapping and fall
        // back to the register only when the group has no claim.
        let rust = first_substantive(&[claim_rust, row.rust.as_str()]);
        let test = first_substantive(&[claim_test, row.test.as_str()]);
        let mutation = first_substantive(&[claim_mutation, row.mutation.as_str()]);

        let mapping_found = if !placeholder(&rust) {
            source.symbols_exist(&rust)
        } else {
            source.has_mapping_for(&row.function)
        };
        let test_found = if !placeholder(&test) {
            test_names(&test)
                .iter()
                .any(|name| source.has_test_named(name))
                || source.has_test_for(&row.function)
        } else {
            source.has_test_for(&row.function)
        };
        let mutation_present = !placeholder(&mutation);
        let status_pass = row.status.trim().eq_ignore_ascii_case("PASS");
        let pass = owner_known(&row.owner)
            && mapping_found
            && test_found
            && mutation_present
            && status_pass;
        let mut groups: Vec<String> = claims.iter().map(|claim| claim.group.clone()).collect();
        groups.sort();
        groups.dedup();
        audits.push(RowAudit {
            number: row.number,
            function: row.function.clone(),
            owner: row.owner.clone(),
            rust,
            test,
            mutation,
            status: row.status.clone(),
            mapping_found,
            test_found,
            mutation_present,
            groups,
            pass,
        });
    }

    let gaps: Vec<Gap> = audits
        .iter()
        .filter(|row| !row.pass)
        .map(|row| {
            let mut reasons = Vec::new();
            if !owner_known(&row.owner) {
                reasons.push("no owner".to_owned());
            }
            if !row.mapping_found {
                reasons.push("no Rust mapping".to_owned());
            }
            if !row.test_found {
                reasons.push("no named test".to_owned());
            }
            if !row.mutation_present {
                reasons.push("no mutation".to_owned());
            }
            if !row.status.trim().eq_ignore_ascii_case("PASS") {
                reasons.push(format!("status {}", row.status));
            }
            Gap {
                function: row.function.clone(),
                owner: row.owner.clone(),
                reasons,
            }
        })
        .collect();

    let k315_rows: Vec<Row> = register
        .iter()
        .map(|row| Row {
            function: row.function.clone(),
            rust: row.rust.clone(),
            test: row.test.clone(),
            mutation: row.mutation.clone(),
            status: row.status.clone(),
        })
        .collect();
    let k315 = check(donor, &k315_rows);

    let replay = rack_replay::replay::replay_all().unwrap_or_else(|error| ReplayReport {
        classes: vec![ClassReport {
            class: rack_replay::replay::Class::Loss,
            traces: 0,
            events: 0,
            mismatches: 0,
            first_mismatch: Some(rack_replay::replay::Mismatch {
                trace: "replay".to_owned(),
                expected: "replay matrix".to_owned(),
                actual: error,
            }),
            samples: Vec::new(),
            gaps: Vec::new(),
        }],
    });

    AcceptanceReport {
        donor_total: donor.len(),
        register_total: register.len(),
        ticket_target: TICKET_TARGET,
        accepted: audits.iter().filter(|row| row.pass).count(),
        rows: audits,
        missing,
        duplicates,
        unknown,
        gaps,
        k315,
        replay,
    }
}

fn owner_summary(rows: &[RowAudit]) -> String {
    let mut counts: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    for row in rows {
        let owner = row.owner.split_whitespace().next().unwrap_or("UNKNOWN");
        let entry = counts.entry(owner).or_default();
        entry.1 = entry.1.saturating_add(1);
        if row.pass {
            entry.0 = entry.0.saturating_add(1);
        }
    }
    let mut text = String::from("owner\tpass\ttotal\n");
    for (owner, (pass, total)) in counts {
        text.push_str(&format!("{owner}\t{pass}\t{total}\n"));
    }
    text
}

fn first_substantive(values: &[&str]) -> String {
    values
        .iter()
        .find(|value| !placeholder(value))
        .map_or_else(String::new, |value| (*value).to_owned())
}

fn placeholder(value: &str) -> bool {
    let value = value.trim().to_ascii_uppercase();
    value.is_empty()
        || matches!(
            value.as_str(),
            "-" | "—"
                | "GAP"
                | "UNMAPPED"
                | "N/A"
                | "NA"
                | "TODO"
                | "NONE"
                | "<NONE>"
                | "BLOCKED"
                | "ASSIGNED"
        )
}

fn test_names(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(|name| {
            name.trim()
                .trim_matches('`')
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_owned()
        })
        .filter(|name| {
            !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        })
        .collect()
}

fn owner_known(owner: &str) -> bool {
    owner.split_whitespace().next().is_some_and(|value| {
        value.starts_with('K') && value[1..].chars().all(|c| c.is_ascii_digit())
    })
}
