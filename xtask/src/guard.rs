// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The stable-Rust guard.
//!
//! Scans every file Cargo knows about (tracked and untracked, excluding `.gitignore` entries) for
//! constructs that could reintroduce nightly Rust: feature attributes, the bootstrap environment
//! variable, nightly-only compiler flags, `+nightly` toolchain selectors and a non-semver toolchain
//! channel. The guard's own source is exempt because it contains the patterns; test fixtures under
//! `xtask/tests/guard/` are exempt through `xtask/guard-allow.txt`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

const GUARD_SOURCE: &str = "xtask/src/guard.rs";
const ALLOWLIST: &str = "xtask/guard-allow.txt";
const TOOLCHAIN: &str = "rust-toolchain.toml";

/// One guard finding, printed as `path:line: message`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Violation {
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) message: String,
}

impl Violation {
    fn new(path: &str, line: usize, message: impl Into<String>) -> Self {
        Self {
            path: path.to_string(),
            line,
            message: message.into(),
        }
    }
}

/// Run the guard over the repository at `root`, returning the exit code (0 = clean, 1 = findings).
pub(crate) fn run(root: &Path) -> Result<u8> {
    let violations = scan_repo(root)?;
    print_violations(&violations);
    if violations.is_empty() {
        eprintln!("guard: ok");
        Ok(0)
    } else {
        eprintln!("guard: {} violation(s)", violations.len());
        Ok(1)
    }
}

pub(crate) fn print_violations(violations: &[Violation]) {
    for violation in violations {
        println!(
            "{}:{}: {}",
            violation.path, violation.line, violation.message
        );
    }
}

/// Scan every workspace file under `root`.
pub(crate) fn scan_repo(root: &Path) -> Result<Vec<Violation>> {
    let allowlist = read_allowlist(root)?;
    let mut violations = Vec::new();
    for rel_path in repository_files(root)? {
        if allowlist.contains(&rel_path) {
            continue;
        }
        let full_path = root.join(&rel_path);
        let Ok(bytes) = fs::read(&full_path) else {
            continue;
        };
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        violations.extend(scan_content(&rel_path, &text));
    }
    violations.sort();
    violations.dedup();
    Ok(violations)
}

fn repository_files(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
        ])
        .current_dir(root)
        .output()
        .context("cannot run `git ls-files`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git ls-files failed: {}", stderr.trim());
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    Ok(listing
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(str::to_string)
        .collect())
}

fn read_allowlist(root: &Path) -> Result<BTreeSet<String>> {
    let path = root.join(ALLOWLIST);
    if !path.is_file() {
        return Ok(BTreeSet::new());
    }
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_allowlist(&text)
}

fn parse_allowlist(text: &str) -> Result<BTreeSet<String>> {
    let mut entries = BTreeSet::new();
    for line in text.lines() {
        let entry = line.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }
        if !entry.starts_with("xtask/tests/guard/") {
            bail!("guard allowlist entry is not a test fixture: `{entry}`");
        }
        entries.insert(entry.to_string());
    }
    Ok(entries)
}

