// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Member matching, wildcard patterns and the `R-30` member table.
//!
//! A size-rule pattern is matched against the object name inside the archive
//! (`libc.a(strlen.o)` -> `strlen.o`). The current shell names a member after its
//! codegen unit (`raven_libc-<hash>.raven_libc.<hash>-cgu.NNN.rcgu.o`), so the matcher
//! also tries the identifier components of the name and their `raven_libc_`-stripped
//! forms: `regex*` matches `raven_libc_regex-...`.

use crate::measure::MemberSize;

/// Whether `pattern` (with `*` wildcards) matches `text` anywhere.
#[must_use]
pub fn wildcard_search(pattern: &str, text: &str) -> bool {
    if pattern.is_empty() {
        return true;
    }
    // A rule pattern is a *search* pattern: it may match anywhere in the name (Rust
    // legacy-mangled names carry `core3fmt` in the middle).
    for start in 0..=text.len() {
        if !text.is_char_boundary(start) {
            continue;
        }
        if glob_at(pattern.as_bytes(), text.as_bytes(), start) {
            return true;
        }
    }
    false
}

fn glob_at(pattern: &[u8], text: &[u8], start: usize) -> bool {
    let mut pattern_index = 0usize;
    let mut text_index = start;
    let mut star: Option<usize> = None;
    let mut resume = start;
    while text_index < text.len() {
        if pattern_index >= pattern.len() {
            return true;
        }
        if pattern_index < pattern.len() && pattern[pattern_index] != b'*' {
            if pattern[pattern_index] == text[text_index] {
                pattern_index += 1;
                text_index += 1;
                continue;
            }
        } else if pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
            star = Some(pattern_index);
            pattern_index += 1;
            resume = text_index;
            continue;
        }
        if let Some(star_index) = star {
            pattern_index = star_index + 1;
            resume += 1;
            text_index = resume;
        } else {
            return false;
        }
    }
    while pattern_index < pattern.len() && pattern[pattern_index] == b'*' {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}

/// All candidate names of one archive member.
#[must_use]
pub fn member_candidates(member: &str) -> Vec<String> {
    let mut candidates = vec![member.to_string()];
    let stem = member.rsplit('/').next().unwrap_or(member);
    candidates.push(stem.to_string());
    let mut current = String::new();
    for ch in stem.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
        } else if !current.is_empty() {
            candidates.push(current.clone());
            if let Some(stripped) = current.strip_prefix("raven_libc_") {
                candidates.push(stripped.to_string());
            }
            if let Some(stripped) = current.strip_prefix("raven_") {
                candidates.push(stripped.to_string());
            }
            current.clear();
        }
    }
    if !current.is_empty() {
        candidates.push(current.clone());
        if let Some(stripped) = current.strip_prefix("raven_libc_") {
            candidates.push(stripped.to_string());
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Whether one member matches any of the patterns.
#[must_use]
pub fn member_matches(member: &str, patterns: &[String]) -> bool {
    let candidates = member_candidates(member);
    patterns.iter().any(|pattern| {
        candidates
            .iter()
            .any(|candidate| wildcard_search(pattern, candidate))
    })
}

/// The members that match one of the patterns, sorted.
#[must_use]
pub fn matching_members(members: &[String], patterns: &[String]) -> Vec<String> {
    let mut out: Vec<String> = members
        .iter()
        .filter(|member| member_matches(member, patterns))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Whether one symbol name matches any pattern (searched, not only prefixed).
#[must_use]
pub fn symbol_matches(symbol: &str, patterns: &[String]) -> bool {
    patterns
        .iter()
        .any(|pattern| wildcard_search(pattern, symbol))
}

/// Members whose `.text` exceeds `limit` (`R-30`), largest first.
#[must_use]
pub fn large_members(members: &[MemberSize], limit: u64) -> Vec<MemberSize> {
    let mut out: Vec<MemberSize> = members
        .iter()
        .filter(|member| member.size.text > limit)
        .cloned()
        .collect();
    out.sort_by(|a, b| b.size.text.cmp(&a.size.text).then(a.name.cmp(&b.name)));
    out
}

/// Render the member table rows (`name text`) used by `members` and `run`.
#[must_use]
pub fn render_table(members: &[MemberSize]) -> String {
    let mut out = String::new();
    for member in members {
        out.push_str(&format!(
            "{} {} {} {}\n",
            member.name, member.size.text, member.size.data, member.size.bss
        ));
    }
    out
}

#[cfg(test)]
#[path = "members_ut.rs"]
mod tests;
