// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Native ABI table generator and diff tool.
//!
//! Reads the `raven-abi` contract source (never a hand-edited copy) and emits a deterministic,
//! machine-readable table of the native ABI: every syscall (number, name, argument count, return)
//! and every `#[repr]` wire struct (name, size, alignment). `diff` compares two tables and reports
//! additions, removals and layout changes.

use std::collections::BTreeMap;
use std::process::ExitCode;

/// One syscall entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Syscall {
    /// Raw syscall number.
    pub number: u64,
    /// Uppercase variant/const name.
    pub name: String,
    /// Argument count from the documented signature (`0` when the signature is absent).
    pub argc: usize,
    /// Return type from the documented signature, whitespace-stripped.
    pub ret: String,
}

/// One wire struct entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Struct {
    /// Type name.
    pub name: String,
    /// Size in bytes from the `size_of` assertion (`0` when not asserted).
    pub size: u64,
    /// Alignment from the `align_of` assertion (`0` when not asserted).
    pub align: u64,
}

/// A parsed ABI table.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Abi {
    /// Syscalls, in source order.
    pub syscalls: Vec<Syscall>,
    /// Structs, in source order.
    pub structs: Vec<Struct>,
}

/// Parse every `*.rs` source file in `dir` (sorted by name) into an [`Abi`].
pub fn parse_dir(dir: &std::path::Path) -> Result<Abi, String> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
        let path = entry.map_err(|e| e.to_string())?.path();
        if path.extension().is_some_and(|e| e == "rs") {
            files.push(path);
        }
    }
    files.sort();
    let mut abi = Abi::default();
    for path in files {
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        abi.syscalls.extend(parse_syscalls(&text));
        abi.structs.extend(parse_structs(&text));
    }
    abi.syscalls.sort_by_key(|s| s.number);
    abi.syscalls.dedup_by(|a, b| a.number == b.number);
    abi.structs.sort_by(|a, b| a.name.cmp(&b.name));
    abi.structs.dedup_by(|a, b| a.name == b.name);
    Ok(abi)
}

/// Parse syscall declarations from one source file.
pub fn parse_syscalls(src: &str) -> Vec<Syscall> {
    let mut out = Vec::new();
    let mut doc: Vec<&str> = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("///") {
            doc.push(rest.trim_start());
            continue;
        }
        if let Some((name, number)) = parse_syscall_line(trimmed) {
            let signature = doc.first().copied().unwrap_or("");
            let (argc, ret) = parse_signature(signature);
            out.push(Syscall {
                number,
                name,
                argc,
                ret,
            });
        }
        doc.clear();
    }
    out
}

/// Match one syscall declaration: `NAME = 0xNN;` or `pub const NAME: usize = 0xNN;`.
fn parse_syscall_line(line: &str) -> Option<(String, u64)> {
    if let Some(rest) = line.strip_suffix(';')
        && let Some((name, value)) = rest.split_once('=')
    {
        let name = name.trim();
        let value = value.trim();
        if is_ident(name)
            && let Some(number) = parse_hex(value)
        {
            return Some((name.to_string(), number));
        }
    }
    if let Some(rest) = line.strip_prefix("pub const ")
        && let Some(rest) = rest.strip_suffix(';')
        && let Some((name, value)) = rest.split_once(": usize = ")
        && is_ident(name)
        && let Some(number) = parse_hex(value)
    {
        return Some((name.to_string(), number));
    }
    None
}

/// True when `s` is a plain uppercase identifier (a syscall name, not `_` or a keyword).
fn is_ident(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
        && s.chars().next().is_some_and(|c| c.is_ascii_uppercase())
}

/// Parse a `0xNN` literal.
fn parse_hex(s: &str) -> Option<u64> {
    let digits = s.strip_prefix("0x")?;
    u64::from_str_radix(digits, 16).ok()
}

