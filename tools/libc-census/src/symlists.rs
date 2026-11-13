// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Parsers for the five donor symbol-list grammars.
//!
//! The parsers keep names and structural facts only. In particular the illumos
//! mapfile is parsed with the grammar of ticket 4.4; no line of it is printed,
//! quoted or stored.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use anyhow::{Context, Result};
use regex::Regex;

use crate::model::Kind;

/// Names and metadata parsed from one donor list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedSymbols {
    /// Sorted names.
    pub names: BTreeSet<String>,
    /// Names in a `FBSDprivate_*` node (FreeBSD).
    pub private: BTreeSet<String>,
    /// Symbol -> version node (FreeBSD).
    pub versions: BTreeMap<String, String>,
    /// Symbol -> comment notes (bionic).
    pub notes: BTreeMap<String, String>,
    /// Identifier lines seen, including duplicates.
    pub identifier_lines: usize,
}

/// One manual entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManualEntry {
    /// Mapped standard tags.
    pub standard: BTreeSet<String>,
    /// Header from `@standards`.
    pub header: Option<String>,
    /// Function, data or macro.
    pub kind: Kind,
}

/// Everything mined from the glibc manual.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ManualOutput {
    /// Entries keyed by name.
    pub entries: BTreeMap<String, ManualEntry>,
    /// Number of `@deftype*` definition lines.
    pub deftype_lines: usize,
    /// Number of `@standards`/`@standardsx` lines.
    pub standards_lines: usize,
    /// One-field `@standards` lines that were skipped.
    pub one_field_standards: usize,
}

static IDENT_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*").expect("identifier regex"));

static ILLUMOS_KEYWORDS: [&str; 8] = [
    "SYMBOL_VERSION",
    "SYMBOL_SCOPE",
    "ASSERT",
    "SIZE",
    "TYPE",
    "FLAGS",
    "ALIAS",
    "FILTER",
];

/// Parse an OpenBSD `Symbols.list`.
pub fn parse_openbsd(text: &str) -> ParsedSymbols {
    let stripped = strip_block_comments(text);
    let mut parsed = ParsedSymbols::default();
    for line in stripped.lines() {
        let Some(token) = line.split_whitespace().next() else {
            continue;
        };
        if token.starts_with('#') || !is_identifier(token) {
            continue;
        }
        parsed.identifier_lines += 1;
        parsed.names.insert(token.to_string());
    }
    parsed
}

/// Parse one FreeBSD `Symbol.map`.
pub fn parse_freebsd(text: &str) -> ParsedSymbols {
    let stripped = strip_hash_comments(&strip_block_comments(text));
    let mut parsed = ParsedSymbols::default();
    let mut block: Option<(String, bool)> = None;
    for line in stripped.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if block.is_none() {
            if let Some((name, rest)) = line.split_once('{') {
                let name = name.trim();
                let private = name.starts_with("FBSDprivate");
                block = Some((name.to_string(), private));
                collect_freebsd_body(&mut parsed, block.as_ref(), rest.trim());
            }
            continue;
        }
        if line.starts_with('}') {
            block = None;
            continue;
        }
        collect_freebsd_body(&mut parsed, block.as_ref(), line);
    }
    parsed
}

fn collect_freebsd_body(parsed: &mut ParsedSymbols, block: Option<&(String, bool)>, body: &str) {
    let Some((version, private)) = block else {
        return;
    };
    for part in body.split(';') {
        let body = part.trim();
        if body.is_empty() {
            continue;
        }
        let body = body
            .trim_start_matches("local:")
            .trim_start_matches("global:")
            .trim();
        if body.is_empty() || body == "*" {
            continue;
        }
        let candidate = body.split_whitespace().next().unwrap_or_default();
        if candidate.contains('{') || candidate.contains('}') || candidate.contains('*') {
            continue;
        }
        let name = candidate;
        if !is_identifier(name) || ILLUMOS_KEYWORDS.contains(&name) {
            continue;
        }
        parsed.identifier_lines += 1;
        parsed.names.insert(name.to_string());
        parsed.versions.insert(name.to_string(), version.clone());
        if *private {
            parsed.private.insert(name.to_string());
        }
    }
}

