// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Argument parsing, page lookup, width and colour policy for `man`.
//!
//! The target program supplies the filesystem and process side; this module stays `no_std` so
//! the same rules are exercised by host tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_mdoc::Document;
use raven_theme::{Decision, Depth, Palette};

/// The `man` version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The default manual root.
pub const DEFAULT_MAN_PATH: &str = "/usr/share/man";

/// Sections searched when the caller does not name one.
pub const SECTIONS: [u8; 4] = [1, 2, 4, 8];

/// The usage line.
pub const USAGE: &str = "usage: man [-hV] [--color=auto|always|never] [section] name";

/// What the command line asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Look up and render a page.
    Render,
    /// Print help.
    Help,
    /// Print the version.
    Version,
}

/// A validated command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    /// The selected action.
    pub action: Action,
    /// Page name, when rendering.
    pub name: Option<String>,
    /// Explicit section, when rendering.
    pub section: Option<u8>,
    /// Colour policy.
    pub color: Decision,
}

/// The command line was invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl core::fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(USAGE)
    }
}

/// Parse the arguments after `argv[0]`.
///
/// # Errors
/// An unknown option, a malformed section, or a missing/extra operand.
pub fn parse(args: &[&str]) -> Result<Invocation, UsageError> {
    let mut color = Decision::Auto;
    let mut operands: Vec<String> = Vec::new();
    let mut options = true;
    let mut index = 0usize;
    while index < args.len() {
        let argument = args[index];
        if options {
            match argument {
                "--" => {
                    options = false;
                    index += 1;
                    continue;
                }
                "-h" | "--help" => {
                    return Ok(Invocation {
                        action: Action::Help,
                        name: None,
                        section: None,
                        color,
                    });
                }
                "-V" | "--version" => {
                    return Ok(Invocation {
                        action: Action::Version,
                        name: None,
                        section: None,
                        color,
                    });
                }
                "--color" => {
                    index += 1;
                    let value = args.get(index).ok_or(UsageError)?;
                    color = Decision::parse(value).ok_or(UsageError)?;
                }
                value if value.starts_with("--color=") => {
                    color = Decision::parse(&value["--color=".len()..]).ok_or(UsageError)?;
                }
                value if value.starts_with('-') && value.len() > 1 => return Err(UsageError),
                value => operands.push(value.to_string()),
            }
        } else {
            operands.push(argument.to_string());
        }
        index += 1;
    }
    let (section, name) = match operands.as_slice() {
        [name] => (None, name.clone()),
        [section, name] => (
            Some(parse_section(section).ok_or(UsageError)?),
            name.clone(),
        ),
        _ => return Err(UsageError),
    };
    if name.is_empty() || name.contains('/') || name == "." || name == ".." {
        return Err(UsageError);
    }
    Ok(Invocation {
        action: Action::Render,
        name: Some(name),
        section,
        color,
    })
}

/// Parse a manual section 1..=9.
#[must_use]
pub fn parse_section(value: &str) -> Option<u8> {
    value
        .parse::<u8>()
        .ok()
        .filter(|section| (1..=9).contains(section))
}

/// Build the search roots: `/usr/share/man`, then `/etc/man.conf`, then `MANPATH`.
#[must_use]
pub fn roots(config: &str, manpath: Option<&str>) -> Vec<String> {
    let mut roots = vec![String::from(DEFAULT_MAN_PATH)];
    for line in config.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if !line.is_empty() {
            roots.push(line.to_string());
        }
    }
    if let Some(manpath) = manpath {
        for part in manpath.split(':') {
            if !part.is_empty() {
                roots.push(part.to_string());
            }
        }
    }
    let mut unique = Vec::new();
    for root in roots {
        if !unique.contains(&root) {
            unique.push(root);
        }
    }
    unique
}

/// Candidate paths in search order: section, then root.
#[must_use]
pub fn candidates(roots: &[String], name: &str, section: Option<u8>) -> Vec<(u8, String)> {
    let sections: Vec<u8> = match section {
        Some(section) => vec![section],
        None => SECTIONS.to_vec(),
    };
    let mut paths = Vec::new();
    for section in sections {
        for root in roots {
            let root = root.trim_end_matches('/');
            paths.push((section, format!("{root}/man{section}/{name}.{section}")));
        }
    }
    paths
}

/// Find the first candidate for which `exists` is true.
#[must_use]
pub fn find_page(
    roots: &[String],
    name: &str,
    section: Option<u8>,
    exists: impl Fn(&str) -> bool,
) -> Option<(u8, String)> {
    candidates(roots, name, section)
        .into_iter()
        .find(|(_, path)| exists(path))
}

/// Resolve the wrapping width from `MANWIDTH`, then `COLUMNS`, then 80.
#[must_use]
pub fn width(manwidth: Option<&str>, columns: Option<&str>) -> usize {
    manwidth
        .and_then(parse_width)
        .or_else(|| columns.and_then(parse_width))
        .unwrap_or(80)
}

fn parse_width(value: &str) -> Option<usize> {
    value
        .trim()
        .parse::<usize>()
        .ok()
        .filter(|width| (20..=300).contains(width))
}

/// Split `MANPAGER`/`PAGER` into a command and its arguments.
#[must_use]
pub fn pager(value: Option<&str>) -> Option<Vec<String>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let argv: Vec<String> = value.split_whitespace().map(ToString::to_string).collect();
    if argv.is_empty() { None } else { Some(argv) }
}

/// Resolve the pager program through `/bin` and `/sbin` when it is a bare name.
#[must_use]
pub fn resolve_pager(argv: &[String], exists: impl Fn(&str) -> bool) -> Option<String> {
    let program = argv.first()?;
    if program.contains('/') {
        return exists(program).then(|| program.clone());
    }
    for directory in ["/bin", "/sbin"] {
        let path = format!("{directory}/{program}");
        if exists(&path) {
            return Some(path);
        }
    }
    None
}

/// Apply the colour decision for a human writer.
#[must_use]
pub fn color_enabled(policy: Decision, is_tty: bool, no_color: bool, term_dumb: bool) -> bool {
    policy.enabled(is_tty, no_color, term_dumb, false)
}

/// Detect the terminal colour depth.
#[must_use]
pub fn depth(colorterm: Option<&str>, term: Option<&str>) -> Depth {
    Depth::detect(colorterm, term)
}

/// Render one page, optionally with ANSI escapes.
///
/// # Errors
/// A page parse error.
pub fn render_page(
    source: &str,
    width: usize,
    enabled: bool,
    colorterm: Option<&str>,
    term: Option<&str>,
) -> Result<String, String> {
    let document = Document::parse(source).map_err(|error| error.to_string())?;
    let palette = Palette::raven_violet();
    let depth = depth(colorterm, term);
    let mut out = String::new();
    for line in document.render(width) {
        out.push_str(&line.painted(&palette, depth, enabled));
        out.push('\n');
    }
    Ok(out)
}

/// The version line.
#[must_use]
pub fn version_line() -> String {
    format!("man {VERSION} (RavenBSD)")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