/// Check one file's contents. `rel_path` is the repository-relative, slash-separated path.
pub(crate) fn scan_content(rel_path: &str, text: &str) -> Vec<Violation> {
    if rel_path == GUARD_SOURCE {
        return Vec::new();
    }
    let is_doc = is_doc(rel_path);
    let is_rust = extension(rel_path) == Some("rs");
    let is_toolchain = rel_path == TOOLCHAIN;
    let mut violations = Vec::new();
    let mut saw_channel = false;

    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        if is_rust && !is_doc {
            if line.contains("#![feature(") || line.contains("#[feature(") {
                violations.push(Violation::new(
                    rel_path,
                    number,
                    "feature attribute is forbidden (stable Rust only)",
                ));
            }
            if line.contains("#![allow(unstable_features)]") {
                violations.push(Violation::new(
                    rel_path,
                    number,
                    "unstable_features allowance is forbidden",
                ));
            }
        }
        if !is_doc && line.contains("RUSTC_BOOTSTRAP") {
            violations.push(Violation::new(
                rel_path,
                number,
                "RUSTC_BOOTSTRAP is forbidden",
            ));
        }
        if !is_doc && is_compiler_flag_file(rel_path) && line.contains("-Z") {
            violations.push(Violation::new(
                rel_path,
                number,
                "nightly-only compiler flag is forbidden",
            ));
        }
        if is_script_or_makefile(rel_path) && line.contains("+nightly") {
            violations.push(Violation::new(
                rel_path,
                number,
                "+nightly toolchain selector is forbidden",
            ));
        }
        if is_toolchain && !line.trim_start().starts_with('#') && line.contains("nightly") {
            violations.push(Violation::new(
                rel_path,
                number,
                "nightly toolchain is forbidden",
            ));
        }
        if is_toolchain && line.trim_start().starts_with("channel") {
            saw_channel = true;
            match channel_value(line) {
                Some(channel) if valid_version(channel) => {}
                Some(channel) => violations.push(Violation::new(
                    rel_path,
                    number,
                    format!("channel `{channel}` is not a stable version"),
                )),
                None => violations.push(Violation::new(
                    rel_path,
                    number,
                    "channel must be a quoted version",
                )),
            }
        }
        if line.contains("cargo +") {
            violations.push(Violation::new(
                rel_path,
                number,
                "`cargo +<toolchain>` selector is forbidden",
            ));
        }
    }
    if is_toolchain && !saw_channel {
        violations.push(Violation::new(
            rel_path,
            1,
            "rust-toolchain.toml has no channel",
        ));
    }
    violations
}

fn is_doc(rel_path: &str) -> bool {
    rel_path.ends_with(".md") || rel_path.starts_with("docs/")
}

fn is_script_or_makefile(rel_path: &str) -> bool {
    rel_path.starts_with("scripts/") || file_name(rel_path) == Some("Makefile")
}

fn is_compiler_flag_file(rel_path: &str) -> bool {
    matches!(extension(rel_path), Some("toml" | "sh" | "rs"))
        || file_name(rel_path) == Some("Makefile")
}

fn extension(rel_path: &str) -> Option<&str> {
    rel_path.rsplit_once('.').map(|(_, extension)| extension)
}

fn file_name(rel_path: &str) -> Option<&str> {
    rel_path.rsplit('/').next()
}

