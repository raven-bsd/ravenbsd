// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `KNOWN_FAILURES` parsing and lookup.

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};

/// A shrink-only list of accepted failures.
#[derive(Clone, Debug, Default)]
pub struct KnownFailures {
    entries: BTreeMap<String, Option<String>>,
}

impl KnownFailures {
    /// Parse `<suite>:<test name>  # <reason>` lines; blanks and `#` comments are ignored.
    pub fn parse(input: &str) -> Result<Self> {
        let mut entries = BTreeMap::new();
        for (index, line) in input.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (body, reason) = match line.find(" #") {
                Some(position) => (
                    line[..position].trim_end(),
                    Some(line[position + 2..].trim().to_string()),
                ),
                None => (line, None),
            };
            let (suite, name) = body.split_once(':').with_context(|| {
                format!(
                    "line {}: expected `<suite>:<test name>`, found `{body}`",
                    index + 1
                )
            })?;
            let suite = suite.trim();
            let name = name.trim();
            if suite.is_empty() || name.is_empty() {
                bail!(
                    "line {}: suite and test name must both be non-empty: `{body}`",
                    index + 1
                );
            }
            entries.entry(format!("{suite}:{name}")).or_insert(reason);
        }
        Ok(Self { entries })
    }

    /// Whether `suite:name` is listed.
    pub fn contains(&self, suite: &str, name: &str) -> bool {
        self.entries.contains_key(&format!("{suite}:{name}"))
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_comments_blanks_and_reasons() {
        let known = KnownFailures::parse(
            "# header\n\nsuite:test  # reason\nother:name#2   # duplicate suffix\n",
        )
        .unwrap();
        assert_eq!(known.len(), 2);
        assert!(known.contains("suite", "test"));
        assert!(known.contains("other", "name#2"));
    }

    #[test]
    fn rejects_malformed_entries() {
        assert!(KnownFailures::parse("no-colon\n").is_err());
        assert!(KnownFailures::parse("suite:\n").is_err());
        assert!(KnownFailures::parse(":name\n").is_err());
    }
}
