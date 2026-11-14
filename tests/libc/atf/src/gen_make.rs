// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reader for the exact BSD-make subset of ticket C71, 4.5.
//!
//! It never runs `make`. Every directive outside the subset is recorded as `unparsed` rather than
//! guessed at; continuations are joined before parsing so tab-indented `\`-continued assignments
//! work.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::{BuildStatus, Framework};

/// One program discovered in a BSD Makefile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MakeProgram {
    /// Program path relative to the scan root, without `.c`.
    pub name: String,
    /// Program source files relative to the scan root.
    pub sources: Vec<String>,
    /// Whitelisted compiler flags.
    pub cflags: Vec<String>,
    /// Whitelisted link flags.
    pub ldadd: Vec<String>,
    /// Framework inferred from the directive that named the program.
    pub framework: Framework,
    /// `ok`, `unparsed` or `needs-sh`.
    pub status: String,
    /// Why the status is not `ok`.
    pub status_reason: Option<String>,
}

/// Everything one Makefile tree yielded.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MakeScan {
    /// C programs, sorted by name.
    pub programs: Vec<MakeProgram>,
    /// Shell tests (`TESTS_SH+=`), sorted.
    pub shell_tests: Vec<String>,
}

impl MakeProgram {
    /// The build status implied by the reader result.
    #[must_use]
    pub fn build_status(&self) -> BuildStatus {
        match self.status.as_str() {
            "ok" => BuildStatus::Built,
            "needs-sh" => BuildStatus::Needs("needs-sh".to_string()),
            _ => BuildStatus::Unparsed(
                self.status_reason
                    .clone()
                    .unwrap_or_else(|| "unparsed make directive".to_string()),
            ),
        }
    }
}

/// Scan `<root>` and every directory reached through `SUBDIR+=` / `TESTS_SUBDIRS+=`.
pub fn scan(root: &Path) -> Result<MakeScan> {
    let mut scan = MakeScan::default();
    if !root.is_dir() {
        return Ok(scan);
    }
    let mut seen = BTreeMap::new();
    if root.join("Makefile").is_file() {
        scan_dir(root, Path::new(""), &mut scan, &mut seen)?;
    } else {
        // The vendor tool copies only directories that contain accepted sources, so a donor
        // root Makefile (OpenBSD `regress/lib/libc`, FreeBSD `lib/libc/tests`) may be absent.
        // Walk the immediate children and obey their own SUBDIR lists.
        let mut names = Vec::new();
        for entry in
            fs::read_dir(root).with_context(|| format!("cannot read {}", root.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", root.display()))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if entry.path().is_dir() && !name.starts_with('.') {
                names.push(name);
            }
        }
        names.sort();
        for name in names {
            scan_dir(root, Path::new(&name), &mut scan, &mut seen)?;
        }
    }
    scan.programs
        .sort_by(|left, right| left.name.cmp(&right.name));
    scan.shell_tests.sort();
    Ok(scan)
}

fn scan_dir(
    root: &Path,
    relative: &Path,
    scan: &mut MakeScan,
    seen: &mut BTreeMap<String, String>,
) -> Result<()> {
    let directory = root.join(relative);
    let makefile = directory.join("Makefile");
    if !makefile.is_file() {
        return Ok(());
    }
    let text = fs::read_to_string(&makefile)
        .with_context(|| format!("cannot read {}", makefile.display()))?;
    let joined = join_continuations(&text);
    parse_makefile(root, relative, &joined, scan, seen)?;
    Ok(())
}