fn channel_value(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("channel")?;
    let rest = rest.trim_start().strip_prefix('=')?.trim_start();
    let rest = rest.strip_prefix('"')?;
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn valid_version(value: &str) -> bool {
    let mut parts = value.split('.');
    let (Some(major), Some(minor), Some(patch), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    [major, minor, patch]
        .iter()
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(rel_path: &str, text: &str) -> Vec<Violation> {
        scan_content(rel_path, text)
    }

    fn fixture(name: &str) -> &'static str {
        match name {
            "feature_positive.rs.txt" => {
                include_str!("../tests/guard/feature_positive.rs.txt")
            }
            "feature_negative.rs.txt" => {
                include_str!("../tests/guard/feature_negative.rs.txt")
            }
            "bootstrap_positive.sh" => include_str!("../tests/guard/bootstrap_positive.sh"),
            "bootstrap_negative.sh" => include_str!("../tests/guard/bootstrap_negative.sh"),
            "zflag_positive.sh" => include_str!("../tests/guard/zflag_positive.sh"),
            "zflag_negative.sh" => include_str!("../tests/guard/zflag_negative.sh"),
            "nightly_positive.sh" => include_str!("../tests/guard/nightly_positive.sh"),
            "nightly_negative.sh" => include_str!("../tests/guard/nightly_negative.sh"),
            "toolchain_nightly_positive.toml" => {
                include_str!("../tests/guard/toolchain_nightly_positive.toml")
            }
            "toolchain_good.toml" => include_str!("../tests/guard/toolchain_good.toml"),
            "toolchain_channel_positive.toml" => {
                include_str!("../tests/guard/toolchain_channel_positive.toml")
            }
            "cargo_plus_positive.txt" => include_str!("../tests/guard/cargo_plus_positive.txt"),
            "cargo_plus_negative.txt" => include_str!("../tests/guard/cargo_plus_negative.txt"),
            "unstable_allow_positive.rs.txt" => {
                include_str!("../tests/guard/unstable_allow_positive.rs.txt")
            }
            "unstable_allow_negative.rs.txt" => {
                include_str!("../tests/guard/unstable_allow_negative.rs.txt")
            }
            other => panic!("unknown fixture {other}"),
        }
    }

    #[test]
    fn feature_attribute_is_flagged() {
        let violations = scan("src/lib.rs", fixture("feature_positive.rs.txt"));
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("feature"));
    }

    #[test]
    fn clean_rust_file_is_not_flagged() {
        assert!(scan("src/lib.rs", fixture("feature_negative.rs.txt")).is_empty());
    }

    #[test]
    fn bootstrap_variable_is_flagged() {
        let violations = scan("scripts/build.sh", fixture("bootstrap_positive.sh"));
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("RUSTC_BOOTSTRAP"));
    }

    #[test]
    fn clean_script_is_not_flagged() {
        assert!(scan("scripts/build.sh", fixture("bootstrap_negative.sh")).is_empty());
    }

    #[test]
    fn docs_are_exempt_from_bootstrap_and_flag_rules() {
        assert!(scan("README.md", "RUSTC_BOOTSTRAP\nRUSTFLAGS=-Zbuild-std").is_empty());
    }

    #[test]
    fn nightly_compiler_flag_is_flagged() {
        let violations = scan("scripts/build.sh", fixture("zflag_positive.sh"));
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("compiler flag"));
    }

    #[test]
    fn clean_script_has_no_compiler_flag() {
        assert!(scan("scripts/build.sh", fixture("zflag_negative.sh")).is_empty());
    }

    #[test]
    fn plus_nightly_in_a_script_is_flagged() {
        let violations = scan("scripts/ci-local.sh", fixture("nightly_positive.sh"));
        assert!(violations.iter().any(|v| v.message.contains("+nightly")));
    }

    #[test]
    fn plus_nightly_in_a_clean_script_is_not_flagged() {
        assert!(scan("scripts/ci-local.sh", fixture("nightly_negative.sh")).is_empty());
    }

    #[test]
    fn nightly_toolchain_is_flagged() {
        let violations = scan(TOOLCHAIN, fixture("toolchain_nightly_positive.toml"));
        assert!(violations.iter().any(|v| v.message.contains("nightly")));
    }

    #[test]
    fn stable_toolchain_is_not_flagged() {
        assert!(scan(TOOLCHAIN, fixture("toolchain_good.toml")).is_empty());
    }

    #[test]
    fn non_semver_channel_is_flagged() {
        let violations = scan(TOOLCHAIN, fixture("toolchain_channel_positive.toml"));
        assert!(
            violations
                .iter()
                .any(|v| v.message.contains("stable version"))
        );
    }

    #[test]
    fn cargo_plus_selector_is_flagged() {
        let violations = scan("notes.txt", fixture("cargo_plus_positive.txt"));
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("cargo +"));
    }

    #[test]
    fn plain_cargo_command_is_not_flagged() {
        assert!(scan("notes.txt", fixture("cargo_plus_negative.txt")).is_empty());
    }

    #[test]
    fn unstable_features_allowance_is_flagged() {
        let violations = scan("src/lib.rs", fixture("unstable_allow_positive.rs.txt"));
        assert_eq!(violations.len(), 1);
        assert!(violations[0].message.contains("unstable_features"));
    }

    #[test]
    fn unstable_features_denial_is_not_flagged() {
        assert!(scan("src/lib.rs", fixture("unstable_allow_negative.rs.txt")).is_empty());
    }

    #[test]
    fn guard_source_is_exempt() {
        let text = "RUSTC_BOOTSTRAP -Z cargo +nightly #![feature(x)";
        assert!(scan(GUARD_SOURCE, text).is_empty());
    }

    #[test]
    fn allowlist_accepts_only_fixtures() {
        let allowlist =
            parse_allowlist("# comment\n\nxtask/tests/guard/a.sh\nxtask/tests/guard/b.rs\n")
                .unwrap();
        assert_eq!(allowlist.len(), 2);
        assert!(parse_allowlist("crates/raven-abi/src/lib.rs\n").is_err());
    }

    #[test]
    fn channel_parser_requires_quotes_and_semver() {
        assert_eq!(channel_value("channel = \"1.97.1\""), Some("1.97.1"));
        assert!(valid_version("1.97.1"));
        assert!(!valid_version("1.97"));
        assert!(!valid_version("nightly-2026-11-09"));
    }

    #[test]
    fn current_repository_is_clean() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let violations = scan_repo(root).unwrap();
        assert!(violations.is_empty(), "{violations:?}");
    }
}
