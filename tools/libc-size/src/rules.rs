// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Probe rules (`C69` 5.B) and hard invariants (`C69` 5.C).
//!
//! Rules are data in `tests/libc/size/rules.toml`; an evaluation that cannot run (probe not
//! built) is `skipped`, never `pass`. Invariants are evaluated from the measured rows and the
//! member lists; the frozen `known-violation` rows keep `check` reporting `KNOWN` until the
//! reviewer removes them.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context as _, Result};
use serde::Deserialize;

use crate::members::{matching_members, symbol_matches};
use crate::report::{InvariantResult, LibcSoRow, Row, RuleResult};
use crate::{Arch, Lang, Linkage, Set};

/// One rule row of `rules.toml`.
#[derive(Clone, Debug, Deserialize)]
pub struct Rule {
    /// Probe id.
    pub probe: String,
    /// The `R-n` invariant this rule defends.
    #[serde(default)]
    pub invariant: String,
    /// Limiting linkage (all when absent).
    #[serde(default)]
    pub linkage: Option<String>,
    /// Limiting set (all when absent).
    #[serde(default)]
    pub set: Option<String>,
    /// Limiting language (all when absent).
    #[serde(default)]
    pub lang: Option<String>,
    /// Member patterns that must not be pulled.
    #[serde(default)]
    pub forbid_members: Vec<String>,
    /// Symbol patterns that must not appear.
    #[serde(default)]
    pub forbid_symbols: Vec<String>,
    /// At least one member must match these patterns.
    #[serde(default)]
    pub require_members: Vec<String>,
    /// `S(probe) - S(P0)` ceiling for the same cell.
    #[serde(default)]
    pub max_text_over_p0: Option<u64>,
    /// `.init_array` ceiling.
    #[serde(default)]
    pub max_init_array: Option<u64>,
}

/// The parsed rule file.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct RuleFile {
    /// Schema version.
    #[serde(default)]
    pub schema: u32,
    /// The rules.
    #[serde(default, rename = "rule")]
    pub rules: Vec<Rule>,
}

/// Load `rules.toml`.
///
/// # Errors
/// An I/O or parse error.
pub fn load(path: &Path) -> Result<RuleFile> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// What a rule sees of one measured row.
pub struct RuleContext<'a> {
    /// `S`.
    pub s: u64,
    /// `B`.
    pub bss: u64,
    /// `.init_array` bytes.
    pub init_array: u64,
    /// Every pulled member.
    pub members: &'a [String],
    /// Every symbol name of the unstripped executable.
    pub symbols: &'a [String],
    /// `S(P0)` of the same cell, when measured.
    pub p0_s: Option<u64>,
}

/// Whether the rule applies to the cell.
#[must_use]
pub fn applies(rule: &Rule, linkage: Linkage, set: Set, lang: Lang) -> bool {
    if let Some(filter) = &rule.linkage
        && filter != linkage.as_str()
    {
        return false;
    }
    if let Some(filter) = &rule.set
        && filter != set.as_str()
    {
        return false;
    }
    if let Some(filter) = &rule.lang
        && filter != lang.as_str()
    {
        return false;
    }
    true
}

