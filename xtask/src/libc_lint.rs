// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `cargo xtask libc-lint`: executable form of the ravenlibc layout rules.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Result, bail};

use crate::libc_lint_data::{self, Crates};
use crate::libc_lint_scan;

/// One lint finding.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Violation {
    /// Check name.
    pub check: String,
    /// Stable key.
    pub key: String,
    /// Repository-relative path.
    pub path: String,
    /// One-based line, 0 for file-level.
    pub line: usize,
    /// Human-readable message.
    pub message: String,
}

/// Check names accepted by the parser.
pub const ALL_CHECKS: &[&str] = &[
    "unsafe-scope",
    "cfg",
    "header",
    "provenance-link",
    "no-std",
    "no-cc",
    "layout",
];

/// Checks parsed but not implemented by C34.
pub const STUB_CHECKS: &[&str] = &[
    "deps",
    "one-symbol",
    "sections",
    "no-fmt",
    "panic-handler",
    "no-alloc",
    "no-registry",
    "hooks",
    "exports",
    "statics",
    "hot",
    "owns",
];

fn error(check: &str, key: &str, path: &str, line: usize, message: impl Into<String>) -> Violation {
    Violation {
        check: check.to_string(),
        key: key.to_string(),
        path: path.to_string(),
        line,
        message: message.into(),
    }
}

fn walk(root: &Path, rel: &str, files: &mut Vec<PathBuf>) {
    let directory = root.join(rel);
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            walk(
                root,
                &path.strip_prefix(root).unwrap_or(&path).to_string_lossy(),
                files,
            );
        } else {
            files.push(path);
        }
    }
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Run the selected checks over `root`.
pub fn run_checks(root: &Path, checks: &[String]) -> Result<Vec<Violation>> {
    let selected: Vec<&str> = if checks.is_empty() || checks.iter().any(|check| check == "all") {
        ALL_CHECKS.to_vec()
    } else {
        checks.iter().map(String::as_str).collect()
    };
    for check in &selected {
        if STUB_CHECKS.contains(check) {
            bail!("libc-lint {check}: not implemented by C34");
        }
        if !ALL_CHECKS.contains(check) {
            bail!("unknown check `{check}`");
        }
    }
    let (crates, features, allow, tree) = libc_lint_data::load(root)?;
    let provenance = libc_lint_data::load_provenance(root)?;
    let mut violations = Vec::new();
    let mut files = Vec::new();
    for scope in [
        "libc/ravenlibc/crates",
        "libc/ravenlibc/crt",
        "libc/ravenlibc/ld",
        "libc/ravenlibc/tools",
        "tests/libc",
    ] {
        walk(root, scope, &mut files);
    }
    files.sort();
    let class = crate_classes(&crates);
    for file in &files {
        let rel = relative(root, file);
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        if selected.contains(&"header") {
            check_header(root, &rel, &text, &provenance, &mut violations);
        }
        if selected.contains(&"provenance-link") {
            check_provenance(&rel, &text, &provenance, &mut violations);
        }
        if rel.ends_with(".rs") {
            if selected.contains(&"unsafe-scope") {
                check_unsafe(&rel, &text, &crates, &allow, &mut violations);
            }
            if selected.contains(&"no-std") {
                check_no_std(&rel, &text, &crates, &mut violations);
            }
            if selected.contains(&"cfg") && rel.ends_with("Cargo.toml") {
                check_cfg(&rel, &text, &features, &mut violations);
            }
        }
        if rel.ends_with("Cargo.toml") {
            if selected.contains(&"no-cc") {
                check_no_cc(&rel, &text, &mut violations);
            }
            if selected.contains(&"cfg") {
                check_cfg(&rel, &text, &features, &mut violations);
            }
        }
        if selected.contains(&"no-cc")
            && [".c", ".cc", ".cpp", ".cxx", ".s", ".S", ".h", ".hpp"]
                .iter()
                .any(|extension| rel.ends_with(extension))
        {
            violations.push(error(
                "no-cc",
                "c-source",
                &rel,
                1,
                "C/C++ source is forbidden in ravenlibc crates",
            ));
        }
    }
    if selected.contains(&"layout") {
        check_layout(root, &crates, &tree, &mut violations)?;
    }
    violations.sort();
    let _ = class;
    Ok(violations)
}

