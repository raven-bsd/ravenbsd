// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Result and report shapes (`C69` 4.4, 5.F, `donor-map/16` 3.A.7.4).
//!
//! `generated` is always `null`: a result file carries no timestamp and no host path, so two
//! runs over the same inputs produce identical bytes.

use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Arch, Lang, Linkage, Set};

/// Toolchain identification: the first line of each version command.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Toolchain {
    /// `rustc --version` first line.
    pub rustc: String,
    /// `ld.lld --version` first line.
    pub lld: String,
}

/// One measured row of the size matrix.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row {
    /// Probe id (`P0` ... `P9`).
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Optimisation set.
    pub set: Set,
    /// Probe language.
    pub lang: Lang,
    /// `S = text + data` of the stripped executable.
    #[serde(rename = "S")]
    pub s: u64,
    /// `bss` of the stripped executable.
    #[serde(rename = "B")]
    pub bss: u64,
    /// Stripped file bytes.
    #[serde(rename = "F")]
    pub file: u64,
    /// Distinct `libc.a` members pulled by the link.
    pub members: u64,
    /// `DT_NEEDED` entries.
    pub needed: u64,
    /// `JUMP_SLOT` relocations.
    pub jump_slot: u64,
    /// `GLOB_DAT` relocations.
    pub glob_dat: u64,
    /// `*_RELATIVE` relocations.
    pub relative: u64,
    /// Budget `S` from `BUDGETS.toml` when one exists, else zero.
    pub budget_s: u64,
    /// `ok`, `over-budget`, `no-budget`, `skipped: <reason>` or `known-violation`.
    pub status: String,
}

impl Row {
    /// A measured row.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn measured(
        probe: &str,
        arch: Arch,
        linkage: Linkage,
        set: Set,
        lang: Lang,
        s: u64,
        bss: u64,
        file: u64,
        members: u64,
        needed: u64,
        jump_slot: u64,
        glob_dat: u64,
        relative: u64,
    ) -> Self {
        Self {
            probe: probe.to_string(),
            arch,
            linkage,
            set,
            lang,
            s,
            bss,
            file,
            members,
            needed,
            jump_slot,
            glob_dat,
            relative,
            budget_s: 0,
            status: "ok".to_string(),
        }
    }

    /// A row that could not be measured. A skip is never a pass.
    #[must_use]
    pub fn skipped(
        probe: &str,
        arch: Arch,
        linkage: Linkage,
        set: Set,
        lang: Lang,
        reason: &str,
    ) -> Self {
        Self {
            probe: probe.to_string(),
            arch,
            linkage,
            set,
            lang,
            s: 0,
            bss: 0,
            file: 0,
            members: 0,
            needed: 0,
            jump_slot: 0,
            glob_dat: 0,
            relative: 0,
            budget_s: 0,
            status: format!("skipped: {reason}"),
        }
    }

    /// True when the row holds a measurement.
    #[must_use]
    pub fn is_measured(&self) -> bool {
        !self.status.starts_with("skipped")
    }

    /// The stable sort key used by the result writer.
    #[must_use]
    pub fn sort_key(&self) -> (u8, u8, u8, u8, u8, String) {
        (
            arch_order(self.arch),
            linkage_order(self.linkage),
            set_order(self.set),
            lang_order(self.lang),
            probe_number(&self.probe),
            self.probe.clone(),
        )
    }
}

fn arch_order(arch: Arch) -> u8 {
    match arch {
        Arch::X86_64 => 0,
        Arch::Aarch64 => 1,
        Arch::Riscv64 => 2,
    }
}

fn linkage_order(linkage: Linkage) -> u8 {
    match linkage {
        Linkage::Static => 0,
        Linkage::Dynamic => 1,
    }
}

fn set_order(set: Set) -> u8 {
    match set {
        Set::Tiny => 0,
        Set::Server => 1,
    }
}

fn lang_order(lang: Lang) -> u8 {
    match lang {
        Lang::Rust => 0,
        Lang::C => 1,
    }
}

