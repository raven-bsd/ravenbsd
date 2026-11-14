// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `BUDGETS.toml`: freeze, check and the `known-violation` mechanism (`C69` 5.C, 5.D, `R-24`).

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::report::{BootRow, InvariantResult, LibcSoRow, Results, Row, RuleResult, SetSymbolRow};
use crate::{Arch, HEADROOM_PERCENT, Lang, Linkage, Set, with_headroom};

/// A frozen budget row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRow {
    /// Probe.
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Set.
    pub set: Set,
    /// Language.
    pub lang: Lang,
    /// Measured `S`.
    pub measured_s: u64,
    /// Measured `B`.
    pub measured_b: u64,
    /// Measured `F`.
    pub measured_f: u64,
    /// Members pulled.
    pub members: u64,
    /// Budget `S` (`ceil(measured * 103 / 100)`).
    pub budget_s: u64,
    /// Budget `B`.
    pub budget_b: u64,
    /// Budget `F`.
    pub budget_f: u64,
}

impl BudgetRow {
    /// Freeze one measured row.
    #[must_use]
    pub fn from_row(row: &Row, headroom: u64) -> Self {
        Self {
            probe: row.probe.clone(),
            arch: row.arch,
            linkage: row.linkage,
            set: row.set,
            lang: row.lang,
            measured_s: row.s,
            measured_b: row.bss,
            measured_f: row.file,
            members: row.members,
            budget_s: with_headroom(row.s, headroom),
            budget_b: with_headroom(row.bss, headroom),
            budget_f: with_headroom(row.file, headroom),
        }
    }

    /// Whether this row describes the same matrix cell as `row`.
    #[must_use]
    pub fn same_cell(&self, row: &Row) -> bool {
        self.probe == row.probe
            && self.arch == row.arch
            && self.linkage == row.linkage
            && self.set == row.set
            && self.lang == row.lang
    }
}

/// A frozen boot budget (`BP-8`, `3.A.9.1`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootBudget {
    /// Architecture.
    pub arch: Arch,
    /// Measured syscalls before `main`.
    pub syscalls_before_main: u64,
    /// Measured start instructions.
    pub start_instructions: u64,
    /// Exact syscall ceiling: a growth from 1 to 2 must fail.
    pub budget_syscalls: u64,
    /// Instruction ceiling (measured + headroom).
    pub budget_instructions: u64,
}

impl BootBudget {
    /// Freeze one measured boot row.
    #[must_use]
    pub fn from_row(row: &BootRow, headroom: u64) -> Self {
        Self {
            arch: row.arch,
            syscalls_before_main: row.syscalls_before_main,
            start_instructions: row.start_instructions,
            budget_syscalls: row.syscalls_before_main,
            budget_instructions: with_headroom(row.start_instructions, headroom),
        }
    }
}

/// A frozen `libc.so.1` row (`R-12`, `R-20`, `BP-7`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibcSoBudget {
    /// Architecture.
    pub arch: Arch,
    /// The exact path that was inspected.
    pub path: String,
    /// Measured `S`.
    #[serde(rename = "S")]
    pub s: u64,
    /// Measured `F`.
    #[serde(rename = "F")]
    pub file: u64,
    /// Measured `*_RELATIVE` count.
    pub relative: u64,
    /// Measured `JUMP_SLOT` count.
    pub jump_slot: u64,
    /// Measured `.data.rel.ro` bytes.
    pub data_rel_ro: u64,
    /// Budget `S`.
    pub budget_s: u64,
    /// Budget `F`.
    pub budget_f: u64,
    /// `ok` or `skipped: <reason>`.
    pub status: String,
}

impl LibcSoBudget {
    /// Freeze one `libc.so.1` row.
    #[must_use]
    pub fn from_row(row: &LibcSoRow, headroom: u64) -> Self {
        Self {
            arch: row.arch,
            path: row.path.clone(),
            s: row.s,
            file: row.file,
            relative: row.relative,
            jump_slot: row.jump_slot,
            data_rel_ro: row.data_rel_ro,
            budget_s: with_headroom(row.s, headroom),
            budget_f: with_headroom(row.file, headroom),
            status: row.status.clone(),
        }
    }
}