fn crate_classes(crates: &Crates) -> BTreeMap<String, String> {
    crates
        .rows
        .iter()
        .map(|row| (row.path.clone(), row.unsafe_.clone()))
        .collect()
}

fn check_header(
    root: &Path,
    rel: &str,
    text: &str,
    imports: &[libc_lint_data::Import],
    violations: &mut Vec<Violation>,
) {
    let first: Vec<&str> = text.lines().take(5).collect();
    let has_spdx = first
        .iter()
        .any(|line| line.contains("SPDX-License-Identifier:"));
    let marker = imports.iter().find(|entry| entry.path == rel);
    if !has_spdx {
        violations.push(error(
            "header",
            "missing-spdx",
            rel,
            0,
            "no SPDX-License-Identifier in the first 5 lines",
        ));
    }
    if marker.is_none() {
        let copyright = "Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors";
        let has_copyright = first.iter().any(|line| {
            line.replace("<!--", "")
                .replace("-->", "")
                .replace("//", "")
                .trim_start_matches('#')
                .trim()
                == copyright
        });
        if !has_copyright {
            violations.push(error(
                "header",
                "missing-copyright",
                rel,
                0,
                "first-party file lacks the RavenBSD copyright line in its first 5 lines",
            ));
        }
        if !first
            .iter()
            .any(|line| line.contains("SPDX-License-Identifier: BSD-3-Clause"))
        {
            violations.push(error(
                "header",
                "wrong-spdx",
                rel,
                0,
                "first-party file must be BSD-3-Clause",
            ));
        }
    } else if let Some(import) = marker
        && !first
            .iter()
            .any(|line| line.contains("SPDX-License-Identifier:") && line.contains(&import.license))
    {
        {
            violations.push(error(
                "header",
                "import-spdx-mismatch",
                rel,
                0,
                "file SPDX does not match its import entry",
            ));
        }
    }
    if text.contains("Generated") && !first.iter().any(|line| line.contains("Generated")) {
        // generated markers are first-10-lines by convention; keep this cheap
    }
    let _ = root;
}

fn check_provenance(
    rel: &str,
    text: &str,
    imports: &[libc_lint_data::Import],
    violations: &mut Vec<Violation>,
) {
    let has_marker = text.contains("RavenBSD: transpiled from");
    let entry = imports.iter().find(|entry| entry.path == rel);
    if has_marker && entry.is_none() {
        violations.push(error(
            "provenance-link",
            "no-import",
            rel,
            1,
            "file carries \"RavenBSD: transpiled from\" but no [[import]] entry names it",
        ));
    }
    if !has_marker && let Some(import) = entry {
        violations.push(error(
            "provenance-link",
            "import-no-marker",
            rel,
            1,
            format!("import entry for {} lacks the marker", import.upstream_path),
        ));
    }
}