/// The numeric part of a probe id (`P10` sorts after `P9`).
#[must_use]
pub fn probe_number(probe: &str) -> u8 {
    probe
        .strip_prefix('P')
        .and_then(|rest| rest.parse::<u8>().ok())
        .unwrap_or(u8::MAX)
}

/// One rule outcome.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleResult {
    /// Probe id.
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Set.
    pub set: Set,
    /// Language.
    pub lang: Lang,
    /// The rule name from `rules.toml`.
    pub rule: String,
    /// The `R-n` invariant this rule defends, for known-violation matching.
    pub invariant: String,
    /// `pass`, `fail` or `skipped`.
    pub status: String,
    /// Human-readable evidence.
    pub detail: String,
}

/// One hard-invariant outcome (`C69` 5.C).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvariantResult {
    /// Probe id the row belongs to.
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Set.
    pub set: Set,
    /// Language.
    pub lang: Lang,
    /// Invariant id (`R-13`, `INV-P2-P0`, ...).
    pub invariant: String,
    /// Measured value.
    pub measured: u64,
    /// The PROPOSED limit.
    pub limit: u64,
    /// `pass`, `fail` or `skipped`.
    pub status: String,
    /// Evidence text.
    pub detail: String,
}

/// The top symbols of one measured probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TopSymbols {
    /// Probe id.
    pub probe: String,
    /// Architecture.
    pub arch: Arch,
    /// Linkage.
    pub linkage: Linkage,
    /// Set.
    pub set: Set,
    /// Language.
    pub lang: Lang,
    /// Up to 30 demangled `name=size` entries, largest first.
    pub symbols: Vec<String>,
}

/// Boot-path measurements of one architecture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BootRow {
    /// Architecture.
    pub arch: Arch,
    /// Syscalls reachable from `__raven_libc_start` (`<= 1`).
    pub syscalls_before_main: u64,
    /// Instructions reachable from `__raven_libc_start`.
    pub start_instructions: u64,
    /// `.init_array` bytes in the P0 link.
    pub init_array: u64,
    /// `.preinit_array` bytes in the P0 link.
    pub preinit_array: u64,
    /// `.ctors` bytes in the P0 link.
    pub ctors: u64,
    /// Early-malloc paths (`[boot-malloc]`).
    pub early_malloc: Vec<String>,
    /// Forbidden start references (`open`, `read`, `getenv`, ...).
    pub forbidden_refs: Vec<String>,
    /// `LD_`/`/etc/` strings found in `.rodata`.
    pub rodata: Vec<String>,
    /// Non-fatal observations (`BP-5`, `BP-7`).
    #[serde(default)]
    pub notes: Vec<String>,
    /// `ok`, `fail` or `skipped: <reason>`.
    pub status: String,
}

/// One `libc.so.1` measurement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibcSoRow {
    /// Architecture.
    pub arch: Arch,
    /// The path that was inspected, repository-relative.
    pub path: String,
    /// `text + data`.
    #[serde(rename = "S")]
    pub s: u64,
    /// `bss`.
    #[serde(rename = "B")]
    pub bss: u64,
    /// File bytes.
    #[serde(rename = "F")]
    pub file: u64,
    /// `DT_NEEDED`.
    pub needed: u64,
    /// `JUMP_SLOT` count.
    pub jump_slot: u64,
    /// `GLOB_DAT` count.
    pub glob_dat: u64,
    /// `*_RELATIVE` count.
    pub relative: u64,
    /// `.data.rel.ro` bytes.
    pub data_rel_ro: u64,
    /// `ok` or `skipped: <reason>`.
    pub status: String,
}

/// The tiny/server symbol-set comparison (`R-26`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetSymbolRow {
    /// Architecture.
    pub arch: Arch,
    /// Names exported by `tiny` only.
    pub tiny_only: Vec<String>,
    /// Names exported by `server` only.
    pub server_only: Vec<String>,
    /// `ok`, `fail` or `skipped: <reason>`.
    pub status: String,
}

/// A skipped row with the exact path that was absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkipPath {
    /// The row key.
    pub row: String,
    /// The exact repository-relative path that was expected.
    pub path: String,
    /// The reason recorded in `status`.
    pub reason: String,
}

