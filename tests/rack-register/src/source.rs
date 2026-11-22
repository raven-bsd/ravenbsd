// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Product-source index for proposing and checking register rows.

use std::collections::BTreeSet;
use std::path::Path;

use rack_acceptance::register::repo_root;

/// Functions and `#[test]` names found under `crates/raven-tcpip`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceIndex {
    functions: BTreeSet<String>,
    tests: BTreeSet<String>,
}

impl SourceIndex {
    /// Load every `.rs` file under `crates/raven-tcpip/src` and `crates/raven-tcpip/tests`.
    pub fn load() -> Result<Self, String> {
        let root = repo_root();
        let mut index = Self::default();
        for relative in ["crates/raven-tcpip/src", "crates/raven-tcpip/tests"] {
            walk(&root.join(relative), &mut index)?;
        }
        if index.functions.is_empty() || index.tests.is_empty() {
            return Err("source index is empty".to_owned());
        }
        Ok(index)
    }

    /// Whether an exact function name exists.
    #[must_use]
    pub fn has_function(&self, name: &str) -> bool {
        self.functions.contains(name)
    }

    /// Whether a register Rust cell names an existing product function.
    #[must_use]
    pub fn symbol_exists(&self, cell: &str) -> bool {
        cell.split([',', ';', '+'])
            .filter_map(symbol_name)
            .any(|name| self.has_function(&name))
    }

    /// First function name matching the donor name or its stripped form.
    #[must_use]
    pub fn mapping_name(&self, donor: &str) -> Option<String> {
        candidates(donor)
            .into_iter()
            .find(|name| self.functions.contains(name))
    }

    /// First test whose name starts with the donor function or its stripped form.
    #[must_use]
    pub fn test_for(&self, donor: &str) -> Option<String> {
        let candidates = candidates(donor);
        let matches = |test: &str| {
            candidates
                .iter()
                .any(|name| test == name || test.starts_with(&format!("{name}_")))
        };
        // A test for `rack_free_trim` also starts with `rack_free_`; do not let it answer for
        // `rack_free`. Skip a candidate when the next token names a longer donor function.
        let longer_donor = |test: &str, name: &str| {
            let Some(rest) = test.strip_prefix(&format!("{name}_")) else {
                return false;
            };
            let Some(first) = rest.split('_').next() else {
                return false;
            };
            self.functions.contains(&format!("{name}_{first}"))
        };
        self.tests
            .iter()
            .filter(|test| matches(test))
            .filter(|test| !candidates.iter().any(|name| longer_donor(test, name)))
            .min_by_key(|test| test.len())
            .cloned()
            .or_else(|| {
                self.tests
                    .iter()
                    .filter(|test| matches(test))
                    .min_by_key(|test| test.len())
                    .cloned()
            })
    }

    /// Whether a test named by a register cell exists.
    #[must_use]
    pub fn has_test_named(&self, name: &str) -> bool {
        if self.tests.contains(name) {
            return true;
        }
        let name = name.strip_prefix("donor_").unwrap_or(name);
        self.tests.contains(name) || self.test_for(name).is_some()
    }

    /// Number of parsed functions.
    #[must_use]
    pub fn function_count(&self) -> usize {
        self.functions.len()
    }

    /// Number of parsed tests.
    #[must_use]
    pub fn test_count(&self) -> usize {
        self.tests.len()
    }
}

fn symbol_name(cell: &str) -> Option<String> {
    let cell = cell.trim().trim_matches('`');
    let cell = cell.split('(').next().unwrap_or(cell);
    let name = cell
        .split([':', '.', '/', ' ', '\t'])
        .rfind(|part| !part.is_empty())?;
    if name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '_')
    {
        Some(name.to_owned())
    } else {
        None
    }
}

fn candidates(donor: &str) -> Vec<String> {
    let mut values = vec![donor.to_owned()];
    for prefix in ["rack_", "tcp_", "rc_"] {
        if let Some(stripped) = donor.strip_prefix(prefix) {
            let stripped = stripped.to_owned();
            if !values.contains(&stripped) {
                values.push(stripped);
            }
        }
    }
    values
}

fn walk(directory: &Path, index: &mut SourceIndex) -> Result<(), String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", directory.display()))?;
        let path = entry.path();
        if path.is_dir() {
            walk(&path, index)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let text = std::fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;
            collect(&text, index);
        }
    }
    Ok(())
}

fn collect(text: &str, index: &mut SourceIndex) {
    collect_macro_tests(text, &mut index.tests);
    let mut pending_test = false;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#[test]") {
            pending_test = true;
            continue;
        }
        if trimmed.starts_with("#[") || trimmed.starts_with("//") || trimmed.is_empty() {
            continue;
        }
        if let Some(name) = function_name(trimmed) {
            index.functions.insert(name.clone());
            if pending_test {
                index.tests.insert(name);
                pending_test = false;
            }
        } else {
            pending_test = false;
        }
    }
}

fn collect_macro_tests(text: &str, tests: &mut BTreeSet<String>) {
    let bytes = text.as_bytes();
    let mut index = 0usize;
    while let Some(offset) = text[index..].find("!(") {
        let mut cursor = index + offset + 2;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric() || bytes[cursor] == b'_')
        {
            cursor += 1;
        }
        let name = &text[start..cursor];
        if name.starts_with("donor_") && name != "donor_bool_test" {
            tests.insert(name.to_owned());
        }
        index = cursor.max(start + 1);
    }
}

fn function_name(line: &str) -> Option<String> {
    let mut rest = line;
    for prefix in [
        "pub(crate) ",
        "pub(super) ",
        "pub ",
        "async ",
        "const ",
        "unsafe ",
    ] {
        rest = rest.strip_prefix(prefix).unwrap_or(rest);
    }
    let rest = rest.strip_prefix("fn ")?;
    let name = rest
        .split(['(', '<', ' ', ':'])
        .next()
        .unwrap_or_default()
        .trim();
    if name.is_empty() {
        None
    } else {
        Some(name.to_owned())
    }
}