/// Parse a documented signature `` `name(args) -> ret` `` into `(argc, ret)`.
fn parse_signature(doc: &str) -> (usize, String) {
    let doc = doc.trim();
    let Some(start) = doc.find('`') else {
        return (0, String::new());
    };
    let Some(end) = doc[start + 1..].find('`') else {
        return (0, String::new());
    };
    let signature = &doc[start + 1..start + 1 + end];
    let Some(open) = signature.find('(') else {
        return (0, String::new());
    };
    let Some(close) = signature.find(')') else {
        return (0, String::new());
    };
    let args = &signature[open + 1..close];
    let ret = signature[close + 1..]
        .trim()
        .strip_prefix("->")
        .unwrap_or("")
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    let argc = if args.trim().is_empty() {
        0
    } else {
        args.split(',').count()
    };
    (argc, ret)
}

/// Parse `#[repr]` wire structs and their size/alignment assertions from one source file.
pub fn parse_structs(src: &str) -> Vec<Struct> {
    // First collect the declared struct names in order, tagging those preceded by a `#[repr]`
    // attribute (looking back a couple of lines past any `#[derive]`).
    let mut names = Vec::new();
    let mut recent: Vec<&str> = Vec::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(name) = trimmed.strip_prefix("pub struct ") {
            let name = name.split(['(', '{', '<']).next().unwrap_or("").trim();
            if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
                let repr = recent.iter().any(|l| l.contains("repr"));
                names.push((name.to_string(), repr));
            }
        }
        recent.push(trimmed);
        if recent.len() > 3 {
            recent.remove(0);
        }
    }
    // Then collect size/align assertions per name.
    let mut size: BTreeMap<String, u64> = BTreeMap::new();
    let mut align: BTreeMap<String, u64> = BTreeMap::new();
    for line in src.lines() {
        let trimmed = line.trim();
        if let Some(idx) = trimmed.find("size_of::<")
            && let Some((name, value)) = split_assert(&trimmed[idx + "size_of::<".len()..])
            && let Some(n) = parse_literal(&value)
        {
            size.entry(name).or_insert(n);
        } else if let Some(idx) = trimmed.find("align_of::<")
            && let Some((name, value)) = split_assert(&trimmed[idx + "align_of::<".len()..])
            && let Some(n) = parse_literal(&value)
        {
            align.entry(name).or_insert(n);
        }
    }
    names
        .into_iter()
        .filter(|(_, repr)| *repr)
        .map(|(name, _)| Struct {
            size: size.get(&name).copied().unwrap_or(0),
            align: align.get(&name).copied().unwrap_or(0),
            name,
        })
        .collect()
}

/// Split `Name>() == VALUE);` into `(Name, VALUE)`.
fn split_assert(rest: &str) -> Option<(String, String)> {
    let close = rest.find(">() == ")?;
    let name = rest[..close].to_string();
    let value = rest[close + ">() == ".len()..]
        .trim_end_matches(");")
        .trim()
        .to_string();
    Some((name, value))
}

/// Parse a non-negative integer literal (`24`, `5 * UNAME_FIELD_LEN` -> `None`).
fn parse_literal(s: &str) -> Option<u64> {
    s.trim().parse().ok()
}

/// Render an ABI table as a deterministic, sorted, machine-readable string.
pub fn render(abi: &Abi) -> String {
    let mut syscalls = abi.syscalls.clone();
    syscalls.sort_by_key(|s| s.number);
    syscalls.dedup_by_key(|s| s.number);
    let mut structs = abi.structs.clone();
    structs.sort_by(|a, b| a.name.cmp(&b.name));
    structs.dedup_by(|a, b| a.name == b.name);
    let mut out = String::new();
    for s in &syscalls {
        out.push_str(&format!(
            "SYSCALL {:#04x} {} argc={} ret={}\n",
            s.number, s.name, s.argc, s.ret
        ));
    }
    for t in &structs {
        out.push_str(&format!(
            "STRUCT {} size={} align={}\n",
            t.name, t.size, t.align
        ));
    }
    out
}

/// One kind of ABI change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// A syscall or struct was added.
    Added,
    /// A syscall or struct was removed.
    Removed,
    /// A syscall's number or a struct's layout changed.
    Changed,
}

/// The result of diffing two tables.
#[derive(Clone, Debug, Default)]
pub struct DiffReport {
    /// Human-readable lines describing each change.
    pub lines: Vec<String>,
    /// Whether any removal or layout change occurred (additions are allowed).
    pub breaking: bool,
}