/// Parse bionic's `libc.map.txt`.
pub fn parse_bionic(text: &str) -> ParsedSymbols {
    let stripped = strip_block_comments(text);
    let mut parsed = ParsedSymbols::default();
    let mut in_block = false;
    let mut global = false;
    for line in stripped.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if !in_block {
            if line.contains('{') {
                in_block = true;
                if line.contains("global:") {
                    global = true;
                }
                if line.contains("local:") {
                    global = false;
                }
                let body = line.split_once('{').map(|(_, rest)| rest).unwrap_or("");
                if !body.trim().is_empty() {
                    collect_bionic_body(&mut parsed, global, body);
                }
            }
            continue;
        }
        if line.starts_with('}') {
            in_block = false;
            global = false;
            continue;
        }
        if let Some(stripped) = line.strip_prefix("global:") {
            global = true;
            let rest = stripped.trim();
            if !rest.is_empty() {
                collect_bionic_body(&mut parsed, global, rest);
            }
            continue;
        }
        if line.starts_with("local:") {
            global = false;
            continue;
        }
        if !global {
            continue;
        }
        collect_bionic_body(&mut parsed, global, line);
    }
    parsed
}

fn collect_bionic_body(parsed: &mut ParsedSymbols, global: bool, body: &str) {
    let (symbol, comment) = match body.split_once('#') {
        Some((left, right)) => (left.trim(), Some(right.trim())),
        None => (body.trim(), None),
    };
    let symbol = symbol.trim_end_matches(';').trim();
    if !is_identifier(symbol) {
        return;
    }
    if comment.is_some_and(|comment| comment.contains("local:")) {
        return;
    }
    let effective_global = global && !symbol.is_empty();
    if !effective_global {
        return;
    }
    parsed.identifier_lines += 1;
    parsed.names.insert(symbol.to_string());
    if let Some(comment) = comment {
        let words: Vec<&str> = comment
            .split_whitespace()
            .filter(|word| matches!(*word, "arm" | "x86") || word.starts_with("introduced="))
            .collect();
        if !words.is_empty() {
            parsed.notes.insert(symbol.to_string(), words.join(" "));
        }
    }
}