/// Parse one Makefile body; recursively handles subdirectories.
fn parse_makefile(
    root: &Path,
    relative: &Path,
    text: &str,
    scan: &mut MakeScan,
    seen: &mut BTreeMap<String, String>,
) -> Result<()> {
    let mut depth = 0usize;
    let mut prog: Option<String> = None;
    let mut tests: Vec<(String, Framework)> = Vec::new();
    let mut srcs_global: Vec<String> = Vec::new();
    let mut srcs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cflags_vars: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut ldadd_vars: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cflags_global = Vec::new();
    let mut ldadd_global = Vec::new();
    let mut unparsed_reasons: BTreeMap<String, String> = BTreeMap::new();
    let mut subdirs: Vec<String> = Vec::new();
    let mut shell_tests: Vec<String> = Vec::new();

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let trimmed = strip_make_comment(trimmed).trim_end();
        if trimmed.starts_with(".if")
            || trimmed.starts_with(".ifdef")
            || trimmed.starts_with(".ifndef")
            || trimmed.starts_with(".ifmake")
            || trimmed.starts_with(".for")
        {
            depth += 1;
            continue;
        }
        if trimmed.starts_with(".endif") || trimmed.starts_with(".endfor") {
            depth = depth.saturating_sub(1);
            continue;
        }
        if trimmed.strip_prefix(".include").is_some() {
            // Includes are not executed. A non-bsd include may hide conditional sources; the
            // missing-source check turns any program that disappears into `unparsed`.
            continue;
        }
        let Some((key, value)) = split_assignment(trimmed) else {
            if depth == 0 && !trimmed.starts_with('.') {
                // A recipe or a shell line: not part of the subset, but it names no test program.
            }
            continue;
        };
        let value = value.trim();
        match key.as_str() {
            "PROG" => {
                let name = value.to_string();
                if !name.is_empty() {
                    if depth > 0 {
                        unparsed_reasons.insert(name.clone(), "PROG inside .if/.for".to_string());
                    }
                    prog = Some(name);
                }
            }
            "SRCS" => {
                srcs_global = source_tokens(value);
            }
            "CFLAGS" | "CPPFLAGS" => {
                cflags_global.extend(whitelist_cflags(value));
            }
            "LDADD" => {
                ldadd_global.extend(whitelist_ldadd(value));
            }
            "LIBADD" => {
                ldadd_global.extend(whitelist_libadd(value));
            }
            "TESTS_C" | "ATF_TESTS_C" | "NETBSD_ATF_TESTS_C" | "PLAIN_TESTS_C" => {
                let framework = if key == "PLAIN_TESTS_C" {
                    Framework::ExitStatus
                } else {
                    Framework::Atf
                };
                for name in value.split_whitespace() {
                    if name.is_empty() || name == ".WAIT" || name.starts_with('$') {
                        continue;
                    }
                    if depth > 0 {
                        unparsed_reasons.insert(name.to_string(), format!("{key} inside .if/.for"));
                    }
                    tests.push((name.to_string(), framework));
                }
            }
            "TESTS_SH" => {
                for name in value.split_whitespace() {
                    if !name.is_empty() && name != ".WAIT" && !name.starts_with('$') {
                        shell_tests.push(name.to_string());
                    }
                }
            }
            "SUBDIR" | "TESTS_SUBDIRS" => {
                for name in value.split_whitespace() {
                    if name.is_empty() || name == ".WAIT" || name.starts_with('$') {
                        continue;
                    }
                    if name.starts_with('-') {
                        continue;
                    }
                    subdirs.push(name.to_string());
                }
            }
            other => {
                if let Some(program) = other.strip_prefix("SRCS.") {
                    srcs.entry(strip_c(program).to_string())
                        .or_default()
                        .extend(source_tokens(value));
                } else if let Some(program) = other.strip_prefix("LDADD.") {
                    ldadd_vars
                        .entry(strip_c(program).to_string())
                        .or_default()
                        .extend(whitelist_ldadd(value));
                } else if let Some(program) = other.strip_prefix("LIBADD.") {
                    ldadd_vars
                        .entry(strip_c(program).to_string())
                        .or_default()
                        .extend(whitelist_libadd(value));
                } else if let Some(program) = other.strip_prefix("COPTS.") {
                    cflags_vars
                        .entry(strip_c(program).to_string())
                        .or_default()
                        .extend(whitelist_cflags(value));
                }
            }
        }
    }

    for (name, framework) in tests {
        let program_name = join_name(relative, &name);
        let mut sources = srcs
            .get(&name)
            .or_else(|| srcs.get(&format!("{name}.c")))
            .cloned()
            .unwrap_or_default();
        if sources.is_empty() {
            sources.push(format!("{name}.c"));
        }
        let mut cflags = cflags_global.clone();
        cflags.extend(cflags_vars.get(&name).cloned().unwrap_or_default());
        let mut ldadd = ldadd_global.clone();
        ldadd.extend(ldadd_vars.get(&name).cloned().unwrap_or_default());
        add_program(
            root,
            scan,
            seen,
            MakeProgram {
                name: program_name,
                sources: sources
                    .iter()
                    .map(|source| join_source(relative, source))
                    .collect(),
                cflags,
                ldadd,
                framework,
                status: if unparsed_reasons.contains_key(&name) {
                    "unparsed".to_string()
                } else {
                    "ok".to_string()
                },
                status_reason: unparsed_reasons.get(&name).cloned(),
            },
        );
    }
    if let Some(name) = prog {
        let mut sources = if srcs_global.is_empty() {
            vec![format!("{name}.c")]
        } else {
            srcs_global.clone()
        };
        if let Some(extra) = srcs.get(&name) {
            if srcs_global.is_empty() {
                sources = extra.clone();
            } else {
                sources.extend(extra.clone());
            }
        }
        let program_name = join_name(relative, &name);
        let mut cflags = cflags_global.clone();
        cflags.extend(cflags_vars.get(&name).cloned().unwrap_or_default());
        let mut ldadd = ldadd_global.clone();
        ldadd.extend(ldadd_vars.get(&name).cloned().unwrap_or_default());
        add_program(
            root,
            scan,
            seen,
            MakeProgram {
                name: program_name,
                sources: sources
                    .iter()
                    .map(|source| join_source(relative, source))
                    .collect(),
                cflags,
                ldadd,
                framework: Framework::ExitStatus,
                status: if unparsed_reasons.contains_key(&name) {
                    "unparsed".to_string()
                } else {
                    "ok".to_string()
                },
                status_reason: unparsed_reasons.get(&name).cloned(),
            },
        );
    }
    for test in shell_tests {
        scan.shell_tests.push(join_shell(relative, &test));
    }
    for subdir in subdirs {
        scan_dir(root, &relative.join(&subdir), scan, seen)?;
    }
    Ok(())
}

