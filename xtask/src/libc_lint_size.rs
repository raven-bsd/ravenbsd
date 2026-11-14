// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-lint size`: the mechanical size rules of `donor-map/16` 3.A.2 (`C69` 4.2, 5.A).
//!
//! Scope notes (reported as deviations):
//! * `[one-symbol]`, `[panic-handler]` and `[hooks]` cover every `raven-libc*` source file.
//! * `[no-fmt]` and `[dyn]` cover the shell crate (`raven-libc/src/**`), the code that is in
//!   every link; the probe rules (`P0` contains no `core3fmt`) and `libc-size monomorph`
//!   measure the archive itself.
//! * `[sections]`, `[no-registry]`, `[statics]` and `[reloc-table]` cover every libc source
//!   file; existing rows are in the reviewer-owned `size-allowlist.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::libc_lint::{Violation, error};

const SHELL: &str = "libc/ravenlibc/crates/raven-libc/src/";

/// One allow-list entry.
#[derive(Clone, Debug, Deserialize)]
pub struct AllowRow {
    /// Repository-relative source path.
    pub file: String,
    /// Why the exception exists.
    #[allow(dead_code)] // read by the reviewer, not by the lint
    #[serde(default)]
    pub reason: String,
}

/// The `size-allowlist.toml` tables the lint reads.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Allow {
    /// R-1 exceptions.
    #[serde(default, rename = "one-symbol")]
    pub one_symbol: Vec<AllowRow>,
    /// R-1 scope exceptions (macro definitions that expand in the shell).
    #[serde(default, rename = "no-mangle")]
    pub no_mangle: Vec<AllowRow>,
    /// R-2 exceptions.
    #[serde(default, rename = "sections")]
    pub sections: Vec<AllowRow>,
    /// R-5 exceptions.
    #[serde(default, rename = "no-fmt")]
    pub no_fmt: Vec<AllowRow>,
    /// R-10 exceptions.
    #[serde(default, rename = "dyn")]
    pub dyn_: Vec<AllowRow>,
    /// R-12 exceptions.
    #[serde(default, rename = "reloc-table")]
    pub reloc_table: Vec<AllowRow>,
}

impl Allow {
    fn allowed(rows: &[AllowRow], rel: &str) -> bool {
        rows.iter().any(|row| row.file == rel)
    }
}

