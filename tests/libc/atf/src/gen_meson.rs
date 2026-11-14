// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Readers for the non-BSD suites: mlibc's `all_test_cases` list, picolibc's file naming, and
//! pjdfstest's TAP scripts (ticket C71, 4.5).

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::{BuildStatus, Framework};

use crate::gen_make::MakeProgram;

/// Parse the first `all_test_cases = [ ... ]` list of an mlibc `meson.build`.
pub fn parse_all_test_cases(text: &str) -> Vec<String> {
    let mut cases = Vec::new();
    let mut inside = false;
    for line in text.lines() {
        let line = line.trim();
        if !inside {
            if line.starts_with("all_test_cases") && line.contains('[') {
                inside = true;
                if let Some((_, rest)) = line.split_once('[') {
                    collect_quoted(rest, &mut cases);
                    if rest.contains(']') {
                        break;
                    }
                }
            }
            continue;
        }
        if line.contains(']') {
            break;
        }
        collect_quoted(line, &mut cases);
    }
    cases
}

fn collect_quoted(line: &str, cases: &mut Vec<String>) {
    let mut rest = line;
    while let Some(start) = rest.find('\'') {
        rest = &rest[start + 1..];
        let Some(end) = rest.find('\'') else {
            break;
        };
        cases.push(rest[..end].to_string());
        rest = &rest[end + 1..];
    }
}

/// Scan `vendor/mlibc`; programs are `ansi/name`, `posix/name` and `rtld/name`.
pub fn scan_mlibc(root: &Path) -> Result<Vec<MakeProgram>> {
    let meson = root.join("tests/meson.build");
    let text =
        fs::read_to_string(&meson).with_context(|| format!("cannot read {}", meson.display()))?;
    let mut programs = Vec::new();
    for case in parse_all_test_cases(&text) {
        let top = case.split('/').next().unwrap_or("");
        if !matches!(top, "ansi" | "posix" | "rtld") {
            continue;
        }
        let source = format!("tests/{case}.c");
        let (status, reason) = if source.ends_with(".cpp") {
            (
                "unparsed".to_string(),
                Some("C++ test, no C++ in the scaffold".to_string()),
            )
        } else if !root.join(&source).is_file() {
            (
                "unparsed".to_string(),
                Some(format!("source not vendored: {source}")),
            )
        } else {
            ("ok".to_string(), None)
        };
        let status = if top == "rtld" && status == "ok" {
            "needs-dynamic".to_string()
        } else {
            status
        };
        programs.push(MakeProgram {
            name: case,
            sources: vec![source],
            cflags: Vec::new(),
            ldadd: Vec::new(),
            framework: Framework::ExitStatus,
            status,
            status_reason: reason,
        });
    }
    programs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(programs)
}

/// Scan `vendor/picolibc/test`; programs are `test-*.c` and `libc-testsuite/*.c`.
pub fn scan_picolibc(root: &Path) -> Result<Vec<MakeProgram>> {
    let mut sources = Vec::new();
    for entry in fs::read_dir(root).with_context(|| format!("cannot read {}", root.display()))? {
        let entry = entry.with_context(|| format!("cannot read {}", root.display()))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("test-") && name.ends_with(".c") && entry.path().is_file() {
            sources.push(name);
        }
    }
    let testsuite = root.join("libc-testsuite");
    if testsuite.is_dir() {
        for entry in fs::read_dir(&testsuite)
            .with_context(|| format!("cannot read {}", testsuite.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", testsuite.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".c") && entry.path().is_file() {
                sources.push(format!("libc-testsuite/{name}"));
            }
        }
    }
    sources.sort();
    let mut programs = Vec::new();
    for source in sources {
        let name = source.strip_suffix(".c").unwrap_or(&source).to_string();
        programs.push(MakeProgram {
            name,
            sources: vec![source],
            cflags: Vec::new(),
            ldadd: Vec::new(),
            framework: Framework::ExitStatus,
            status: "ok".to_string(),
            status_reason: None,
        });
    }
    Ok(programs)
}

/// Scan `vendor/pjdfstest/tests`: every `<group>/*.t` is a TAP program that needs a mounted FS.
pub fn scan_pjdfstest(root: &Path) -> Result<Vec<MakeProgram>> {
    let mut programs = Vec::new();
    let Ok(groups) = fs::read_dir(root) else {
        return Ok(programs);
    };
    for group in groups {
        let group = group.context("cannot read a pjdfstest group")?;
        if !group.path().is_dir() {
            continue;
        }
        let group_name = group.file_name().to_string_lossy().into_owned();
        for entry in fs::read_dir(group.path())
            .with_context(|| format!("cannot read {}", group.path().display()))?
        {
            let entry = entry.context("cannot read a pjdfstest script")?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with(".t") {
                programs.push(MakeProgram {
                    name: format!("{group_name}/{}", name.trim_end_matches(".t")),
                    sources: vec![format!("{group_name}/{name}")],
                    cflags: Vec::new(),
                    ldadd: Vec::new(),
                    framework: Framework::Tap,
                    status: "needs-fs".to_string(),
                    status_reason: Some("needs a mounted filesystem (FS lane)".to_string()),
                });
            }
        }
    }
    programs.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(programs)
}

/// The build status implied by a generated program.
#[must_use]
pub fn status_of(program: &MakeProgram) -> BuildStatus {
    match program.status.as_str() {
        "ok" => BuildStatus::Built,
        other => BuildStatus::Needs(other.to_string()),
    }
}

#[cfg(test)]
#[path = "gen_meson_ut.rs"]
mod tests;
