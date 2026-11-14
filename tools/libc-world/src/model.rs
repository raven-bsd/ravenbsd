// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data model: package results, failures, steps and the architecture table.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One target architecture.
pub const ARCHES: &[&str] = &["x86_64", "aarch64", "riscv64"];

/// ELF `e_machine` for one architecture.
#[must_use]
pub fn elf_machine(arch: &str) -> Option<u16> {
    match arch {
        "x86_64" => Some(0x3e),
        "aarch64" => Some(0xb7),
        "riscv64" => Some(0xf3),
        _ => None,
    }
}

/// The compiler triple of one architecture.
#[must_use]
pub fn triple(arch: &str) -> Option<String> {
    match arch {
        "x86_64" | "aarch64" | "riscv64" => Some(format!("{arch}-unknown-ravenbsd")),
        _ => None,
    }
}

/// The Linux-gnu triple used by `--host-alias linux`.
#[must_use]
pub fn linux_triple(arch: &str) -> Option<String> {
    match arch {
        "x86_64" | "aarch64" | "riscv64" => Some(format!("{arch}-unknown-linux-gnu")),
        _ => None,
    }
}

/// Package status values of section 4.8.
pub const STATUSES: &[&str] = &[
    "built",
    "failed",
    "placeholder",
    "tarball-missing",
    "tarball-bad-sha",
    "tarball-unsafe",
    "tarball-layout",
    "timeout",
    "skipped",
];

/// Build kinds implemented by this ticket.
pub const BUILDERS: &[&str] = &["ac", "cm", "me", "mk", "ca", "go", "py", "cf", "kc", "b2"];

/// Build kinds whose builder exists.
pub const IMPLEMENTED_BUILDERS: &[&str] = &["ac", "cm", "mk", "cf", "kc"];

/// A failure class of section 4.7.
pub const FAILURE_CLASSES: &[&str] = &[
    "missing-header",
    "missing-symbol",
    "missing-lib",
    "configure",
    "platform",
    "behaviour",
    "other",
    "timeout",
];

/// One failing class.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Failure {
    /// Class token.
    pub class: String,
    /// First human-readable detail.
    pub detail: String,
}

/// One applied patch.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PatchRecord {
    /// Patch file name.
    pub file: String,
    /// Declared class (`libc`, `platform`, `toolchain`).
    pub class: String,
    /// Declared reason.
    pub reason: String,
    /// A `platform` patch whose reason text still counts as libc.
    pub reclassified_as_libc: bool,
}

/// One executed step.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Step {
    /// Step name.
    pub name: String,
    /// Exit code (`None` for a signal or a timeout).
    pub exit: Option<i32>,
}

/// One package result, serialised to `results/<arch>/<name>.json`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct PackageResult {
    /// Package name.
    pub name: String,
    /// Row number in the 200-row list.
    pub n: u32,
    /// Version (or `PLACEHOLDER`).
    pub version: String,
    /// Status token.
    pub status: String,
    /// Host alias given to `configure` (`raven` or `linux`).
    pub host_alias: String,
    /// Link mode (`static` or `dynamic`).
    pub link: String,
    /// Failure, if any.
    pub failure: Option<Failure>,
    /// All finding classes and their counts.
    pub findings: BTreeMap<String, u32>,
    /// Patches counted as libc patches.
    pub libc_patches: u32,
    /// Patches counted as platform patches.
    pub platform_patches: u32,
    /// Patches counted as toolchain patches.
    pub toolchain_patches: u32,
    /// Every applied patch with its header.
    pub patches: Vec<PatchRecord>,
    /// Disabled features whose reason names libc.
    pub configure_flags_disabled_because_libc: Vec<String>,
    /// Configure probes that failed for a claimed libc symbol.
    pub libc_probe_failed: Vec<String>,
    /// Implicit function declarations seen.
    pub implicit_decls: Vec<String>,
    /// Missing headers seen.
    pub missing_headers: Vec<String>,
    /// Missing libraries seen.
    pub missing_libs: Vec<String>,
    /// Number of distinct undefined symbols.
    pub undefined_symbols: u32,
    /// Number of sysroot headers used.
    pub headers_used: u32,
    /// Libraries named by link lines.
    pub libs: Vec<String>,
    /// Test-suite state (`not-run`).
    pub tests: String,
    /// Total size of regular files in `dest`.
    pub size_bytes: u64,
    /// Executed steps.
    pub steps: Vec<Step>,
}

impl PackageResult {
    /// A result skeleton for a package that has not run.
    #[must_use]
    pub fn new(n: u32, name: &str, version: &str, status: &str) -> Self {
        Self {
            name: name.to_string(),
            n,
            version: version.to_string(),
            status: status.to_string(),
            host_alias: "raven".to_string(),
            link: "static".to_string(),
            failure: None,
            findings: BTreeMap::new(),
            libc_patches: 0,
            platform_patches: 0,
            toolchain_patches: 0,
            patches: Vec::new(),
            configure_flags_disabled_because_libc: Vec::new(),
            libc_probe_failed: Vec::new(),
            implicit_decls: Vec::new(),
            missing_headers: Vec::new(),
            missing_libs: Vec::new(),
            undefined_symbols: 0,
            headers_used: 0,
            libs: Vec::new(),
            tests: "not-run".to_string(),
            size_bytes: 0,
            steps: Vec::new(),
        }
    }

    /// Whether this package counts as built.
    #[must_use]
    pub fn is_built(&self) -> bool {
        self.status == "built"
    }
}

#[cfg(test)]
#[path = "model_ut.rs"]
mod tests;