/// Diff two rendered tables.
pub fn diff(base: &str, head: &str) -> DiffReport {
    let base = parse_rendered(base);
    let head = parse_rendered(head);
    let mut report = DiffReport::default();

    // Syscalls by number.
    for (number, name) in &head.syscalls {
        match base.syscalls.get(number) {
            Some(base_name) if base_name != name => {
                report.lines.push(format!(
                    "CHANGED syscall {number:#04x}: {base_name} -> {name}"
                ));
                report.breaking = true;
            }
            None => report
                .lines
                .push(format!("ADDED syscall {number:#04x} {name}")),
            _ => {}
        }
    }
    for (number, name) in &base.syscalls {
        if !head.syscalls.contains_key(number) {
            report
                .lines
                .push(format!("REMOVED syscall {number:#04x} {name}"));
            report.breaking = true;
        }
    }

    // Structs by name.
    for (name, (size, align)) in &head.structs {
        match base.structs.get(name) {
            Some((base_size, base_align)) if (base_size, base_align) != (size, align) => {
                report.lines.push(format!(
                    "CHANGED struct {name}: size {base_size}->{size} align {base_align}->{align}"
                ));
                report.breaking = true;
            }
            None => report
                .lines
                .push(format!("ADDED struct {name} size={size} align={align}")),
            _ => {}
        }
    }
    for (name, (size, align)) in &base.structs {
        if !head.structs.contains_key(name) {
            report
                .lines
                .push(format!("REMOVED struct {name} size={size} align={align}"));
            report.breaking = true;
        }
    }
    report
}

/// Parse a rendered table back into maps keyed by number (syscalls) and name (structs).
fn parse_rendered(text: &str) -> Rendered {
    let mut syscalls = BTreeMap::new();
    let mut structs = BTreeMap::new();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("SYSCALL ") {
            let mut parts = rest.split_whitespace();
            let number = parts.next().and_then(parse_hex).unwrap_or(0);
            let name = parts.next().unwrap_or("").to_string();
            syscalls.insert(number, name);
        } else if let Some(rest) = line.strip_prefix("STRUCT ") {
            let name = rest.split_whitespace().next().unwrap_or("").to_string();
            let mut size = 0u64;
            let mut align = 0u64;
            for field in rest.split_whitespace().skip(1) {
                if let Some(v) = field.strip_prefix("size=") {
                    size = v.parse().unwrap_or(0);
                } else if let Some(v) = field.strip_prefix("align=") {
                    align = v.parse().unwrap_or(0);
                }
            }
            structs.insert(name, (size, align));
        }
    }
    Rendered { syscalls, structs }
}

struct Rendered {
    syscalls: BTreeMap<u64, String>,
    structs: BTreeMap<String, (u64, u64)>,
}

/// Read the ABI source under `dir` and print the rendered table.
fn generate(dir: &std::path::Path) -> Result<String, String> {
    let abi = parse_dir(dir)?;
    Ok(render(&abi))
}

/// The CLI entry point.
pub fn run() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let default_source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .unwrap_or(std::path::Path::new("."))
        .join("crates/raven-abi/src");
    match args.get(1).map(String::as_str) {
        Some("generate") => {
            let source = if args.get(2).map(String::as_str) == Some("--source") {
                args.get(3).map(std::path::PathBuf::from)
            } else {
                None
            }
            .unwrap_or(default_source);
            match generate(&source) {
                Ok(table) => {
                    print!("{table}");
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("abi-diff: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Some("diff") if args.len() >= 4 => {
            let base = std::fs::read_to_string(&args[2]).unwrap_or_default();
            let head = std::fs::read_to_string(&args[3]).unwrap_or_default();
            let report = diff(&base, &head);
            for line in &report.lines {
                println!("{line}");
            }
            if report.lines.is_empty() {
                println!("abi-diff: no changes");
            }
            if report.breaking {
                ExitCode::from(1)
            } else {
                ExitCode::SUCCESS
            }
        }
        _ => {
            eprintln!(
                "usage: abi-diff generate [--source <dir>] | abi-diff diff <base.txt> <head.txt>"
            );
            ExitCode::from(2)
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