fn check_unsafe(
    rel: &str,
    text: &str,
    crates: &Crates,
    allow: &libc_lint_data::Allowlist,
    violations: &mut Vec<Violation>,
) {
    if !rel.contains("/src/") {
        return;
    }
    let class = crates
        .rows
        .iter()
        .find(|row| rel.starts_with(&format!("libc/ravenlibc/{}", row.path)))
        .map(|row| row.unsafe_.as_str())
        .unwrap_or("P");
    let lines = libc_lint_scan::scan(text);
    if matches!(class, "P" | "M") {
        let root_text = lines
            .iter()
            .map(|line| line.code.as_str())
            .collect::<String>();
        let attr = if class == "P" {
            "#![forbid(unsafe_code)]"
        } else {
            "#![deny(unsafe_code)]"
        };
        if rel.ends_with("src/lib.rs") || rel.ends_with("src/main.rs") {
            let compact: String = root_text.chars().filter(|c| !c.is_whitespace()).collect();
            if !compact.contains(&attr.replace(' ', "")) {
                violations.push(error(
                    "unsafe-scope",
                    "root-attr",
                    rel,
                    0,
                    format!("crate root lacks {attr}"),
                ));
            }
        }
    }
    for (index, line) in lines.iter().enumerate() {
        if !line.code.contains("unsafe") {
            continue;
        }
        let line_no = index + 1;
        let allowed = allow.rows.iter().any(|row| {
            rel.starts_with(&format!(
                "libc/ravenlibc/{}/{}",
                crate_path(crates, &row.crate_),
                row.path
            ))
        });
        if class == "P" {
            violations.push(error(
                "unsafe-scope",
                "forbidden",
                rel,
                line_no,
                "`unsafe` is forbidden in a class P crate",
            ));
        } else if class == "M" && !allowed {
            violations.push(error(
                "unsafe-scope",
                "not-allowlisted",
                rel,
                line_no,
                "`unsafe` in a class M crate outside xtask/unsafe-allowlist.toml",
            ));
        }
        if line.code.contains("unsafe {") || line.code.contains("unsafe impl") {
            let safety = line.comment.contains("SAFETY:")
                || lines[index.saturating_sub(8)..index]
                    .iter()
                    .any(|previous| previous.comment.contains("SAFETY:"));
            if !safety {
                violations.push(error(
                    "unsafe-scope",
                    "missing-safety",
                    rel,
                    line_no,
                    "`unsafe` block without a `// SAFETY:` comment",
                ));
            }
        }
    }
}

fn crate_path<'a>(crates: &'a Crates, name: &str) -> &'a str {
    crates
        .rows
        .iter()
        .find(|row| row.name == name)
        .map(|row| row.path.as_str())
        .unwrap_or("crates/unknown")
}

fn check_no_std(rel: &str, text: &str, crates: &Crates, violations: &mut Vec<Violation>) {
    let Some(row) = crates
        .rows
        .iter()
        .find(|row| rel.starts_with(&format!("libc/ravenlibc/{}", row.path)))
    else {
        return;
    };
    let lines = libc_lint_scan::scan(text);
    for (index, line) in lines.iter().enumerate() {
        if rel.ends_with("_ut.rs") || rel.contains("/tests/") {
            continue;
        }
        if line.code.contains("std::") || line.code.contains("use std") {
            violations.push(error(
                "no-std",
                "std-path",
                rel,
                index + 1,
                "path through `std` in a shipped crate",
            ));
        }
        if row.std == "core"
            && !row.alloc_feature
            && (line.code.contains("alloc::") || line.code.contains("extern crate alloc"))
        {
            violations.push(error(
                "no-std",
                "alloc-in-core",
                rel,
                index + 1,
                "alloc used by a core-only crate",
            ));
        }
    }
}

fn check_cfg(
    rel: &str,
    text: &str,
    features: &libc_lint_data::Features,
    violations: &mut Vec<Violation>,
) {
    let allowed: Vec<&str> = features.rows.iter().map(|row| row.name.as_str()).collect();
    if rel.ends_with("Cargo.toml") {
        let mut in_features = false;
        for (index, line) in text.lines().enumerate() {
            let trimmed = line.trim();
            if trimmed.starts_with('[') {
                in_features = trimmed == "[features]";
                continue;
            }
            if in_features
                && let Some(name) = trimmed.split('=').next().map(str::trim)
                && !name.is_empty()
                && !allowed.contains(&name)
            {
                violations.push(error(
                    "cfg",
                    "unknown-feature-key",
                    rel,
                    index + 1,
                    format!("feature `{name}` is not in features.toml"),
                ));
            }
        }
    }
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("feature = \"")
            && let Some(name) = trimmed.split('"').nth(1)
            && !allowed.contains(&name)
        {
            violations.push(error(
                "cfg",
                "unknown-feature-cfg",
                rel,
                index + 1,
                format!("feature `{name}` is not in features.toml"),
            ));
        }
    }
}