/// A recorded first-measurement violation. It never weakens the invariant; `check` reports the
/// row as `KNOWN` until the reviewer removes the row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnownViolation {
    /// Probe.
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Set.
    pub set: Set,
    /// Language.
    pub lang: Lang,
    /// Invariant id (`R-13`, `INV-B-P0`, ...).
    pub invariant: String,
    /// The rule name when the violation comes from `rules.toml` (empty otherwise).
    #[serde(default)]
    pub rule: String,
    /// Measured value.
    pub measured: u64,
    /// The PROPOSED limit.
    pub limit: u64,
    /// Why the row is frozen (`first measurement`).
    pub reason: String,
    /// Human-readable evidence.
    #[serde(default)]
    pub detail: String,
}

impl KnownViolation {
    fn matches(
        &self,
        probe: &str,
        arch: Arch,
        linkage: Linkage,
        set: Set,
        lang: Lang,
        invariant: &str,
    ) -> bool {
        self.probe == probe
            && self.arch == arch
            && self.linkage == linkage
            && self.set == set
            && self.lang == lang
            && self.invariant == invariant
    }
}

/// The complete `BUDGETS.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Budgets {
    /// Schema version.
    pub schema: u32,
    /// Metric definition.
    pub metric: String,
    /// Headroom percentage (`R-24`: 3).
    pub headroom_percent: u64,
    /// `rustc --version` first line of the measurement.
    pub rustc: String,
    /// `enforced` or `report-only`.
    pub status: String,
    /// One row per measured probe cell.
    #[serde(default)]
    pub budget: Vec<BudgetRow>,
    /// Per-arch boot budgets.
    #[serde(default, rename = "boot")]
    pub boot: Vec<BootBudget>,
    /// Per-arch `libc.so.1` budgets.
    #[serde(default, rename = "libc_so")]
    pub libc_so: Vec<LibcSoBudget>,
    /// Frozen first-measurement violations.
    #[serde(default, rename = "known-violation")]
    pub known_violation: Vec<KnownViolation>,
}

/// The fixed metric line of `5.D`.
pub const METRIC: &str =
    "S = Berkeley text+data of the stripped executable; B = bss; F = file bytes";

impl Budgets {
    /// Freeze a result document into budgets (`measured + 3 percent`).
    #[must_use]
    pub fn freeze(results: &Results) -> Self {
        Self::freeze_with(results, HEADROOM_PERCENT as u64)
    }

    /// Freeze with an explicit headroom.
    #[must_use]
    pub fn freeze_with(results: &Results, headroom: u64) -> Self {
        let budget: Vec<BudgetRow> = results
            .rows
            .iter()
            .filter(|row| row.is_measured())
            .map(|row| BudgetRow::from_row(row, headroom))
            .collect();
        let boot: Vec<BootBudget> = results
            .boot
            .iter()
            .filter(|row| !row.status.starts_with("skipped"))
            .map(|row| BootBudget::from_row(row, headroom))
            .collect();
        let libc_so: Vec<LibcSoBudget> = results
            .libc_so
            .iter()
            .map(|row| LibcSoBudget::from_row(row, headroom))
            .collect();
        let mut known_violation = Vec::new();
        for invariant in results
            .invariants
            .iter()
            .filter(|invariant| invariant.status == "fail")
        {
            known_violation.push(known_from_invariant(invariant));
        }
        for boot in results
            .boot
            .iter()
            .filter(|boot| boot.status != "ok" && !boot.status.starts_with("skipped"))
        {
            for (invariant, measured, limit, detail) in boot_findings(boot) {
                known_violation.push(KnownViolation {
                    probe: "P0".to_string(),
                    arch: boot.arch,
                    linkage: Linkage::Static,
                    set: Set::Tiny,
                    lang: Lang::Rust,
                    invariant: invariant.to_string(),
                    rule: String::new(),
                    measured,
                    limit,
                    reason: "first measurement".to_string(),
                    detail,
                });
            }
        }
        for rule in results.rules.iter().filter(|rule| rule.status == "fail") {
            let covered = known_violation.iter().any(|known| {
                known.matches(
                    &rule.probe,
                    rule.arch,
                    rule.linkage,
                    rule.set,
                    rule.lang,
                    &rule.invariant,
                )
            });
            if !covered {
                known_violation.push(KnownViolation {
                    probe: rule.probe.clone(),
                    arch: rule.arch,
                    linkage: rule.linkage,
                    set: rule.set,
                    lang: rule.lang,
                    invariant: rule.invariant.clone(),
                    rule: rule.rule.clone(),
                    measured: 0,
                    limit: 0,
                    reason: "first measurement".to_string(),
                    detail: rule.detail.clone(),
                });
            }
        }
        for set in results
            .set_symbols
            .iter()
            .filter(|row| row.status == "fail")
        {
            known_violation.push(KnownViolation {
                probe: "P9".to_string(),
                arch: set.arch,
                linkage: Linkage::Static,
                set: Set::Server,
                lang: Lang::Rust,
                invariant: "R-26".to_string(),
                rule: String::new(),
                measured: u64::try_from(set.tiny_only.len().saturating_add(set.server_only.len()))
                    .unwrap_or(u64::MAX),
                limit: 0,
                reason: "first measurement".to_string(),
                detail: format!(
                    "tiny-only {} server-only {}",
                    set.tiny_only.len(),
                    set.server_only.len()
                ),
            });
        }
        let mut row = Self {
            schema: 1,
            metric: METRIC.to_string(),
            headroom_percent: headroom,
            rustc: results.toolchain.rustc.clone(),
            status: "enforced".to_string(),
            budget,
            boot,
            libc_so,
            known_violation,
        };
        row.sort();
        row
    }