/// Load the allow-list; missing means empty.
///
/// # Errors
/// A parse error.
pub fn load_allow(root: &Path) -> Result<Allow> {
    let path = root.join("tests/libc/size/size-allowlist.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(Allow::default());
    };
    toml::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// Run the check over the scanned files.
///
/// # Errors
/// An I/O or parse failure.
pub(crate) fn run(root: &Path, files: &[PathBuf], violations: &mut Vec<Violation>) -> Result<()> {
    let allow = load_allow(root)?;
    let hooks = load_hook_names(root)?;
    check_panic_count(root, files, violations);
    for file in files {
        let rel = relative(root, file);
        if !is_libc_source(&rel) && !is_sections_scope(&rel) {
            continue;
        }
        let Ok(text) = fs::read_to_string(file) else {
            continue;
        };
        if is_libc_source(&rel) && rel.ends_with(".rs") && !is_test_file(&rel) {
            check_one_symbol(&rel, &text, &allow, violations);
            check_no_mangle_scope(&rel, &text, &allow, violations);
            check_no_fmt(&rel, &text, &allow, violations);
            check_panic_handler(&rel, &text, violations);
            check_no_registry(&rel, &text, violations);
            check_statics(&rel, &text, violations);
            check_hooks(root, &rel, &text, &hooks, violations);
            check_reloc_table(&rel, &text, &allow, violations);
            check_dyn(&rel, &text, &allow, violations);
        }
        if is_sections_scope(&rel) && !rel.ends_with("_ut.rs") {
            check_sections(&rel, &text, &allow, violations);
        }
    }
    Ok(())
}

fn relative(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

fn is_libc_source(rel: &str) -> bool {
    rel.starts_with("libc/ravenlibc/crates/") && rel.contains("/src/")
}

fn is_test_file(rel: &str) -> bool {
    rel.ends_with("_ut.rs") || rel.contains("/tests/") || rel.contains("/probes/")
}

fn is_shell(rel: &str) -> bool {
    rel.starts_with(SHELL)
}

fn is_sections_scope(rel: &str) -> bool {
    is_libc_source(rel)
        || rel.ends_with("Cargo.toml")
        || rel.starts_with(".cargo/")
        || (rel.starts_with("xtask/src/libc") && rel.ends_with(".rs"))
        || rel.starts_with("tools/")
}

fn check_one_symbol(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if !is_shell(rel) || Allow::allowed(&allow.one_symbol, rel) {
        return;
    }
    let mut symbols = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if !line.contains("no_mangle") || !line.trim_start().starts_with('#') {
            continue;
        }
        for following in lines.iter().skip(index + 1).take(4) {
            if let Some(name) = item_name(following) {
                symbols.push(name);
                break;
            }
        }
    }
    if symbols.is_empty() {
        return;
    }
    if symbols.len() > 4 {
        violations.push(error(
            "size",
            "one-symbol",
            rel,
            0,
            format!(
                "{} exported items in one file (R-1 of 16 3.A.2; fix: at most four aliases of one function)",
                symbols.len()
            ),
        ));
    }
    let stem = Path::new(rel)
        .file_stem()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_default();
    let first = symbols.first().map(String::as_str).unwrap_or_default();
    if normalize(first) != normalize(&stem) {
        violations.push(error(
            "size",
            "one-symbol",
            rel,
            0,
            format!(
                "first export `{first}` does not name the file `{stem}` (R-1 of 16 3.A.2; fix: rename the file after its first symbol)"
            ),
        ));
    }
}

fn item_name(line: &str) -> Option<String> {
    let line = line.trim();
    let after = if let Some(position) = line.find("fn ") {
        line.get(position + 3..)
    } else if let Some(position) = line.find("static ") {
        let rest = line.get(position + 7..)?;
        Some(rest.trim_start_matches("mut "))
    } else {
        None
    }?;
    let name: String = after
        .chars()
        .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .collect();
    if name.is_empty() { None } else { Some(name) }
}

fn normalize(value: &str) -> String {
    value.trim_start_matches('_').to_string()
}

fn check_no_mangle_scope(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if is_shell(rel) || Allow::allowed(&allow.no_mangle, rel) {
        return;
    }
    for (index, line) in text.lines().enumerate() {
        if line.contains("no_mangle") && line.trim_start().starts_with('#') {
            violations.push(error(
                "size",
                "no-mangle-scope",
                rel,
                index + 1,
                "`no_mangle` outside the C-ABI shell (rule R-1 of 16 3.A.2; fix: export Rust-ABI items from logic crates)",
            ));
        }
    }
}

const FORBIDDEN_FMT: &[&str] = &[
    "format_args!",
    "write!",
    "writeln!",
    "format!",
    "to_string",
    "println!",
    "print!",
    "eprintln!",
    "dbg!",
    "assert_eq!",
    "assert_ne!",
    "debug_assert_eq!",
    "debug_assert_ne!",
];

fn check_no_fmt(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if !is_shell(rel) || Allow::allowed(&allow.no_fmt, rel) {
        return;
    }
    for (index, line) in crate::libc_lint_scan::scan(text).iter().enumerate() {
        for token in FORBIDDEN_FMT {
            if line.code.contains(token) {
                violations.push(error(
                    "size",
                    "no-fmt",
                    rel,
                    index + 1,
                    format!(
                        "`{token}` pulls `core::fmt` into libc (rule R-5 of 16 3.A.2; fix: write into `&mut [u8]`)"
                    ),
                ));
            }
        }
        if line.code.contains("derive") && line.code.contains("Debug") {
            violations.push(error(
                "size",
                "no-fmt",
                rel,
                index + 1,
                "`#[derive(Debug)]` pulls `core::fmt` (rule R-5 of 16 3.A.2; fix: do not derive Debug in the shell)",
            ));
        }
    }
}

fn check_panic_handler(rel: &str, text: &str, violations: &mut Vec<Violation>) {
    if !is_shell(rel) {
        return;
    }
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if !line.contains("#[panic_handler]") {
            continue;
        }
        let mut body = String::new();
        let mut depth = 0i32;
        let mut entered = false;
        for following in lines.iter().skip(index + 1) {
            for ch in following.chars() {
                if ch == '{' {
                    depth += 1;
                    entered = true;
                    continue;
                }
                if ch == '}' {
                    depth -= 1;
                }
                if entered && depth > 0 {
                    body.push(ch);
                }
            }
            if entered && depth == 0 {
                break;
            }
        }
        for forbidden in ["info", "message", "location", "payload"] {
            if body.contains(forbidden) {
                violations.push(error(
                    "size",
                    "panic-handler",
                    rel,
                    index + 1,
                    format!(
                        "handler body names `{forbidden}` (rule R-6 of 16 3.A.2; fix: fixed string only, never PanicInfo)"
                    ),
                ));
            }
        }
    }
}