fn add_program(
    root: &Path,
    scan: &mut MakeScan,
    seen: &mut BTreeMap<String, String>,
    mut program: MakeProgram,
) {
    if let Some(previous) = seen.get(&program.name) {
        if *previous != program.sources.join(" ") {
            // Two Makefiles in one directory name the same program with different sources: keep
            // the first and make the collision visible instead of silently overwriting.
            program.status = "unparsed".to_string();
            program.status_reason = Some(format!("duplicate program `{}`", program.name));
        } else {
            return;
        }
    }
    seen.insert(program.name.clone(), program.sources.join(" "));
    if program.status == "ok" {
        for source in &program.sources {
            if !root.join(source).is_file() {
                program.status = "unparsed".to_string();
                program.status_reason = Some(format!("source not vendored: {source}"));
                break;
            }
        }
    }
    scan.programs.push(program);
}

fn strip_make_comment(line: &str) -> &str {
    line.split_once(" #").map_or(line, |(before, _)| before)
}

fn split_assignment(line: &str) -> Option<(String, String)> {
    let (key, rest) = line.split_once('=')?;
    let key = key.trim();
    let key = key
        .strip_suffix('+')
        .or_else(|| key.strip_suffix('?'))
        .or_else(|| key.strip_suffix(':'))
        .unwrap_or(key)
        .trim();
    if key.is_empty() || key.contains(char::is_whitespace) {
        return None;
    }
    Some((key.to_string(), rest.to_string()))
}

fn source_tokens(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|token| !token.starts_with('$') && !token.starts_with('-'))
        .map(ToString::to_string)
        .collect()
}

fn whitelist_cflags(value: &str) -> Vec<String> {
    value
        .split_whitespace()
        .filter(|token| {
            !token.starts_with('$')
                && (token.starts_with("-D")
                    || token.starts_with("-U")
                    || token.starts_with("-I")
                    || token.starts_with("-std=")
                    || token.starts_with("-W"))
                && !token.starts_with("-Wl,")
        })
        .map(ToString::to_string)
        .collect()
}

fn whitelist_ldadd(value: &str) -> Vec<String> {
    const ALLOWED: &[&str] = &["-lm", "-lpthread", "-lrt", "-lutil", "-lcrypt", "-ldl"];
    value
        .split_whitespace()
        .filter(|token| ALLOWED.contains(token))
        .map(ToString::to_string)
        .collect()
}

/// FreeBSD spells libraries as `LIBADD.<program>+= m`; map the allowed names to `-l`.
fn whitelist_libadd(value: &str) -> Vec<String> {
    const ALLOWED: &[&str] = &["m", "pthread", "rt", "util", "crypt", "dl"];
    value
        .split_whitespace()
        .filter(|token| ALLOWED.contains(token))
        .map(|token| format!("-l{token}"))
        .collect()
}

fn join_name(directory: &Path, name: &str) -> String {
    let name = strip_c(name);
    join_plain(directory, name)
}

fn join_source(directory: &Path, name: &str) -> String {
    join_plain(directory, name)
}

fn join_shell(directory: &Path, name: &str) -> String {
    let name = if name.ends_with(".sh") {
        name.to_string()
    } else {
        format!("{name}.sh")
    };
    join_plain(directory, &name)
}

fn join_plain(directory: &Path, name: &str) -> String {
    if directory.as_os_str().is_empty() {
        name.to_string()
    } else {
        format!(
            "{}/{}",
            directory.to_string_lossy().replace('\\', "/"),
            name
        )
    }
}

fn strip_c(name: &str) -> &str {
    name.strip_suffix(".c").unwrap_or(name)
}

/// Join `\`-continued lines before parsing; preserves the leading tab of the continuation.
fn join_continuations(text: &str) -> String {
    let mut result = String::new();
    let mut pending = String::new();
    for line in text.lines() {
        if let Some(prefix) = line.strip_suffix('\\') {
            pending.push_str(prefix);
            if !prefix.ends_with(char::is_whitespace) {
                pending.push(' ');
            }
            continue;
        }
        if !pending.is_empty() {
            pending.push_str(line);
            result.push_str(&pending);
            pending.clear();
        } else {
            result.push_str(line);
        }
        result.push('\n');
    }
    if !pending.is_empty() {
        result.push_str(&pending);
        result.push('\n');
    }
    result
}

#[cfg(test)]
#[path = "gen_make_ut.rs"]
mod tests;
