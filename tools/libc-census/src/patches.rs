// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Distribution patch classifier (ticket C33 section 4.6).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

/// One classifier category.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Category {
    /// A changed include line names a libc header.
    LibcHeader,
    /// A changed line tests a libc identity macro.
    LibcMacro,
    /// A changed line calls a non-standard universe symbol.
    LibcSymbol,
    /// None of the above, but the patch talks about a libc.
    LibcSemantics,
}

impl Category {
    /// Spelling used in `ports.json`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LibcHeader => "libc-header",
            Self::LibcMacro => "libc-macro",
            Self::LibcSymbol => "libc-symbol",
            Self::LibcSemantics => "libc-semantics",
        }
    }
}

/// What one patch fired.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClassifiedPatch {
    /// Categories, sorted.
    pub categories: BTreeSet<Category>,
    /// Headers named on changed include lines.
    pub headers: BTreeSet<String>,
    /// Libc identity macros.
    pub macros: BTreeSet<String>,
    /// Non-standard universe names called on changed lines.
    pub symbols: BTreeSet<String>,
}

static HEADER_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[+-]\s*#\s*include\s*<(sys/[\w./+-]+|linux/[\w./+-]+|machine/[\w./+-]+|bits/[\w./+-]+|bsd/[\w./+-]+|features\.h|endian\.h|byteswap\.h|alloca\.h|malloc\.h|stropts\.h|execinfo\.h|error\.h|err\.h)>",
    )
    .expect("header regex")
});

static MACRO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"\b(__GLIBC__|__GLIBC_MINOR__|__USE_[\x41-\x5a0-9_]+|_GNU_SOURCE|_BSD_SOURCE|_DEFAULT_SOURCE|__MUSL__|__UCLIBC__|__linux__|__FreeBSD__|__OpenBSD__|__NetBSD__)\b",
    )
    .expect("macro regex")
});

static CALL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*[ \t]*\(").expect("call regex")
});

static NAME_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x41-\x5aa-z_][\x41-\x5aa-z0-9_]*").expect("name regex"));

static SEMANTICS_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(musl|glibc|uclibc|libc|portab)\w*").expect("semantics regex")
});

/// Classify one patch.
///
/// `symbol_rings` maps a universe name to its computed ring; ring-0 names are
/// deliberately excluded from `libc-symbol` (a conforming libc cannot lack them
/// and `index`/`error`-style words are noise, A-bsd-4).
pub fn classify_patch(
    path: &Path,
    text: &str,
    symbol_rings: &BTreeMap<String, u8>,
) -> ClassifiedPatch {
    let mut classified = ClassifiedPatch::default();
    for line in text.lines() {
        if !line.starts_with('+') && !line.starts_with('-') {
            continue;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        if let Some(capture) = HEADER_RE.captures(line)
            && let Some(header) = capture.get(1)
        {
            classified.headers.insert(header.as_str().to_string());
            classified.categories.insert(Category::LibcHeader);
        }
        for capture in MACRO_RE.captures_iter(line) {
            if let Some(name) = capture.get(1) {
                classified.macros.insert(name.as_str().to_string());
                classified.categories.insert(Category::LibcMacro);
            }
        }
        for call in CALL_RE.find_iter(line) {
            let Some(name) = NAME_RE.find(call.as_str()) else {
                continue;
            };
            let name = name.as_str();
            if symbol_rings.get(name).is_some_and(|ring| *ring != 0) {
                classified.symbols.insert(name.to_string());
                classified.categories.insert(Category::LibcSymbol);
            }
        }
    }
    if classified.categories.is_empty() {
        let before_hunk = text.split_once("@@").map_or(text, |(head, _)| head);
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if SEMANTICS_RE.is_match(file_name) || SEMANTICS_RE.is_match(before_hunk) {
            classified.categories.insert(Category::LibcSemantics);
        }
    }
    classified
}

#[cfg(test)]
#[path = "patches_ut.rs"]
mod tests;