    /// Deterministic ordering.
    pub fn sort(&mut self) {
        self.budget.sort_by(|a, b| {
            (
                a.arch,
                a.linkage,
                a.set,
                a.lang,
                crate::report::probe_number(&a.probe),
                &a.probe,
            )
                .cmp(&(
                    b.arch,
                    b.linkage,
                    b.set,
                    b.lang,
                    crate::report::probe_number(&b.probe),
                    &b.probe,
                ))
        });
        self.boot.sort_by_key(|row| row.arch);
        self.libc_so.sort_by_key(|row| row.arch);
        self.known_violation.sort_by(|a, b| {
            (
                a.arch,
                a.linkage,
                a.set,
                a.lang,
                crate::report::probe_number(&a.probe),
                &a.invariant,
            )
                .cmp(&(
                    b.arch,
                    b.linkage,
                    b.set,
                    b.lang,
                    crate::report::probe_number(&b.probe),
                    &b.invariant,
                ))
        });
    }

    /// Serialize with the generated header (`5.D`).
    ///
    /// # Errors
    /// A TOML serialization error.
    pub fn to_toml(&self) -> Result<String> {
        let body = toml::to_string_pretty(self).context("serialize BUDGETS.toml")?;
        let mut out = String::new();
        out.push_str("# SPDX-License-Identifier: BSD-3-Clause\n");
        out.push_str("# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors\n");
        out.push_str("# GENERATED by libc-size freeze from a measurement; reviewer re-freezes\n");
        out.push_str(&body);
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out)
    }

    /// Read `BUDGETS.toml`.
    ///
    /// # Errors
    /// An I/O or parse error.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    /// Write `BUDGETS.toml`.
    ///
    /// # Errors
    /// An I/O error.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(path, self.to_toml()?)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// Find the budget row of a result row.
    #[must_use]
    pub fn row_for(&self, row: &Row) -> Option<&BudgetRow> {
        self.budget.iter().find(|budget| budget.same_cell(row))
    }

    /// Find a known violation for an invariant.
    #[must_use]
    pub fn known_for(&self, invariant: &InvariantResult) -> Option<&KnownViolation> {
        self.known_violation.iter().find(|known| {
            known.matches(
                &invariant.probe,
                invariant.arch,
                invariant.linkage,
                invariant.set,
                invariant.lang,
                &invariant.invariant,
            )
        })
    }

    /// Find a known violation for a failed probe rule.
    #[must_use]
    pub fn known_for_rule(&self, rule: &RuleResult) -> Option<&KnownViolation> {
        self.known_violation.iter().find(|known| {
            known.matches(
                &rule.probe,
                rule.arch,
                rule.linkage,
                rule.set,
                rule.lang,
                &rule.invariant,
            )
        })
    }

    /// Find a known violation for a failed symbol-set comparison.
    #[must_use]
    pub fn known_for_set_symbols(&self, row: &SetSymbolRow) -> bool {
        self.known_violation
            .iter()
            .any(|known| known.probe == "P9" && known.arch == row.arch && known.invariant == "R-26")
    }
}