fn check_panic_count(root: &Path, files: &[PathBuf], violations: &mut Vec<Violation>) {
    let mut count = 0usize;
    for file in files {
        let rel = relative(root, file);
        if !is_shell(&rel) || !rel.ends_with(".rs") || is_test_file(&rel) {
            continue;
        }
        if let Ok(text) = fs::read_to_string(file) {
            count += text.matches("#[panic_handler]").count();
        }
    }
    let crate_root = root
        .join("libc/ravenlibc/crates/raven-libc/src/lib.rs")
        .is_file();
    if count != 1 && (count > 0 || crate_root) {
        violations.push(error(
            "size",
            "panic-handler",
            "libc/ravenlibc/crates/raven-libc/src/panic.rs",
            0,
            format!(
                "{count} handlers in the shell crate, exactly one required (rule R-6 of 16 3.A.2; fix: feature `panic-handler` has one handler)"
            ),
        ));
    }
}

fn check_no_registry(rel: &str, text: &str, violations: &mut Vec<Violation>) {
    for (index, line) in crate::libc_lint_scan::scan(text).iter().enumerate() {
        for token in ["inventory::", "linkme", "init_all"] {
            if line.code.contains(token) {
                violations.push(error(
                    "size",
                    "no-registry",
                    rel,
                    index + 1,
                    format!(
                        "`{token}` is a subsystem registry (rule R-9 of 16 3.A.2; fix: lazy start or a weak hook)"
                    ),
                ));
            }
        }
        if line.code.contains("link_section") && line.code.contains("init_array") {
            violations.push(error(
                "size",
                "no-registry",
                rel,
                index + 1,
                "libc item in `.init_array` (rule R-9 of 16 3.A.2; fix: libc adds no constructors)",
            ));
        }
        if line.code.contains("static") && line.code.contains('[') && line.code.contains("fn(") {
            violations.push(error(
                "size",
                "no-registry",
                rel,
                index + 1,
                "static array of function pointers (rule R-9 of 16 3.A.2; fix: no init registry)",
            ));
        }
    }
}

fn check_statics(rel: &str, text: &str, violations: &mut Vec<Violation>) {
    for (index, line) in crate::libc_lint_scan::scan(text).iter().enumerate() {
        for token in [
            "lazy_static",
            "OnceLock",
            "LazyLock",
            "OnceCell",
            "once_cell",
        ] {
            if line.code.contains(token) {
                violations.push(error(
                    "size",
                    "statics",
                    rel,
                    index + 1,
                    format!(
                        "`{token}` is a lazy cell (rule R-28 of 16 3.A.2; fix: `AtomicU8`/`AtomicPtr` state machine)"
                    ),
                ));
            }
        }
    }
}

struct Hooks {
    names: Vec<String>,
}

