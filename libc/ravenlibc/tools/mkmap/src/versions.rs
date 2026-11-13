// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Parsers for `abi/Versions.def` and the per-crate `Symbol.map` files.
//!
//! The grammar is the functional subset of a linker version script that the pipeline uses:
//! nodes with an optional parent, one `global:` block of `name;` lines and an optional
//! `local: *;` line in the last node.

use anyhow::{Context, Result, bail};

/// One node of the version chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Node {
    /// Node name (`RAVEN_1.0`, `RAVENprivate_1.0`).
    pub name: String,
    /// The node this one inherits from, if any.
    pub parent: Option<String>,
}

/// One exported name and its architecture restriction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Symbol {
    /// The C symbol name.
    pub name: String,
    /// `None` when every supported architecture exports it, else the explicit list.
    pub arches: Option<Vec<String>>,
}

impl Symbol {
    /// Whether the symbol belongs to the chosen architecture.
    #[must_use]
    pub fn on_arch(&self, arch: &str) -> bool {
        match &self.arches {
            None => true,
            Some(list) => list.iter().any(|candidate| candidate == arch),
        }
    }
}

/// The symbols one `Symbol.map` contributes to one node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeBody {
    /// The node name.
    pub node: String,
    /// The symbols in file order.
    pub symbols: Vec<Symbol>,
}

/// Parse `abi/Versions.def`.
///
/// # Errors
/// A malformed block or a node without a body.
pub fn parse_versions(text: &str) -> Result<Vec<Node>> {
    let mut nodes: Vec<Node> = Vec::new();
    for (number, raw) in text.lines().enumerate() {
        let line = strip_comment(raw).trim();
        if line.is_empty() {
            continue;
        }
        let Some(open) = line.find('{') else {
            bail!("line {}: expected `{{`", number + 1);
        };
        let name = line
            .get(..open)
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .with_context(|| format!("line {}: node without a name", number + 1))?;
        validate_name(name)?;
        if nodes.iter().any(|node| node.name == name) {
            bail!("line {}: duplicate node `{name}`", number + 1);
        }
        let rest = line.get(open + 1..).unwrap_or("").trim();
        let Some(close) = rest.find('}') else {
            bail!("line {}: node `{name}` has no closing brace", number + 1);
        };
        let after = rest.get(close + 1..).unwrap_or("").trim();
        let parent = if after.is_empty() {
            None
        } else {
            let parent = after.trim_end_matches(';').trim();
            if parent.is_empty() {
                None
            } else {
                validate_name(parent)?;
                Some(parent.to_string())
            }
        };
        nodes.push(Node {
            name: name.to_string(),
            parent,
        });
    }
    if nodes.is_empty() {
        bail!("Versions.def has no node");
    }
    for node in &nodes {
        if let Some(parent) = &node.parent
            && !nodes.iter().any(|candidate| &candidate.name == parent)
        {
            bail!("node `{}` inherits from unknown `{parent}`", node.name);
        }
    }
    Ok(nodes)
}

/// Parse one `Symbol.map`.
///
/// # Errors
/// A malformed block, an unknown directive or a symbol without `;`.
pub fn parse_symbol_map(text: &str, source: &str) -> Result<Vec<NodeBody>> {
    let mut bodies: Vec<NodeBody> = Vec::new();
    let mut current: Option<usize> = None;
    for (number, raw) in text.lines().enumerate() {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (code, arch_comment) = match raw.find('#') {
            Some(hash) => (
                raw.get(..hash).unwrap_or("").trim(),
                Some(raw.get(hash + 1..).unwrap_or("").trim()),
            ),
            None => (trimmed, None),
        };
        if code.is_empty() {
            continue;
        }
        if let Some(open) = code.find('{') {
            let name = code
                .get(..open)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .with_context(|| format!("{source}:{}: node without a name", number + 1))?;
            validate_name(name)?;
            if bodies.iter().any(|body| body.node == name) {
                bail!("{source}:{}: node `{name}` twice in one file", number + 1);
            }
            bodies.push(NodeBody {
                node: name.to_string(),
                symbols: Vec::new(),
            });
            current = Some(bodies.len().saturating_sub(1));
            let rest = code.get(open + 1..).unwrap_or("").trim();
            if rest.starts_with('}') {
                current = None;
            }
            continue;
        }
        if code.starts_with('}') {
            current = None;
            continue;
        }
        if code == "global:" || code == "local:" {
            continue;
        }
        let Some(slot) = current else {
            bail!("{source}:{}: symbol outside a node", number + 1);
        };
        let Some(symbol) = code.strip_suffix(';') else {
            bail!("{source}:{}: expected `name;`", number + 1);
        };
        let symbol = symbol.trim();
        if symbol.is_empty() || symbol == "*" {
            continue;
        }
        let arches = match arch_comment {
            Some(text) if !text.is_empty() => Some(
                text.split_whitespace()
                    .map(str::to_string)
                    .collect::<Vec<_>>(),
            ),
            Some(_) => bail!("{source}:{}: empty architecture comment", number + 1),
            None => None,
        };
        let name = symbol;
        validate_name(name)?;
        if let Some(body) = bodies.get_mut(slot) {
            if body.symbols.iter().any(|existing| existing.name == name) {
                bail!("{source}:{}: duplicate symbol `{name}`", number + 1);
            }
            body.symbols.push(Symbol {
                name: name.to_string(),
                arches,
            });
        }
    }
    Ok(bodies)
}

/// Strip a `#` comment that starts the line or follows whitespace.
#[must_use]
pub fn strip_comment(line: &str) -> &str {
    match line.find('#') {
        Some(index) => line.get(..index).unwrap_or(""),
        None => line,
    }
}

/// Symbol names are C identifiers plus the version-script punctuation `@` and `$`.
fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("empty symbol name");
    }
    for character in name.chars() {
        if !(character.is_ascii_alphanumeric() || matches!(character, '_' | '.' | '$' | '@')) {
            bail!("invalid symbol name `{name}`");
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "versions_ut.rs"]
mod tests;