/// The boot findings that need a `known-violation` row (the status string is a summary).
#[must_use]
pub fn boot_findings(boot: &BootRow) -> Vec<(&'static str, u64, u64, String)> {
    let mut out = Vec::new();
    if boot.init_array != 0 || boot.preinit_array != 0 || boot.ctors != 0 {
        out.push((
            "BP-1",
            boot.init_array
                .saturating_add(boot.preinit_array)
                .saturating_add(boot.ctors),
            0,
            format!(
                ".init_array={} .preinit_array={} .ctors={}",
                boot.init_array, boot.preinit_array, boot.ctors
            ),
        ));
    }
    if boot.syscalls_before_main > crate::boot::SYSCALL_CEILING {
        out.push((
            "BP-syscalls",
            boot.syscalls_before_main,
            crate::boot::SYSCALL_CEILING,
            format!(
                "{} syscall(s) reachable before main; ceiling {} (O-A4)",
                boot.syscalls_before_main,
                crate::boot::SYSCALL_CEILING
            ),
        ));
    }
    if boot.start_instructions > crate::boot::START_INSTRUCTION_CEILING {
        out.push((
            "BP-1-instructions",
            boot.start_instructions,
            crate::boot::START_INSTRUCTION_CEILING,
            format!(
                "static reachable instruction count {}; PROPOSED ceiling {} (replaced by measured + 3 percent)",
                boot.start_instructions,
                crate::boot::START_INSTRUCTION_CEILING
            ),
        ));
    }
    if !boot.early_malloc.is_empty() {
        out.push((
            "BP-2",
            u64::try_from(boot.early_malloc.len()).unwrap_or(u64::MAX),
            0,
            boot.early_malloc.join("; "),
        ));
    }
    if !boot.forbidden_refs.is_empty() {
        out.push((
            "BP-3",
            u64::try_from(boot.forbidden_refs.len()).unwrap_or(u64::MAX),
            0,
            boot.forbidden_refs.join("; "),
        ));
    }
    if !boot.rodata.is_empty() {
        out.push((
            "BP-4",
            u64::try_from(boot.rodata.len()).unwrap_or(u64::MAX),
            0,
            boot.rodata.join("; "),
        ));
    }
    out
}

fn known_from_invariant(invariant: &InvariantResult) -> KnownViolation {
    KnownViolation {
        probe: invariant.probe.clone(),
        arch: invariant.arch,
        linkage: invariant.linkage,
        set: invariant.set,
        lang: invariant.lang,
        invariant: invariant.invariant.clone(),
        rule: String::new(),
        measured: invariant.measured,
        limit: invariant.limit,
        reason: "first measurement".to_string(),
        detail: invariant.detail.clone(),
    }
}

/// One check outcome.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckStatus {
    /// The row is within budget and all rules hold.
    Pass,
    /// A real regression.
    Fail,
    /// Not measured; never counted as a pass.
    Skipped,
    /// A frozen first-measurement violation; `check` does not fail on it.
    Known,
    /// A budget row without a result.
    BudgetOnly,
}

impl CheckStatus {
    /// The report spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pass => "PASS",
            Self::Fail => "FAIL",
            Self::Skipped => "SKIPPED",
            Self::Known => "KNOWN",
            Self::BudgetOnly => "BUDGET-ONLY",
        }
    }
}

/// One check line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckRow {
    /// The matrix cell or invariant.
    pub key: String,
    /// The outcome.
    pub status: CheckStatus,
    /// Evidence.
    pub message: String,
}

/// The complete check result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckReport {
    /// Every line.
    pub rows: Vec<CheckRow>,
}

impl CheckReport {
    /// True when no row is `FAIL`.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.rows.iter().any(|row| row.status == CheckStatus::Fail)
    }

    /// Count one status.
    #[must_use]
    pub fn count(&self, status: CheckStatus) -> usize {
        self.rows.iter().filter(|row| row.status == status).count()
    }

    /// Render the human summary.
    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for row in &self.rows {
            out.push_str(&format!(
                "{:<10} {} {}\n",
                row.status.as_str(),
                row.key,
                row.message
            ));
        }
        out.push_str(&format!(
            "libc-size check: {} pass, {} fail, {} known, {} skipped, {} budget-only\n",
            self.count(CheckStatus::Pass),
            self.count(CheckStatus::Fail),
            self.count(CheckStatus::Known),
            self.count(CheckStatus::Skipped),
            self.count(CheckStatus::BudgetOnly),
        ));
        out
    }
}

