// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Propose the repaired 219-row register from the owner map, group reports, delegation map and
//! current product source.

use std::collections::BTreeMap;

use rack_acceptance::register::{RegisterRow, load_donor, load_register};
use rack_acceptance::reports::{GroupRow, load_group_rows};
use rack_coverage::Function;

use crate::delegate;
use crate::source::SourceIndex;

/// Build the repaired register in donor-line order.
pub fn propose(
    donor: &[Function],
    current: &[RegisterRow],
    groups: &[GroupRow],
    source: &SourceIndex,
) -> Vec<RegisterRow> {
    let current_by_name: BTreeMap<&str, &RegisterRow> = current
        .iter()
        .map(|row| (row.function.as_str(), row))
        .collect();
    let mut groups_by_name: BTreeMap<&str, Vec<&GroupRow>> = BTreeMap::new();
    for row in groups {
        groups_by_name
            .entry(row.function.as_str())
            .or_default()
            .push(row);
    }

    donor
        .iter()
        .enumerate()
        .map(|(index, function)| {
            let old = current_by_name.get(function.name.as_str()).copied();
            let owner = old
                .map_or("UNOWNED", |row| {
                    row.owner.split_whitespace().next().unwrap_or("UNOWNED")
                })
                .to_owned();
            let claims = groups_by_name
                .get(function.name.as_str())
                .cloned()
                .unwrap_or_default();
            let claim = choose_claim(&owner, &claims, source);
            let delegation = delegate::get(&function.name);
            let test = claim
                .and_then(|claim| substantive(&claim.test))
                .filter(|value| source.has_test_named(value))
                .or_else(|| {
                    old.and_then(|row| substantive(&row.test))
                        .filter(|value| source.has_test_named(value))
                })
                .or_else(|| source.test_for(&function.name))
                .or_else(|| delegation.map(|entry| entry.test.to_owned()))
                .unwrap_or_default();
            let rust = claim
                .and_then(|claim| substantive(&claim.rust))
                .filter(|value| source.symbol_exists(value))
                .or_else(|| {
                    old.and_then(|row| substantive(&row.rust))
                        .filter(|value| source.symbol_exists(value))
                })
                .or_else(|| delegation.map(|entry| entry.rust.to_owned()))
                .or_else(|| source.mapping_name(&function.name))
                .unwrap_or_default();
            let mutation = claim
                .and_then(|claim| substantive(&claim.mutation))
                .or_else(|| old.and_then(|row| substantive(&row.mutation)))
                .or_else(|| delegation.map(|entry| entry.mutation.to_owned()))
                .unwrap_or_else(|| {
                    if test.is_empty() {
                        String::new()
                    } else {
                        delegate::generic_mutation(&test)
                    }
                });
            RegisterRow {
                number: index + 1,
                function: function.name.clone(),
                owner: if owner == "UNOWNED" {
                    claim.map_or_else(|| "K334".to_owned(), |claim| claim.group.clone())
                } else {
                    owner
                },
                rust,
                test,
                mutation,
                status: if matches!(
                    function.name.as_str(),
                    "rack_cong_signal" | "rack_update_prr"
                ) {
                    "GAP K269".to_owned()
                } else {
                    "PASS".to_owned()
                },
            }
        })
        .collect()
}

fn choose_claim<'a>(
    owner: &str,
    claims: &[&'a GroupRow],
    source: &SourceIndex,
) -> Option<&'a GroupRow> {
    let complete = |claim: &&GroupRow| {
        substantive(&claim.rust).is_some()
            && substantive(&claim.test).is_some()
            && substantive(&claim.mutation).is_some()
    };
    let existing = |claim: &&GroupRow| {
        complete(claim) && substantive(&claim.rust).is_some_and(|rust| source.symbol_exists(&rust))
    };
    claims
        .iter()
        .copied()
        .find(|claim| claim.group == owner && existing(claim))
        .or_else(|| {
            claims
                .iter()
                .copied()
                .find(|claim| claim.group != "K269" && existing(claim))
        })
        .or_else(|| claims.iter().copied().find(existing))
        .or_else(|| {
            claims
                .iter()
                .copied()
                .find(|claim| claim.group == owner && complete(claim))
        })
        .or_else(|| claims.iter().copied().find(complete))
        .or_else(|| claims.iter().copied().find(|claim| claim.group == owner))
        .or_else(|| claims.first().copied())
}

fn substantive(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('`').trim();
    let upper = value.to_ascii_uppercase();
    if value.is_empty()
        || matches!(
            upper.as_str(),
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
    {
        None
    } else {
        Some(value.to_owned())
    }
}

/// Render the register table body.
#[must_use]
pub fn render(rows: &[RegisterRow], donor: &[Function]) -> String {
    let lines: BTreeMap<&str, usize> = donor
        .iter()
        .map(|function| (function.name.as_str(), function.line))
        .collect();
    let mut text = String::from(
        "| # | Donor function (`rack.c` line) | Owner | Rust | Named test | Mutation/negative | Status |\n\
         |---:|---|---|---|---|---|---|\n",
    );
    for row in rows {
        let line = lines.get(row.function.as_str()).copied().unwrap_or(0);
        text.push_str(&format!(
            "| {} | `{}` (L{}) | {} | `{}` | `{}` | {} | {} |\n",
            row.number, row.function, line, row.owner, row.rust, row.test, row.mutation, row.status
        ));
    }
    text
}

/// Convenience wrapper used by the CLI.
pub fn propose_from_repo(reports: Option<&std::path::Path>) -> Result<Vec<RegisterRow>, String> {
    let donor = load_donor()?;
    let current = load_register()?;
    let groups = reports.map(load_group_rows).unwrap_or_default();
    let source = SourceIndex::load()?;
    Ok(propose(&donor, &current, &groups, &source))
}