/// Parse an illumos `mapfile-vers` using only the ticket 4.4 grammar.
pub fn parse_illumos(text: &str) -> ParsedSymbols {
    let mut parsed = ParsedSymbols::default();
    let mut in_block = false;
    let mut assertion_depth = 0usize;
    for line in text.lines() {
        let line = line.trim();
        if assertion_depth > 0 {
            assertion_depth = adjust_depth(assertion_depth, line);
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with('$') {
            continue;
        }
        if !in_block {
            if line.starts_with("SYMBOL_VERSION") && line.contains('{') {
                in_block = true;
            }
            continue;
        }
        if line.starts_with('}') {
            in_block = false;
            continue;
        }
        if matches!(line, "protected:" | "global:" | "local:" | "exported:") {
            continue;
        }
        let Some(token) = IDENT_RE.find(line) else {
            continue;
        };
        if token.start() != 0 {
            continue;
        }
        let name = token.as_str();
        if ILLUMOS_KEYWORDS.contains(&name) {
            if name == "ASSERT" || line.contains('{') {
                assertion_depth = adjust_depth(0, &line[token.end()..]);
            }
            continue;
        }
        let rest = line[token.end()..].trim_start();
        if rest.starts_with(';') || rest.starts_with('=') {
            parsed.identifier_lines += 1;
            parsed.names.insert(name.to_string());
        } else if rest.starts_with('{') {
            parsed.identifier_lines += 1;
            parsed.names.insert(name.to_string());
            assertion_depth = adjust_depth(0, rest);
        }
    }
    parsed
}

fn adjust_depth(mut depth: usize, text: &str) -> usize {
    for character in text.chars() {
        match character {
            '{' => depth = depth.saturating_add(1),
            '}' => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    depth
}

/// Parse one glibc manual `.texi` file.
pub fn parse_manual(text: &str) -> ManualOutput {
    let mut output = ManualOutput::default();
    let mut pending: Option<(String, Kind, bool)> = None;

    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Some((name, kind)) = definition_line(trimmed) {
            finalize_pending(&mut output, &mut pending);
            output.deftype_lines += 1;
            pending = Some((name, kind, false));
            continue;
        }
        if trimmed.starts_with("@standardsx{") {
            output.standards_lines += 1;
            let Some(fields) = braced_fields(trimmed, "@standardsx") else {
                continue;
            };
            if fields.len() < 3 {
                output.one_field_standards += 1;
                continue;
            }
            let name = fields[0].clone();
            if !is_identifier(&name) {
                continue;
            }
            let tag = crate::standards::map_tag(&fields[1]);
            let header = fields[2].clone();
            let kind = match &pending {
                Some((pending_name, kind, _)) if pending_name == &name => *kind,
                _ => Kind::Func,
            };
            let entry = output.entries.entry(name.clone()).or_insert(ManualEntry {
                kind,
                ..ManualEntry::default()
            });
            if !tag.is_empty() {
                entry.standard.insert(tag);
            }
            if !header.is_empty() {
                entry.header = Some(header);
            }
            if let Some((pending_name, _, closed)) = pending.as_mut()
                && pending_name == &name
            {
                *closed = true;
            }
            continue;
        }
        if trimmed.starts_with("@standards{") {
            output.standards_lines += 1;
            let Some(fields) = braced_fields(trimmed, "@standards") else {
                continue;
            };
            if fields.len() < 2 {
                output.one_field_standards += 1;
                continue;
            }
            if let Some((name, _, closed)) = pending.as_mut()
                && !*closed
            {
                let tag = crate::standards::map_tag(&fields[0]);
                let header = fields[1].clone();
                let entry = output.entries.entry(name.clone()).or_insert(ManualEntry {
                    kind: Kind::Func,
                    ..ManualEntry::default()
                });
                if !tag.is_empty() {
                    entry.standard.insert(tag);
                }
                if !header.is_empty() {
                    entry.header = Some(header);
                }
                *closed = true;
            }
            continue;
        }
        finalize_pending(&mut output, &mut pending);
    }
    finalize_pending(&mut output, &mut pending);
    output
}

fn finalize_pending(output: &mut ManualOutput, pending: &mut Option<(String, Kind, bool)>) {
    if let Some((name, kind, _)) = pending.take() {
        let entry = output.entries.entry(name).or_insert(ManualEntry {
            kind,
            ..ManualEntry::default()
        });
        if entry.kind != kind && kind != Kind::Func {
            entry.kind = kind;
        }
    }
}

fn definition_line(line: &str) -> Option<(String, Kind)> {
    let (command, rest) = line.split_once(char::is_whitespace)?;
    let rest = rest.trim();
    match command {
        "@deftypefun" | "@deftypefunx" | "@deftypefn" | "@deftypefnx" => {
            let before_paren = rest.split_once('(').map(|(head, _)| head).unwrap_or(rest);
            let name = IDENT_RE
                .find_iter(before_paren)
                .last()
                .map(|mat| mat.as_str().to_string())?;
            if is_identifier(&name) {
                Some((name, Kind::Func))
            } else {
                None
            }
        }
        "@deftypevr" | "@deftypevrx" => {
            let name = IDENT_RE
                .find_iter(rest)
                .last()
                .map(|mat| mat.as_str().to_string())?;
            if !is_identifier(&name) {
                return None;
            }
            let category = category_of(rest).unwrap_or_default();
            let kind = if category == "Macro" {
                Kind::Macro
            } else {
                Kind::Data
            };
            Some((name, kind))
        }
        "@deftypevar" | "@deftypevarx" => {
            let name = IDENT_RE
                .find_iter(rest)
                .last()
                .map(|mat| mat.as_str().to_string())?;
            if is_identifier(&name) {
                Some((name, Kind::Data))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn first_braced(text: &str) -> Option<String> {
    let start = text.find('{')?;
    let end = text[start + 1..].find('}')? + start + 1;
    Some(text[start + 1..end].trim().to_string())
}

fn category_of(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    if trimmed.starts_with('{') {
        first_braced(trimmed)
    } else {
        trimmed
            .split_whitespace()
            .next()
            .map(|word| word.to_string())
    }
}

fn braced_fields(line: &str, command: &str) -> Option<Vec<String>> {
    let rest = line.strip_prefix(command)?;
    let start = rest.find('{')?;
    let end = rest.rfind('}')?;
    if end <= start {
        return None;
    }
    Some(
        rest[start + 1..end]
            .split(',')
            .map(|field| field.trim().to_string())
            .collect(),
    )
}

fn strip_block_comments(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut characters = text.chars().peekable();
    let mut in_comment = false;
    while let Some(character) = characters.next() {
        if !in_comment {
            if character == '/' && characters.peek() == Some(&'*') {
                characters.next();
                in_comment = true;
            } else {
                output.push(character);
            }
        } else if character == '*' && characters.peek() == Some(&'/') {
            characters.next();
            in_comment = false;
        } else if character == '\n' {
            output.push('\n');
        } else {
            output.push(' ');
        }
    }
    output
}

fn strip_hash_comments(text: &str) -> String {
    text.lines()
        .map(|line| match line.find('#') {
            Some(index) => &line[..index],
            None => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn is_identifier(text: &str) -> bool {
    crate::universe::is_identifier(text)
}

/// Read and parse a file.
pub fn parse_file(path: &Path, parser: fn(&str) -> ParsedSymbols) -> Result<ParsedSymbols> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(parser(&text))
}

#[cfg(test)]
#[path = "symlists_ut.rs"]
mod tests;