fn load_hook_names(root: &Path) -> Result<Hooks> {
    #[derive(Deserialize)]
    struct File {
        #[serde(default, rename = "hook")]
        hook: Vec<Row>,
    }
    #[derive(Deserialize)]
    struct Row {
        hook: String,
    }
    let path = root.join("tests/libc/size/hooks.toml");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(Hooks { names: Vec::new() });
    };
    let file: File = toml::from_str(&text).with_context(|| format!("parse {}", path.display()))?;
    Ok(Hooks {
        names: file.hook.into_iter().map(|row| row.hook).collect(),
    })
}

fn check_hooks(root: &Path, rel: &str, text: &str, hooks: &Hooks, violations: &mut Vec<Violation>) {
    if !is_shell(rel) {
        return;
    }
    let lines: Vec<&str> = text.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if !line.contains("weak_hook!") {
            continue;
        }
        let mut name = None;
        for following in lines.iter().skip(index + 1).take(6) {
            if let Some(found) = item_name(following) {
                name = Some(found);
                break;
            }
        }
        let Some(name) = name else {
            continue;
        };
        if !hooks.names.iter().any(|hook| hook == &name) {
            violations.push(error(
                "size",
                "hooks",
                rel,
                index + 1,
                format!(
                    "`{name}` is not a row of hooks.toml (rule R-18 of 16 3.A.2; fix: add the hook to the table)"
                ),
            ));
            continue;
        }
        if let Some(member) = hook_member(root, &name)
            && !member.is_file()
        {
            violations.push(error(
                "size",
                "hooks",
                rel,
                index + 1,
                format!(
                    "strong member {} is missing (rule R-18 of 16 3.A.2; fix: define the strong hook in another file)",
                    member.display()
                ),
            ));
        }
    }
}

fn hook_member(root: &Path, name: &str) -> Option<PathBuf> {
    let path = root.join("tests/libc/size/hooks.toml");
    let text = fs::read_to_string(path).ok()?;
    #[derive(Deserialize)]
    struct File {
        #[serde(default, rename = "hook")]
        hook: Vec<Row>,
    }
    #[derive(Deserialize)]
    struct Row {
        hook: String,
        strong_member: String,
    }
    let file: File = toml::from_str(&text).ok()?;
    let row = file.hook.into_iter().find(|row| row.hook == name)?;
    let directory = root.join("libc/ravenlibc/crates/raven-libc/src/hooks");
    let stem = row.strong_member.trim_end_matches(".o");
    let primary = directory.join(format!("{stem}.rs"));
    if primary.is_file() {
        return Some(primary);
    }
    // The shell names the strong member file after the hook stem, not after the donor object
    // (`__rl_funcs_on_exit` lives in `hooks/funcs_on_exit.rs`, not `hooks/atexit.rs`).
    let hook_stem = name.trim_start_matches("__rl_");
    Some(directory.join(format!("{hook_stem}.rs")))
}

fn check_reloc_table(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if Allow::allowed(&allow.reloc_table, rel) {
        return;
    }
    for (index, line) in crate::libc_lint_scan::scan(text).iter().enumerate() {
        let code = &line.code;
        if code.contains("static")
            && (code.contains("&'static [&'static str]")
                || code.contains("&'static [&str]")
                || code.contains("&[&str]"))
        {
            violations.push(error(
                "size",
                "reloc-table",
                rel,
                index + 1,
                "table of string pointers adds one relocation per element (rule R-12 of 16 3.A.2; fix: one string blob plus u16 offsets)",
            ));
        }
    }
}

fn check_dyn(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if !is_shell(rel) || Allow::allowed(&allow.dyn_, rel) {
        return;
    }
    for (index, line) in crate::libc_lint_scan::scan(text).iter().enumerate() {
        if line.code.contains("dyn ") {
            violations.push(error(
                "size",
                "dyn",
                rel,
                index + 1,
                "trait object in the shell (rule R-10 of 16 3.A.2; fix: a vtable is a relocation in libc.so)",
            ));
        }
    }
}