/// Evaluate one rule for one row.
#[must_use]
pub fn evaluate(rule: &Rule, row: &Row, context: &RuleContext<'_>) -> RuleResult {
    let mut failures = Vec::new();
    if !row.is_measured() {
        return RuleResult {
            probe: row.probe.clone(),
            arch: row.arch,
            linkage: row.linkage,
            set: row.set,
            lang: row.lang,
            rule: rule_name(rule),
            invariant: invariant_name(rule),
            status: "skipped".to_string(),
            detail: "probe not built or not measured".to_string(),
        };
    }
    for pattern in &rule.forbid_members {
        let matched = matching_members(context.members, std::slice::from_ref(pattern));
        if !matched.is_empty() {
            failures.push(format!("member `{pattern}` pulled: {}", matched.join(", ")));
        }
    }
    for pattern in &rule.forbid_symbols {
        let matched: Vec<&String> = context
            .symbols
            .iter()
            .filter(|symbol| symbol_matches(symbol, std::slice::from_ref(pattern)))
            .collect();
        if !matched.is_empty() {
            failures.push(format!(
                "symbol `{pattern}` present: {}",
                matched
                    .iter()
                    .take(5)
                    .map(|symbol| symbol.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
    }
    if !rule.require_members.is_empty()
        && matching_members(context.members, &rule.require_members).is_empty()
    {
        failures.push(format!(
            "no member matches {}",
            rule.require_members.join(", ")
        ));
    }
    if let Some(limit) = rule.max_init_array
        && context.init_array > limit
    {
        failures.push(format!(
            ".init_array is {} bytes, limit {}",
            context.init_array, limit
        ));
    }
    if let Some(limit) = rule.max_text_over_p0 {
        match context.p0_s {
            Some(p0) => {
                let delta = context.s.saturating_sub(p0);
                if delta > limit {
                    failures.push(format!("S(probe)-S(P0) = {delta}, limit {limit}"));
                }
            }
            None => {
                return RuleResult {
                    probe: row.probe.clone(),
                    arch: row.arch,
                    linkage: row.linkage,
                    set: row.set,
                    lang: row.lang,
                    rule: rule_name(rule),
                    invariant: invariant_name(rule),
                    status: "skipped".to_string(),
                    detail: "P0 of the same cell is not measured".to_string(),
                };
            }
        }
    }
    if failures.is_empty() {
        RuleResult {
            probe: row.probe.clone(),
            arch: row.arch,
            linkage: row.linkage,
            set: row.set,
            lang: row.lang,
            rule: rule_name(rule),
            invariant: invariant_name(rule),
            status: "pass".to_string(),
            detail: String::new(),
        }
    } else {
        RuleResult {
            probe: row.probe.clone(),
            arch: row.arch,
            linkage: row.linkage,
            set: row.set,
            lang: row.lang,
            rule: rule_name(rule),
            invariant: invariant_name(rule),
            status: "fail".to_string(),
            detail: failures.join("; "),
        }
    }
}

fn rule_name(rule: &Rule) -> String {
    format!("{}-{}", rule.probe, invariant_name(rule))
}

fn invariant_name(rule: &Rule) -> String {
    if rule.invariant.is_empty() {
        rule.probe.clone()
    } else {
        rule.invariant.clone()
    }
}

/// The cell key used to pair a probe with P0.
#[must_use]
pub fn cell_key(arch: Arch, linkage: Linkage, set: Set, lang: Lang) -> String {
    format!(
        "{}/{}/{}/{}",
        arch.short(),
        linkage.as_str(),
        set.as_str(),
        lang.as_str()
    )
}

/// The hard invariants that need only rows.
#[must_use]
pub fn evaluate_row_invariants(rows: &[Row]) -> Vec<InvariantResult> {
    let mut out = Vec::new();
    for row in rows.iter().filter(|row| row.is_measured()) {
        if row.probe == "P0" {
            out.push(invariant(
                row,
                "INV-B-P0",
                row.bss,
                512,
                row.bss <= 512,
                format!("B(P0) = {} bytes; PROPOSED ceiling 512", row.bss),
            ));
        }
        if row.probe == "P2" {
            out.push(invariant(
                row,
                "INV-B-P2",
                row.bss,
                4096,
                row.bss <= 4096,
                format!("B(P2) = {} bytes; PROPOSED ceiling 4096", row.bss),
            ));
        }
        if row.linkage == Linkage::Dynamic {
            out.push(invariant(
                row,
                "INV-NEEDED",
                row.needed,
                1,
                row.needed == 1,
                format!("DT_NEEDED = {}; must be exactly 1 (libc.so.1)", row.needed),
            ));
        }
    }
    for high in ["P2", "P3"] {
        let low = if high == "P2" { "P0" } else { "P2" };
        for row in rows
            .iter()
            .filter(|row| row.probe == high && row.is_measured())
        {
            let Some(base) = rows.iter().find(|candidate| {
                candidate.probe == low
                    && candidate.arch == row.arch
                    && candidate.linkage == row.linkage
                    && candidate.set == row.set
                    && candidate.lang == row.lang
                    && candidate.is_measured()
            }) else {
                continue;
            };
            let delta = row.s.saturating_sub(base.s);
            let (name, limit) = if high == "P2" {
                ("INV-S-P2-P0", 5120)
            } else {
                ("INV-S-P3-P2", 10240)
            };
            out.push(invariant(
                row,
                name,
                delta,
                limit,
                delta <= limit,
                format!("S({high})-S({low}) = {delta}; PROPOSED ceiling {limit}"),
            ));
        }
    }
    out
}

/// The member-pull invariant of `P2` (`R-13`): zero members of the plug-in crates.
#[must_use]
pub fn evaluate_member_invariants(
    rows: &[Row],
    members: &BTreeMap<String, Vec<String>>,
) -> Vec<InvariantResult> {
    let patterns = ["dtoa*", "locale*", "tz*", "resolv*", "regex*", "iconv*"].map(str::to_string);
    let mut out = Vec::new();
    for row in rows
        .iter()
        .filter(|row| row.probe == "P2" && row.is_measured())
    {
        let key = cell_key(row.arch, row.linkage, row.set, row.lang);
        let Some(list) = members.get(&key) else {
            out.push(invariant(
                row,
                "R-13",
                0,
                0,
                true,
                "member list unavailable; not counted as a pass".to_string(),
            ));
            if let Some(value) = out.last_mut() {
                value.status = "skipped".to_string();
            }
            continue;
        };
        let matched = matching_members(list, &patterns);
        out.push(invariant(
            row,
            "R-13",
            u64::try_from(matched.len()).unwrap_or(u64::MAX),
            0,
            matched.is_empty(),
            format!(
                "P2 pulls {} forbidden member(s): {}",
                matched.len(),
                matched.join(", ")
            ),
        ));
    }
    out
}

/// The `libc.so.1` invariants (`R-12`, `R-20`, `BP-7` when C62 lands).
#[must_use]
pub fn evaluate_libc_so_invariants(rows: &[LibcSoRow]) -> Vec<InvariantResult> {
    let mut out = Vec::new();
    for row in rows.iter().filter(|row| row.status == "ok") {
        out.push(so_invariant(
            row,
            "INV-LIBC-SO-S",
            row.s,
            1_300_000,
            row.s <= 1_300_000,
            format!("S(libc.so.1) = {}; ceiling 1,300,000", row.s),
        ));
        out.push(so_invariant(
            row,
            "R-20-JUMP_SLOT",
            row.jump_slot,
            40,
            row.jump_slot <= 40,
            format!("JUMP_SLOT = {}; ceiling 40", row.jump_slot),
        ));
        out.push(so_invariant(
            row,
            "INV-LIBC-SO-NEEDED",
            row.needed,
            1,
            row.needed == 1,
            format!("DT_NEEDED = {}; must be exactly 1", row.needed),
        ));
    }
    out
}

fn invariant(
    row: &Row,
    name: &str,
    measured: u64,
    limit: u64,
    pass: bool,
    detail: String,
) -> InvariantResult {
    InvariantResult {
        probe: row.probe.clone(),
        arch: row.arch,
        linkage: row.linkage,
        set: row.set,
        lang: row.lang,
        invariant: name.to_string(),
        measured,
        limit,
        status: if pass { "pass" } else { "fail" }.to_string(),
        detail,
    }
}

fn so_invariant(
    row: &LibcSoRow,
    name: &str,
    measured: u64,
    limit: u64,
    pass: bool,
    detail: String,
) -> InvariantResult {
    InvariantResult {
        probe: "P9".to_string(),
        arch: row.arch,
        linkage: Linkage::Dynamic,
        set: Set::Server,
        lang: Lang::Rust,
        invariant: name.to_string(),
        measured,
        limit,
        status: if pass { "pass" } else { "fail" }.to_string(),
        detail,
    }
}

#[cfg(test)]
#[path = "rules_ut.rs"]
mod tests;
