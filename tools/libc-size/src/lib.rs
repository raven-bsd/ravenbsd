// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ravenlibc size gate (`donor-map/16` Part A, ticket C69).
//!
//! The crate builds probes P0-P9 for the three architectures, measures the stripped
//! executables (`S = text + data`, `B = bss`, `F = file bytes`), compares the values with
//! `tests/libc/size/BUDGETS.toml`, evaluates the probe rules and the hard invariants, and
//! runs the static boot-path checks (`BP-1` ... `BP-5`, `BP-7`).
//!
//! Everything is host-side and deterministic: no timestamps, no absolute paths in a result
//! file, and every subprocess is spawned with an argument vector (never a shell).

#![forbid(unsafe_code)]

pub mod boot;
pub mod budget;
pub mod build;
pub mod measure;
pub mod members;
pub mod monomorph;
pub mod probe;
pub mod relocs;
pub mod report;
pub mod rules;
pub mod testutil;
pub mod tools;

use serde::{Deserialize, Serialize};

/// Headroom of `R-24`: a budget is the measured value plus three percent.
pub const HEADROOM_PERCENT: u128 = 3;

/// One architecture of the size matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Arch {
    /// x86-64.
    X86_64,
    /// AArch64 soft-float.
    Aarch64,
    /// RISC-V 64 GC.
    Riscv64,
}

impl Arch {
    /// Every architecture, in report order.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[Self::X86_64, Self::Aarch64, Self::Riscv64]
    }

    /// The short name used in paths and result rows.
    #[must_use]
    pub fn short(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }

    /// The pinned Rust target triple.
    #[must_use]
    pub fn triple(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none",
            Self::Aarch64 => "aarch64-unknown-none-softfloat",
            Self::Riscv64 => "riscv64gc-unknown-none-elf",
        }
    }

    /// The clang target triple of a C probe.
    #[must_use]
    pub fn c_triple(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none-elf",
            Self::Aarch64 => "aarch64-unknown-none-elf",
            Self::Riscv64 => "riscv64-unknown-none-elf",
        }
    }

    /// Parse one architecture name.
    ///
    /// # Errors
    /// An unknown name.
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        match value {
            "x86_64" => Ok(Self::X86_64),
            "aarch64" => Ok(Self::Aarch64),
            "riscv64" => Ok(Self::Riscv64),
            other => anyhow::bail!("unknown arch `{other}`"),
        }
    }

    /// Parse a comma-separated list, `all` accepted.
    ///
    /// # Errors
    /// An unknown name.
    pub fn parse_list(value: &str) -> anyhow::Result<Vec<Self>> {
        if value == "all" {
            return Ok(Self::all().to_vec());
        }
        let mut arches = Vec::new();
        for item in value.split(',').filter(|item| !item.is_empty()) {
            let arch = Self::parse(item)?;
            if !arches.contains(&arch) {
                arches.push(arch);
            }
        }
        if arches.is_empty() {
            anyhow::bail!("no architecture selected");
        }
        arches.sort();
        Ok(arches)
    }
}

/// Static or dynamic linkage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Linkage {
    /// `libc.a` and `crt1.o`.
    Static,
    /// `libc.so.1` and `Scrt1.o`.
    Dynamic,
}

impl Linkage {
    /// The report spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Dynamic => "dynamic",
        }
    }

    /// Parse a comma-separated list, `all` accepted.
    ///
    /// # Errors
    /// An unknown name.
    pub fn parse_list(value: &str) -> anyhow::Result<Vec<Self>> {
        if value == "all" {
            return Ok(vec![Self::Static, Self::Dynamic]);
        }
        let mut out = Vec::new();
        for item in value.split(',').filter(|item| !item.is_empty()) {
            let linkage = match item {
                "static" => Self::Static,
                "dynamic" => Self::Dynamic,
                other => anyhow::bail!("unknown linkage `{other}`"),
            };
            if !out.contains(&linkage) {
                out.push(linkage);
            }
        }
        if out.is_empty() {
            anyhow::bail!("no linkage selected");
        }
        Ok(out)
    }
}

/// The optimisation set (`R-26`): `tiny` is `opt-level=z`, `server` is `opt-level=s`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Set {
    /// `opt-level=z`, static only.
    Tiny,
    /// `opt-level=s`, static or dynamic.
    Server,
}

impl Set {
    /// The report spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Server => "server",
        }
    }

    /// Parse a comma-separated list, `all` accepted.
    ///
    /// # Errors
    /// An unknown name.
    pub fn parse_list(value: &str) -> anyhow::Result<Vec<Self>> {
        if value == "all" {
            return Ok(vec![Self::Tiny, Self::Server]);
        }
        let mut out = Vec::new();
        for item in value.split(',').filter(|item| !item.is_empty()) {
            let set = match item {
                "tiny" => Self::Tiny,
                "server" => Self::Server,
                other => anyhow::bail!("unknown set `{other}`"),
            };
            if !out.contains(&set) {
                out.push(set);
            }
        }
        if out.is_empty() {
            anyhow::bail!("no set selected");
        }
        Ok(out)
    }
}

/// The language of a probe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Lang {
    /// `no_std` Rust probe.
    Rust,
    /// Freestanding C probe.
    C,
}

impl Lang {
    /// The report spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::C => "c",
        }
    }

    /// Parse a comma-separated list, `all` accepted.
    ///
    /// # Errors
    /// An unknown name.
    pub fn parse_list(value: &str) -> anyhow::Result<Vec<Self>> {
        if value == "all" {
            return Ok(vec![Self::Rust, Self::C]);
        }
        let mut out = Vec::new();
        for item in value.split(',').filter(|item| !item.is_empty()) {
            let lang = match item {
                "rust" => Self::Rust,
                "c" => Self::C,
                other => anyhow::bail!("unknown language `{other}`"),
            };
            if !out.contains(&lang) {
                out.push(lang);
            }
        }
        if out.is_empty() {
            anyhow::bail!("no language selected");
        }
        Ok(out)
    }
}

/// Ceiling of `measured * (100 + headroom) / 100` (`R-24`; zero stays zero).
#[must_use]
pub fn with_headroom(measured: u64, headroom_percent: u64) -> u64 {
    let measured = u128::from(measured);
    let percent = 100u128 + u128::from(headroom_percent);
    let value = measured.saturating_mul(percent).saturating_add(99) / 100;
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