/// Compare a result document with its budgets (`R-24`).
#[must_use]
pub fn check(results: &Results, budgets: &Budgets, allow_missing: bool) -> CheckReport {
    let mut report = CheckReport::default();
    for row in &results.rows {
        let key = format!(
            "{} {} {} {} {}",
            row.probe,
            row.arch.short(),
            row.linkage.as_str(),
            row.set.as_str(),
            row.lang.as_str()
        );
        if !row.is_measured() {
            report.rows.push(CheckRow {
                key,
                status: CheckStatus::Skipped,
                message: row.status.clone(),
            });
            continue;
        }
        let Some(budget) = budgets.row_for(row) else {
            report.rows.push(CheckRow {
                key,
                status: if allow_missing {
                    CheckStatus::Skipped
                } else {
                    CheckStatus::Fail
                },
                message: if allow_missing {
                    "no budget (allowed)".to_string()
                } else {
                    "no budget".to_string()
                },
            });
            continue;
        };
        let mut over = Vec::new();
        if row.s > budget.budget_s {
            over.push(format!("S {} > {}", row.s, budget.budget_s));
        }
        if row.bss > budget.budget_b {
            over.push(format!("B {} > {}", row.bss, budget.budget_b));
        }
        if row.file > budget.budget_f {
            over.push(format!("F {} > {}", row.file, budget.budget_f));
        }
        if over.is_empty() {
            report.rows.push(CheckRow {
                key,
                status: CheckStatus::Pass,
                message: format!(
                    "S {}/{} B {}/{} F {}/{}",
                    row.s, budget.budget_s, row.bss, budget.budget_b, row.file, budget.budget_f
                ),
            });
        } else {
            report.rows.push(CheckRow {
                key,
                status: CheckStatus::Fail,
                message: over.join("; "),
            });
        }
    }
    for rule in &results.rules {
        if rule.status == "fail" {
            let known = budgets.known_for_rule(rule).is_some();
            report.rows.push(CheckRow {
                key: format!(
                    "rule {} {} {} {}",
                    rule.probe,
                    rule.arch.short(),
                    rule.linkage.as_str(),
                    rule.lang.as_str()
                ),
                status: if known {
                    CheckStatus::Known
                } else {
                    CheckStatus::Fail
                },
                message: format!("{}: {}", rule.rule, rule.detail),
            });
        } else if rule.status == "skipped" {
            report.rows.push(CheckRow {
                key: format!("rule {} {} {}", rule.probe, rule.arch.short(), rule.rule),
                status: CheckStatus::Skipped,
                message: rule.detail.clone(),
            });
        }
    }
    for invariant in &results.invariants {
        match invariant.status.as_str() {
            "fail" => {
                let known = budgets.known_for(invariant).is_some();
                report.rows.push(CheckRow {
                    key: format!(
                        "{} {} {} {} {}",
                        invariant.invariant,
                        invariant.probe,
                        invariant.arch.short(),
                        invariant.linkage.as_str(),
                        invariant.lang.as_str()
                    ),
                    status: if known {
                        CheckStatus::Known
                    } else {
                        CheckStatus::Fail
                    },
                    message: format!(
                        "measured {} limit {} ({})",
                        invariant.measured, invariant.limit, invariant.detail
                    ),
                });
            }
            "skipped" => report.rows.push(CheckRow {
                key: invariant.invariant.clone(),
                status: CheckStatus::Skipped,
                message: invariant.detail.clone(),
            }),
            _ => {}
        }
    }
    for boot in &results.boot {
        if boot.status.starts_with("skipped") {
            report.rows.push(CheckRow {
                key: format!("boot {}", boot.arch.short()),
                status: CheckStatus::Skipped,
                message: boot.status.clone(),
            });
            continue;
        }
        let Some(budget) = budgets.boot.iter().find(|row| row.arch == boot.arch) else {
            report.rows.push(CheckRow {
                key: format!("boot {}", boot.arch.short()),
                status: CheckStatus::Fail,
                message: "no boot budget".to_string(),
            });
            continue;
        };
        for (invariant, measured, limit, detail) in boot_findings(boot) {
            let known = budgets.known_violation.iter().any(|row| {
                row.arch == boot.arch && row.probe == "P0" && row.invariant == invariant
            });
            report.rows.push(CheckRow {
                key: format!("{} boot {}", invariant, boot.arch.short()),
                status: if known {
                    CheckStatus::Known
                } else {
                    CheckStatus::Fail
                },
                message: format!("measured {measured} limit {limit} ({detail})"),
            });
        }
        let mut over = Vec::new();
        if boot.syscalls_before_main > budget.budget_syscalls {
            over.push(format!(
                "syscalls {} > {}",
                boot.syscalls_before_main, budget.budget_syscalls
            ));
        }
        if boot.start_instructions > budget.budget_instructions {
            over.push(format!(
                "instructions {} > {}",
                boot.start_instructions, budget.budget_instructions
            ));
        }
        report.rows.push(CheckRow {
            key: format!("boot {}", boot.arch.short()),
            status: if over.is_empty() {
                CheckStatus::Pass
            } else {
                CheckStatus::Fail
            },
            message: if over.is_empty() {
                format!(
                    "syscalls {}/{} instructions {}/{}",
                    boot.syscalls_before_main,
                    budget.budget_syscalls,
                    boot.start_instructions,
                    budget.budget_instructions
                )
            } else {
                over.join("; ")
            },
        });
    }
    for so in &results.libc_so {
        if so.status != "ok" {
            report.rows.push(CheckRow {
                key: format!("libc.so.1 {}", so.arch.short()),
                status: CheckStatus::Skipped,
                message: so.status.clone(),
            });
            continue;
        }
        match budgets.libc_so.iter().find(|row| row.arch == so.arch) {
            Some(budget) if so.s <= budget.budget_s && so.file <= budget.budget_f => {
                report.rows.push(CheckRow {
                    key: format!("libc.so.1 {}", so.arch.short()),
                    status: CheckStatus::Pass,
                    message: format!(
                        "S {}/{} F {}/{}",
                        so.s, budget.budget_s, so.file, budget.budget_f
                    ),
                });
            }
            Some(budget) => report.rows.push(CheckRow {
                key: format!("libc.so.1 {}", so.arch.short()),
                status: CheckStatus::Fail,
                message: format!(
                    "S {} > {} or F {} > {}",
                    so.s, budget.budget_s, so.file, budget.budget_f
                ),
            }),
            None => report.rows.push(CheckRow {
                key: format!("libc.so.1 {}", so.arch.short()),
                status: CheckStatus::Fail,
                message: "no libc.so.1 budget".to_string(),
            }),
        }
    }
    for set in &results.set_symbols {
        match set.status.as_str() {
            "fail" => {
                let known = budgets.known_for_set_symbols(set);
                report.rows.push(CheckRow {
                    key: format!("set-symbols {}", set.arch.short()),
                    status: if known {
                        CheckStatus::Known
                    } else {
                        CheckStatus::Fail
                    },
                    message: format!(
                        "tiny-only {} server-only {}",
                        set.tiny_only.len(),
                        set.server_only.len()
                    ),
                });
            }
            "skipped: no archive" => report.rows.push(CheckRow {
                key: format!("set-symbols {}", set.arch.short()),
                status: CheckStatus::Skipped,
                message: set.status.clone(),
            }),
            _ => {}
        }
    }
    for budget in &budgets.budget {
        let present = results.rows.iter().any(|row| {
            row.is_measured()
                && row.probe == budget.probe
                && row.arch == budget.arch
                && row.linkage == budget.linkage
                && row.set == budget.set
                && row.lang == budget.lang
        });
        if !present {
            report.rows.push(CheckRow {
                key: format!(
                    "{} {} {} {} {}",
                    budget.probe,
                    budget.arch.short(),
                    budget.linkage.as_str(),
                    budget.set.as_str(),
                    budget.lang.as_str()
                ),
                status: CheckStatus::BudgetOnly,
                message: "budget row without a result".to_string(),
            });
        }
    }
    report
}

#[cfg(test)]
#[path = "budget_ut.rs"]
mod tests;