/// The complete result document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Results {
    /// Schema version.
    pub schema: u32,
    /// Always `None`: no timestamps.
    pub generated: Option<String>,
    /// Toolchain identification.
    pub toolchain: Toolchain,
    /// The measured matrix.
    pub rows: Vec<Row>,
    /// Probe rule outcomes.
    pub rules: Vec<RuleResult>,
    /// Hard-invariant outcomes.
    pub invariants: Vec<InvariantResult>,
    /// Boot-path rows.
    pub boot: Vec<BootRow>,
    /// `libc.so.1` rows.
    pub libc_so: Vec<LibcSoRow>,
    /// Tiny/server symbol sets.
    pub set_symbols: Vec<SetSymbolRow>,
    /// Top symbols of P2/P9.
    pub top_symbols: Vec<TopSymbols>,
    /// Every absent artifact with its exact path.
    pub skips: Vec<SkipPath>,
}

impl Results {
    /// An empty document.
    #[must_use]
    pub fn new(toolchain: Toolchain) -> Self {
        Self {
            schema: 1,
            generated: None,
            toolchain,
            rows: Vec::new(),
            rules: Vec::new(),
            invariants: Vec::new(),
            boot: Vec::new(),
            libc_so: Vec::new(),
            set_symbols: Vec::new(),
            top_symbols: Vec::new(),
            skips: Vec::new(),
        }
    }

    /// Sort every vector into its deterministic order.
    pub fn finalize(&mut self) {
        self.rows.sort_by_key(Row::sort_key);
        self.rules.sort_by(|a, b| {
            (
                arch_order(a.arch),
                linkage_order(a.linkage),
                set_order(a.set),
                lang_order(a.lang),
                probe_number(&a.probe),
                &a.rule,
            )
                .cmp(&(
                    arch_order(b.arch),
                    linkage_order(b.linkage),
                    set_order(b.set),
                    lang_order(b.lang),
                    probe_number(&b.probe),
                    &b.rule,
                ))
        });
        self.invariants.sort_by(|a, b| {
            (
                arch_order(a.arch),
                linkage_order(a.linkage),
                set_order(a.set),
                lang_order(a.lang),
                probe_number(&a.probe),
                &a.invariant,
            )
                .cmp(&(
                    arch_order(b.arch),
                    linkage_order(b.linkage),
                    set_order(b.set),
                    lang_order(b.lang),
                    probe_number(&b.probe),
                    &b.invariant,
                ))
        });
        self.boot.sort_by_key(|row| arch_order(row.arch));
        self.libc_so.sort_by_key(|row| arch_order(row.arch));
        self.set_symbols.sort_by_key(|row| arch_order(row.arch));
        self.top_symbols.sort_by(|a, b| {
            (
                arch_order(a.arch),
                probe_number(&a.probe),
                lang_order(a.lang),
                set_order(a.set),
            )
                .cmp(&(
                    arch_order(b.arch),
                    probe_number(&b.probe),
                    lang_order(b.lang),
                    set_order(b.set),
                ))
        });
        self.skips
            .sort_by(|a, b| a.row.cmp(&b.row).then(a.path.cmp(&b.path)));
    }

    /// Serialize with a trailing newline.
    ///
    /// # Errors
    /// A serialization error.
    pub fn to_json(&self) -> Result<String> {
        let mut text = serde_json::to_string_pretty(self).context("serialize results")?;
        text.push('\n');
        Ok(text)
    }

    /// Read a result document.
    ///
    /// # Errors
    /// An I/O or parse error.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("cannot read {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
    }

    /// Write the document.
    ///
    /// # Errors
    /// An I/O error.
    pub fn write(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        std::fs::write(path, self.to_json()?)
            .with_context(|| format!("cannot write {}", path.display()))
    }

    /// The measured row of one matrix cell.
    #[must_use]
    pub fn row(
        &self,
        probe: &str,
        arch: Arch,
        linkage: Linkage,
        set: Set,
        lang: Lang,
    ) -> Option<&Row> {
        self.rows.iter().find(|row| {
            row.probe == probe
                && row.arch == arch
                && row.linkage == linkage
                && row.set == set
                && row.lang == lang
        })
    }
}

