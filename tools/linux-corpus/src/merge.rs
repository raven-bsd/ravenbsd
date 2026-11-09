// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Merge per-program results into the JSON/Markdown coverage dashboard.

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::runner::ProgramResult;
use crate::strace::SyscallStat;
use crate::syscalls;

/// Host identification embedded in the dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Host {
    /// Kernel release, for example `5.19.0-50-generic`.
    pub kernel: String,
    /// Machine architecture, for example `x86_64`.
    pub arch: String,
}

/// One program in the merged JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramSummary {
    /// Program id.
    pub id: String,
    /// Category.
    pub category: String,
    /// Exit status, or null for skipped/setup-failed.
    pub exit: Option<i32>,
    /// Whether the program was skipped because a required binary is missing.
    pub skipped: bool,
    /// Number of distinct syscalls used.
    pub syscalls_used: usize,
}

/// One syscall in the merged JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyscallSummary {
    /// Syscall name as printed by `strace`.
    pub name: String,
    /// x86_64 syscall number from the generated table, or null when unknown.
    pub x86_64_nr: Option<u32>,
    /// Program ids that used this syscall, sorted.
    pub programs: Vec<String>,
    /// Number of programs that used this syscall.
    pub program_count: usize,
    /// Total calls across all programs.
    pub total_calls: u64,
    /// Calls whose result was `-1`.
    pub errors: u64,
    /// Normalised argument patterns.
    pub patterns: BTreeMap<String, BTreeMap<String, u64>>,
}

/// The merged coverage document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Coverage {
    /// Generation date (`YYYY-MM-DD`).
    pub generated: String,
    /// Host kernel/arch.
    pub host: Host,
    /// Programs, sorted by id.
    pub programs: Vec<ProgramSummary>,
    /// Syscalls, sorted by program count descending then name.
    pub syscalls: Vec<SyscallSummary>,
}

/// Options for `merge`.
#[derive(Debug, Clone)]
pub struct MergeOptions {
    /// Directory containing `<id>.json` results.
    pub out: PathBuf,
    /// Output JSON path.
    pub json: PathBuf,
    /// Output Markdown path.
    pub md: PathBuf,
    /// Optional file listing already-implemented syscalls.
    pub implemented: Option<PathBuf>,
}

/// Load all per-program results from `out`, sorted by id.
pub fn load_results(out: &Path) -> Result<Vec<ProgramResult>> {
    if !out.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(out).with_context(|| format!("reading {}", out.display()))? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("json") && path.is_file() {
            paths.push(path);
        }
    }
    paths.sort();
    let mut results = Vec::new();
    for path in paths {
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        let result: ProgramResult = serde_json::from_str(&text)
            .with_context(|| format!("parsing result {}", path.display()))?;
        results.push(result);
    }
    results.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(results)
}

/// Build the merged dashboard and write both files.
pub fn merge(options: &MergeOptions) -> Result<Coverage> {
    let results = load_results(&options.out)?;
    let coverage = build_coverage(&results);
    write_json(&coverage, &options.json)?;
    let implemented = match &options.implemented {
        Some(path) => Some(read_implemented(path)?),
        None => None,
    };
    write_markdown(&coverage, &results, &options.md, implemented.as_ref())?;
    Ok(coverage)
}

fn build_coverage(results: &[ProgramResult]) -> Coverage {
    let mut programs: Vec<ProgramSummary> = results
        .iter()
        .map(|result| ProgramSummary {
            id: result.id.clone(),
            category: result.category.clone(),
            exit: result.exit,
            skipped: result.skipped,
            syscalls_used: result.syscalls.len(),
        })
        .collect();
    programs.sort_by(|left, right| left.id.cmp(&right.id));

    let mut aggregate: BTreeMap<String, (BTreeSet<String>, SyscallStat)> = BTreeMap::new();
    for result in results {
        for (name, stat) in &result.syscalls {
            let entry = aggregate
                .entry(name.clone())
                .or_insert_with(|| (BTreeSet::new(), SyscallStat::default()));
            entry.0.insert(result.id.clone());
            entry.1.count += stat.count;
            entry.1.errors += stat.errors;
            for (kind, values) in &stat.patterns {
                let target = entry.1.patterns.entry(kind.clone()).or_default();
                for (value, count) in values {
                    *target.entry(value.clone()).or_insert(0) += count;
                }
            }
        }
    }

    let mut syscalls: Vec<SyscallSummary> = aggregate
        .into_iter()
        .map(|(name, (programs, stat))| SyscallSummary {
            x86_64_nr: syscalls::x86_64_nr(&name),
            name,
            program_count: programs.len(),
            programs: programs.into_iter().collect(),
            total_calls: stat.count,
            errors: stat.errors,
            patterns: stat.patterns,
        })
        .collect();
    syscalls.sort_by(|left, right| {
        right
            .program_count
            .cmp(&left.program_count)
            .then_with(|| left.name.cmp(&right.name))
    });

    Coverage {
        generated: today(),
        host: host(),
        programs,
        syscalls,
    }
}

