// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The shared `strftime`/`strptime` format-directive scanner.
//!
//! Both directions walk the same `%`-directive grammar: optional padding/case flags, the `E`/`O`
//! alternative forms, then one conversion byte. The scanner yields literals and directives; each
//! direction implements only the conversion dispatch.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: bounded scanner/index arithmetic.

use crate::locale::TimeLocale;

/// The full or abbreviated weekday name for `index`.
#[must_use]
pub(crate) fn day_name(locale: &dyn TimeLocale, index: i32, abbreviated: bool) -> &str {
    let Some(index) = usize::try_from(index).ok().filter(|value| *value < 7) else {
        return "?";
    };
    if abbreviated {
        locale.abday(index)
    } else {
        locale.day(index)
    }
}

/// The full or abbreviated month name for `index`.
#[must_use]
pub(crate) fn month_name(locale: &dyn TimeLocale, index: i32, abbreviated: bool) -> &str {
    let Some(index) = usize::try_from(index).ok().filter(|value| *value < 12) else {
        return "?";
    };
    if abbreviated {
        locale.abmon(index)
    } else {
        locale.mon(index)
    }
}

fn prefix(buf: &[u8], name: &str) -> Option<usize> {
    let bytes = name.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let candidate = buf.get(..bytes.len())?;
    candidate.eq_ignore_ascii_case(bytes).then_some(bytes.len())
}

/// Find the full or abbreviated weekday name at the start of `buf`.
#[must_use]
pub(crate) fn find_day(buf: &[u8], locale: &dyn TimeLocale) -> Option<(usize, usize)> {
    for index in 0..7usize {
        if let Some(length) = prefix(buf, locale.day(index)) {
            return Some((index, length));
        }
    }
    for index in 0..7usize {
        if let Some(length) = prefix(buf, locale.abday(index)) {
            return Some((index, length));
        }
    }
    None
}

/// Find the full or abbreviated month name at the start of `buf`.
#[must_use]
pub(crate) fn find_month(buf: &[u8], locale: &dyn TimeLocale) -> Option<(usize, usize)> {
    for index in 0..12usize {
        if let Some(length) = prefix(buf, locale.mon(index)) {
            return Some((index, length));
        }
    }
    for index in 0..12usize {
        if let Some(length) = prefix(buf, locale.abmon(index)) {
            return Some((index, length));
        }
    }
    None
}

/// The format string of a composite conversion (`%c`, `%D`, ...), if any.
#[must_use]
pub(crate) fn composite(conversion: u8, locale: &dyn TimeLocale) -> Option<&[u8]> {
    match conversion {
        b'c' => Some(locale.d_t_fmt().as_bytes()),
        b'D' => Some(b"%m/%d/%y"),
        b'F' => Some(b"%Y-%m-%d"),
        b'R' => Some(b"%H:%M"),
        b'r' => Some(locale.t_fmt_ampm().as_bytes()),
        b'T' => Some(b"%H:%M:%S"),
        b'v' => Some(b"%e-%b-%Y"),
        b'X' => Some(locale.t_fmt().as_bytes()),
        b'x' => Some(locale.d_fmt().as_bytes()),
        b'+' => Some(b"%a %b %e %H:%M:%S %Z %Y"),
        _ => None,
    }
}

/// One `%`-directive padding choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pad {
    /// No explicit flag.
    Default,
    /// `-`: no padding.
    Less,
    /// `_`: space padding.
    Space,
    /// `0`: zero padding.
    Zero,
}

/// One parsed `%` directive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Directive {
    /// Conversion byte after the flags.
    pub conversion: u8,
    /// Padding flag.
    pub pad: Pad,
    /// `^`: upper-case the output (`strftime` extension).
    pub upper: bool,
    /// `#`: swap the case (`strftime` extension).
    pub swap: bool,
    /// Number of `E` modifiers seen (`strptime` accepts at most one).
    pub alt_e: u8,
    /// Number of `O` modifiers seen (`strptime` accepts at most one).
    pub alt_o: u8,
}

/// One item of a format string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Item {
    /// A literal byte (including whitespace).
    Literal(u8),
    /// A complete `%` directive.
    Directive(Directive),
    /// A trailing `%` with no conversion.
    TrailingPercent,
}

/// The format scanner.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FormatIter<'a> {
    format: &'a [u8],
    index: usize,
}

impl<'a> FormatIter<'a> {
    /// Scan `format`.
    #[must_use]
    pub(crate) const fn new(format: &'a [u8]) -> Self {
        Self { format, index: 0 }
    }
}

impl Iterator for FormatIter<'_> {
    type Item = Item;

    fn next(&mut self) -> Option<Self::Item> {
        let byte = *self.format.get(self.index)?;
        self.index += 1;
        if byte != b'%' {
            return Some(Item::Literal(byte));
        }
        let mut directive = Directive {
            conversion: 0,
            pad: Pad::Default,
            upper: false,
            swap: false,
            alt_e: 0,
            alt_o: 0,
        };
        loop {
            match self.format.get(self.index).copied() {
                Some(b'-') => directive.pad = Pad::Less,
                Some(b'_') => directive.pad = Pad::Space,
                Some(b'0') => directive.pad = Pad::Zero,
                Some(b'^') => directive.upper = true,
                Some(b'#') => directive.swap = true,
                Some(b'E') => directive.alt_e = directive.alt_e.saturating_add(1),
                Some(b'O') => directive.alt_o = directive.alt_o.saturating_add(1),
                _ => break,
            }
            self.index += 1;
        }
        let Some(conversion) = self.format.get(self.index).copied() else {
            return Some(Item::TrailingPercent);
        };
        self.index += 1;
        directive.conversion = conversion;
        Some(Item::Directive(directive))
    }
}

#[cfg(test)]
#[path = "timefmt_ut.rs"]
mod tests;