/// Render the Markdown report (`donor-map/16` 3.A.7.4).
#[must_use]
pub fn render_markdown(results: &Results) -> String {
    let mut out = String::new();
    out.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    out.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n");
    out.push_str("# ravenlibc size report\n\n");
    out.push_str(
        "Metric: `S = Berkeley text+data of the stripped executable; B = bss; F = file bytes`.\n",
    );
    out.push_str(
        "Budgets: measured + 3 percent (`R-24`). `status = skipped: ...` is never a pass.\n\n",
    );
    for arch in Arch::all() {
        let rows: Vec<&Row> = results
            .rows
            .iter()
            .filter(|row| row.arch == *arch)
            .collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("## {}\n\n", arch.short()));
        out.push_str(
            "| probe | linkage | set | lang | S | B | F | members | needed | budget | status |\n",
        );
        out.push_str("|---|---|---|---|---|---|---|---|---|---|---|\n");
        for row in rows {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
                row.probe,
                row.linkage.as_str(),
                row.set.as_str(),
                row.lang.as_str(),
                row.s,
                row.bss,
                row.file,
                row.members,
                row.needed,
                row.budget_s,
                row.status
            ));
        }
        out.push('\n');
        if let Some(boot) = results.boot.iter().find(|boot| boot.arch == *arch) {
            out.push_str(&format!(
                "Boot: syscalls before `main` {}, start instructions {}, `.init_array` {}, `.preinit_array` {}, `.ctors {}`.\n\n",
                boot.syscalls_before_main,
                boot.start_instructions,
                boot.init_array,
                boot.preinit_array,
                boot.ctors
            ));
            for path in &boot.early_malloc {
                out.push_str(&format!("* `[boot-malloc]` {path}\n"));
            }
            for reference in &boot.forbidden_refs {
                out.push_str(&format!("* `[boot-ref]` {reference}\n"));
            }
            for string in &boot.rodata {
                out.push_str(&format!("* `[boot-rodata]` {string}\n"));
            }
            out.push('\n');
        }
    }
    let failed_rules: Vec<&RuleResult> = results
        .rules
        .iter()
        .filter(|rule| rule.status == "fail")
        .collect();
    if !failed_rules.is_empty() {
        out.push_str("## Failing probe rules\n\n");
        for rule in failed_rules {
            out.push_str(&format!(
                "* {} {} {} {}: `{}` {}\n",
                rule.probe,
                rule.arch.short(),
                rule.linkage.as_str(),
                rule.lang.as_str(),
                rule.rule,
                rule.detail
            ));
        }
        out.push('\n');
    }
    let failed: Vec<&InvariantResult> = results
        .invariants
        .iter()
        .filter(|invariant| invariant.status == "fail")
        .collect();
    if !failed.is_empty() {
        out.push_str("## Hard-invariant failures\n\n");
        for invariant in failed {
            out.push_str(&format!(
                "* {} {} {} {}: `{}` measured {} > {}\n",
                invariant.probe,
                invariant.arch.short(),
                invariant.linkage.as_str(),
                invariant.lang.as_str(),
                invariant.invariant,
                invariant.measured,
                invariant.limit
            ));
        }
        out.push('\n');
    }
    for top in &results.top_symbols {
        if top.symbols.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "## Top symbols: {} {} {} {}\n\n",
            top.probe,
            top.arch.short(),
            top.set.as_str(),
            top.lang.as_str()
        ));
        for symbol in top.symbols.iter().take(20) {
            out.push_str(&format!("* `{symbol}`\n"));
        }
        out.push('\n');
    }
    if !results.skips.is_empty() {
        out.push_str("## Skipped artifacts\n\n");
        for skip in &results.skips {
            out.push_str(&format!(
                "* {}: `skipped: {}` (`{}`)\n",
                skip.row, skip.reason, skip.path
            ));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
#[path = "report_ut.rs"]
mod tests;