fn today() -> String {
    Command::new("date")
        .args(["-u", "+%Y-%m-%d"])
        .output()
        .ok()
        .and_then(|output| {
            output
                .status
                .success()
                .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn host() -> Host {
    let kernel = Command::new("uname")
        .arg("-r")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let arch = Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| std::env::consts::ARCH.to_string());
    Host { kernel, arch }
}

fn write_json(coverage: &Coverage, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut text = serde_json::to_string_pretty(coverage).context("serialising coverage")?;
    text.push('\n');
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

fn write_markdown(
    coverage: &Coverage,
    results: &[ProgramResult],
    path: &Path,
    implemented: Option<&HashSet<String>>,
) -> Result<()> {
    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n");
    text.push_str("# Linux syscall coverage\n\n");
    text.push_str(&format!(
        "Generated: {}\nHost: {} ({})\n\n",
        coverage.generated, coverage.host.kernel, coverage.host.arch
    ));
    let skipped = results.iter().filter(|result| result.skipped).count();
    let failed = results
        .iter()
        .filter(|result| !result.is_ok() && !result.skipped)
        .count();
    text.push_str(&format!(
        "Programs: {} total, {} skipped, {} not passing.\n\n",
        results.len(),
        skipped,
        failed
    ));

    text.push_str("## Top 60 syscalls by number of programs\n\n");
    text.push_str("| # | syscall | x86_64 nr | programs | calls | errors |\n");
    text.push_str("|---|---|---|---|---|---|\n");
    for (index, syscall) in coverage.syscalls.iter().take(60).enumerate() {
        let number = syscall
            .x86_64_nr
            .map_or_else(|| "-".to_string(), |value| value.to_string());
        text.push_str(&format!(
            "| {} | `{}` | {} | {} | {} | {} |\n",
            index + 1,
            syscall.name,
            number,
            syscall.program_count,
            syscall.total_calls,
            syscall.errors
        ));
    }
    text.push('\n');

    text.push_str("## Programs by category\n\n");
    for category in crate::corpus::CATEGORIES {
        text.push_str(&format!("### {category}\n\n"));
        let mut rows: Vec<&ProgramResult> = results
            .iter()
            .filter(|result| result.category == *category)
            .collect();
        rows.sort_by(|left, right| left.id.cmp(&right.id));
        if rows.is_empty() {
            text.push_str("_No programs._\n\n");
            continue;
        }
        text.push_str("| program | status | exit | syscalls | reason |\n");
        text.push_str("|---|---|---|---|---|\n");
        for result in rows {
            let exit = result
                .exit
                .map_or_else(|| "-".to_string(), |value| value.to_string());
            let reason = result.reason.clone().unwrap_or_default();
            text.push_str(&format!(
                "| `{}` | {} | {} | {} | {} |\n",
                result.id,
                result.status(),
                exit,
                result.syscalls.len(),
                reason
            ));
        }
        text.push('\n');
    }

    if let Some(implemented) = implemented {
        text.push_str("## Not yet implemented\n\n");
        let missing: Vec<&SyscallSummary> = coverage
            .syscalls
            .iter()
            .filter(|syscall| !implemented.contains(&syscall.name))
            .collect();
        if missing.is_empty() {
            text.push_str("_All corpus syscalls are implemented._\n");
        } else {
            text.push_str("| syscall | x86_64 nr | programs | calls |\n");
            text.push_str("|---|---|---|---|\n");
            for syscall in missing {
                let number = syscall
                    .x86_64_nr
                    .map_or_else(|| "-".to_string(), |value| value.to_string());
                text.push_str(&format!(
                    "| `{}` | {} | {} | {} |\n",
                    syscall.name, number, syscall.program_count, syscall.total_calls
                ));
            }
        }
        text.push('\n');
    }

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let text = format!("{}\n", text.trim_end());
    fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Read a file with one syscall name per line.
pub fn read_implemented(path: &Path) -> Result<HashSet<String>> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
}

/// Syscalls used by the corpus but absent from `implemented`, ranked.
pub fn missing(out: &Path, implemented: &HashSet<String>) -> Result<Vec<SyscallSummary>> {
    let results = load_results(out)?;
    let coverage = build_coverage(&results);
    Ok(coverage
        .syscalls
        .into_iter()
        .filter(|syscall| !implemented.contains(&syscall.name))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempDir;

    fn result(id: &str, category: &str, exit: i32, syscalls: &[(&str, u64, u64)]) -> ProgramResult {
        let mut map = BTreeMap::new();
        for (name, count, errors) in syscalls {
            map.insert(
                (*name).to_string(),
                SyscallStat {
                    count: *count,
                    errors: *errors,
                    patterns: BTreeMap::new(),
                },
            );
        }
        ProgramResult {
            id: id.to_string(),
            category: category.to_string(),
            exit: Some(exit),
            expected_exit: 0,
            skipped: false,
            reason: None,
            timed_out: false,
            setup_failed: false,
            capture_error: None,
            duration_ms: 1,
            syscalls_used: map.len(),
            strace: format!("out/{id}.strace"),
            syscalls: map,
            unparsed_lines: 0,
        }
    }

    #[test]
    fn merge_sorts_and_counts_programs() {
        let results = vec![
            result("b", "dynamic", 0, &[("read", 2, 0), ("write", 1, 0)]),
            result("a", "static", 0, &[("read", 5, 1)]),
        ];
        let coverage = build_coverage(&results);
        assert_eq!(coverage.programs[0].id, "a");
        assert_eq!(coverage.syscalls[0].name, "read");
        assert_eq!(coverage.syscalls[0].program_count, 2);
        assert_eq!(coverage.syscalls[0].total_calls, 7);
        assert_eq!(coverage.syscalls[0].errors, 1);
        assert_eq!(coverage.syscalls[0].x86_64_nr, Some(0));
        assert_eq!(coverage.syscalls[1].name, "write");
        assert_eq!(coverage.syscalls[1].program_count, 1);
    }

    #[test]
    fn merge_writes_json_and_markdown() {
        let temp = TempDir::new("merge-write");
        let out = temp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let result = result("one", "shell", 0, &[("read", 1, 0)]);
        fs::write(
            out.join("one.json"),
            serde_json::to_string(&result).unwrap(),
        )
        .unwrap();
        let json = temp.path().join("coverage.json");
        let md = temp.path().join("coverage.md");
        let coverage = merge(&MergeOptions {
            out,
            json: json.clone(),
            md: md.clone(),
            implemented: None,
        })
        .unwrap();
        assert_eq!(coverage.programs.len(), 1);
        let json_text = fs::read_to_string(&json).unwrap();
        let parsed: Coverage = serde_json::from_str(&json_text).unwrap();
        assert_eq!(parsed.syscalls[0].name, "read");
        let md_text = fs::read_to_string(&md).unwrap();
        assert!(md_text.starts_with("<!-- SPDX-License-Identifier"));
        assert!(md_text.contains("Top 60 syscalls"));
        assert!(md_text.contains("### shell"));
        assert!(md_text.contains("`one`"));
    }

    #[test]
    fn missing_is_ranked_and_filtered() {
        let temp = TempDir::new("merge-missing");
        let out = temp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let result = result("one", "shell", 0, &[("read", 1, 0), ("write", 1, 0)]);
        fs::write(
            out.join("one.json"),
            serde_json::to_string(&result).unwrap(),
        )
        .unwrap();
        let missing = missing(&out, &HashSet::from(["read".to_string()])).unwrap();
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].name, "write");
    }

    #[test]
    fn implemented_file_parses_comments_and_blanks() {
        let temp = TempDir::new("merge-impl");
        temp.write("impl.txt", "# comment\n\nread\n write \n");
        let set = read_implemented(&temp.path().join("impl.txt")).unwrap();
        assert!(set.contains("read"));
        assert!(set.contains("write"));
        assert_eq!(set.len(), 2);
    }

    #[test]
    fn empty_out_directory_is_an_empty_coverage() {
        let temp = TempDir::new("merge-empty");
        let coverage = build_coverage(&[]);
        assert!(coverage.programs.is_empty());
        assert!(coverage.syscalls.is_empty());
        assert!(temp.path().is_dir());
    }

    #[test]
    fn markdown_lists_skipped_and_not_implemented_sections() {
        let temp = TempDir::new("merge-status");
        let out = temp.path().join("out");
        fs::create_dir_all(&out).unwrap();
        let mut skipped = result("skip", "net", 0, &[]);
        skipped.skipped = true;
        skipped.exit = None;
        skipped.reason = Some("not installed: foo".to_string());
        fs::write(
            out.join("skip.json"),
            serde_json::to_string(&skipped).unwrap(),
        )
        .unwrap();
        let md = temp.path().join("coverage.md");
        fs::write(temp.path().join("impl.txt"), "").unwrap();
        merge(&MergeOptions {
            out,
            json: temp.path().join("coverage.json"),
            md: md.clone(),
            implemented: Some(temp.path().join("impl.txt")),
        })
        .unwrap();
        let text = fs::read_to_string(md).unwrap();
        assert!(text.contains("skipped"));
        assert!(text.contains("Not yet implemented"));
    }
}