fn check_sections(rel: &str, text: &str, allow: &Allow, violations: &mut Vec<Violation>) {
    if Allow::allowed(&allow.sections, rel) {
        return;
    }
    let lines: Vec<&str> = text.lines().collect();
    // Rust source: comments and literals are blanked (a token in a comment is not a defect).
    // TOML/build files: the string *is* the configuration, so scan the raw line.
    let scanned: Vec<String> = if rel.ends_with(".rs") {
        crate::libc_lint_scan::scan(text)
            .into_iter()
            .map(|line| line.code)
            .collect()
    } else {
        lines.iter().map(|line| (*line).to_string()).collect()
    };
    for (index, code) in scanned.iter().enumerate() {
        if code.contains("link_section") {
            let near_function = lines
                .iter()
                .skip(index.saturating_sub(3))
                .take(6)
                .any(|candidate| candidate.contains("fn "));
            if near_function {
                violations.push(error(
                    "size",
                    "sections",
                    rel,
                    index + 1,
                    "`link_section` on a function (rule R-2 of 16 3.A.2; fix: per-item sections are the stable default)",
                ));
            }
        }
        for token in [
            "link-dead-code",
            "no-gc-sections",
            "--no-gc-sections",
            concat!("-", "Zfunction-sections"),
        ] {
            if code.contains(token) {
                violations.push(error(
                    "size",
                    "sections",
                    rel,
                    index + 1,
                    format!(
                        "`{token}` disables section GC (rule R-2 of 16 3.A.2; fix: keep `--gc-sections`)"
                    ),
                ));
            }
        }
    }
}

/// `R-3`: the three libc profiles are pinned in the root `Cargo.toml`.
#[cfg(test)]
#[must_use]
pub fn profiles_pinned(text: &str) -> Vec<String> {
    let mut problems = Vec::new();
    let value: toml::Value = match toml::from_str(text) {
        Ok(value) => value,
        Err(error) => return vec![format!("cannot parse Cargo.toml: {error}")],
    };
    let profiles = value.get("profile");
    let check = |name: &str, key: &str, expected: toml::Value, problems: &mut Vec<String>| {
        let actual = profiles
            .and_then(|profiles| profiles.get(name))
            .and_then(|profile| profile.get(key));
        if actual != Some(&expected) {
            problems.push(format!(
                "[profile.{name}] {key} must be {expected}, got {actual:?}"
            ));
        }
    };
    check(
        "libc-a",
        "inherits",
        toml::Value::String("release".to_string()),
        &mut problems,
    );
    check(
        "libc-a",
        "opt-level",
        toml::Value::String("z".to_string()),
        &mut problems,
    );
    check(
        "libc-a",
        "codegen-units",
        toml::Value::Integer(4096),
        &mut problems,
    );
    check("libc-a", "lto", toml::Value::Boolean(false), &mut problems);
    check(
        "libc-a",
        "panic",
        toml::Value::String("abort".to_string()),
        &mut problems,
    );
    check(
        "libc-a",
        "debug",
        toml::Value::Boolean(false),
        &mut problems,
    );
    check(
        "libc-a",
        "incremental",
        toml::Value::Boolean(false),
        &mut problems,
    );
    check(
        "libc-a",
        "strip",
        toml::Value::String("none".to_string()),
        &mut problems,
    );
    check(
        "libc-a-server",
        "opt-level",
        toml::Value::String("s".to_string()),
        &mut problems,
    );
    check(
        "libc-so",
        "opt-level",
        toml::Value::String("s".to_string()),
        &mut problems,
    );
    check(
        "libc-so",
        "lto",
        toml::Value::String("fat".to_string()),
        &mut problems,
    );
    check(
        "libc-so",
        "codegen-units",
        toml::Value::Integer(1),
        &mut problems,
    );
    check(
        "libc-so",
        "panic",
        toml::Value::String("abort".to_string()),
        &mut problems,
    );
    problems
}

#[cfg(test)]
#[path = "libc_lint_size_ut.rs"]
mod tests;