fn check_no_cc(rel: &str, text: &str, violations: &mut Vec<Violation>) {
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if ["cc", "cmake", "bindgen", "cbindgen", "libloading"]
            .iter()
            .any(|name| {
                trimmed.starts_with(&format!("{name} ="))
                    || trimmed.starts_with(&format!("{name}."))
            })
        {
            violations.push(error(
                "no-cc",
                "banned-dep",
                rel,
                index + 1,
                "C/C++ build dependency is forbidden in ravenlibc crates (hard rule 2)",
            ));
        }
        if trimmed.starts_with("build =") {
            violations.push(error(
                "no-cc",
                "build-script",
                rel,
                index + 1,
                "build scripts are forbidden in ravenlibc crates",
            ));
        }
    }
}

fn check_layout(
    root: &Path,
    crates: &Crates,
    tree: &libc_lint_data::Tree,
    violations: &mut Vec<Violation>,
) -> Result<()> {
    let base = root.join("libc/ravenlibc");
    for entry in &tree.entries {
        if entry.required {
            let path = base.join(&entry.path);
            let exists = if entry.kind == "dir" {
                path.is_dir()
            } else {
                path.is_file()
            };
            if !exists {
                violations.push(error(
                    "layout",
                    "missing-required",
                    &format!("libc/ravenlibc/{}", entry.path),
                    0,
                    "required tree entry is missing",
                ));
            }
        }
    }
    if base.join("Cargo.toml").exists() {
        violations.push(error(
            "layout",
            "forbidden-file",
            "libc/ravenlibc/Cargo.toml",
            0,
            "libc/ravenlibc/Cargo.toml must not exist (manifest-less Cargo.toml breaks cargo)",
        ));
    }
    let crates_dir = base.join("crates");
    let mut has_plain = false;
    let mut has_crate = false;
    if let Ok(entries) = fs::read_dir(&crates_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("Cargo.toml").is_file() {
                    has_crate = true;
                } else {
                    violations.push(error(
                        "layout",
                        "crate-no-manifest",
                        &format!(
                            "libc/ravenlibc/crates/{}",
                            entry.file_name().to_string_lossy()
                        ),
                        0,
                        "crate directory has no Cargo.toml",
                    ));
                }
            } else {
                has_plain = true;
            }
        }
    }
    if !has_plain && !has_crate {
        violations.push(error(
            "layout",
            "glob-guard",
            "libc/ravenlibc/crates",
            0,
            "crates/ needs a plain file or a crate with Cargo.toml",
        ));
    }
    for row in &crates.rows {
        for dep in &row.deps {
            if !crates.rows.iter().any(|candidate| &candidate.name == dep) {
                violations.push(error(
                    "layout",
                    "crates-toml",
                    "libc/ravenlibc/xtask/crates.toml",
                    0,
                    format!("missing dependency {dep}"),
                ));
            }
        }
    }
    Ok(())
}

/// Count the files in the lint scopes.
#[must_use]
pub fn count_files(root: &Path) -> usize {
    let mut files = Vec::new();
    for scope in [
        "libc/ravenlibc/crates",
        "libc/ravenlibc/crt",
        "libc/ravenlibc/ld",
        "libc/ravenlibc/tools",
        "tests/libc",
    ] {
        walk(root, scope, &mut files);
    }
    files.len()
}

/// Render violations as the stable text format.
#[must_use]
pub fn render(violations: &[Violation]) -> String {
    let mut out = String::new();
    for violation in violations {
        out.push_str(&format!(
            "{}:{}: {}: [{}] {}\n",
            violation.path, violation.line, violation.check, violation.key, violation.message
        ));
    }
    out
}

/// JSON rendering (hand-written, stable key order).
#[must_use]
pub fn json(violations: &[Violation], files_scanned: usize) -> String {
    let mut out = format!("{{\"files_scanned\":{files_scanned},\"violations\":[");
    for (index, violation) in violations.iter().enumerate() {
        if index > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"check\":\"{}\",\"key\":\"{}\",\"path\":\"{}\",\"line\":{},\"message\":\"{}\"}}",
            escape(&violation.check),
            escape(&violation.key),
            escape(&violation.path),
            violation.line,
            escape(&violation.message)
        ));
    }
    out.push_str("]}");
    out
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
#[path = "libc_lint_ut.rs"]
mod tests;
