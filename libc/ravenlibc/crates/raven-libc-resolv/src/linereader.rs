// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A bounded line reader that skips an over-long line whole.
//!
//! The donor `fgets` loop splits a line larger than its buffer and reinterprets the tail as a
//! new line; the ticket forbids that (A-net-10, D-12-3). This reader consumes the whole
//! physical line and reports `None` for it, so the caller never sees a fragment.

/// Iterates the lines of a byte slice, dropping every line longer than `max_line`.
pub struct LineReader<'a> {
    data: &'a [u8],
    pos: usize,
    max_line: usize,
    skipped: usize,
}

impl<'a> LineReader<'a> {
    /// A reader over `data`; `max_line` is the longest accepted line without its newline.
    #[must_use]
    pub const fn new(data: &'a [u8], max_line: usize) -> Self {
        Self {
            data,
            pos: 0,
            max_line,
            skipped: 0,
        }
    }

    /// The next accepted line, with its `\n` and a trailing `\r` removed.
    ///
    /// Over-long lines are consumed whole and never returned.
    pub fn next_line(&mut self) -> Option<&'a [u8]> {
        while self.pos < self.data.len() {
            let rest = self.data.get(self.pos..)?;
            let newline = rest.iter().position(|byte| *byte == b'\n');
            let (line, next) = match newline {
                Some(index) => (
                    rest.get(..index)?,
                    self.pos.checked_add(index)?.checked_add(1)?,
                ),
                None => (rest, self.data.len()),
            };
            self.pos = next;
            if line.len() > self.max_line {
                self.skipped = self.skipped.saturating_add(1);
                continue;
            }
            return Some(trim_cr(line));
        }
        None
    }

    /// The number of lines dropped for being over-long.
    #[must_use]
    pub const fn skipped(&self) -> usize {
        self.skipped
    }

    /// The byte offset after the last returned (or skipped) line.
    #[must_use]
    pub const fn position(&self) -> usize {
        self.pos
    }

    /// Restart from the beginning.
    pub fn rewind(&mut self) {
        self.pos = 0;
        self.skipped = 0;
    }
}

/// Strip one trailing carriage return.
#[must_use]
pub fn trim_cr(line: &[u8]) -> &[u8] {
    match line.last() {
        Some(b'\r') => line.get(..line.len().saturating_sub(1)).unwrap_or(line),
        _ => line,
    }
}

/// Trim ASCII whitespace from both ends.
#[must_use]
pub fn trim(line: &[u8]) -> &[u8] {
    let mut start = 0usize;
    while start < line.len()
        && line
            .get(start)
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        start = start.saturating_add(1);
    }
    let mut end = line.len();
    while end > start
        && line
            .get(end.saturating_sub(1))
            .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        end = end.saturating_sub(1);
    }
    line.get(start..end).unwrap_or(&[])
}

/// Split on ASCII whitespace, dropping empty tokens.
pub fn words(line: &[u8]) -> impl Iterator<Item = &[u8]> + '_ {
    line.split(|byte| byte.is_ascii_whitespace())
        .filter(|word| !word.is_empty())
}

/// Cut a line at the first `#`.
#[must_use]
pub fn strip_comment(line: &[u8]) -> &[u8] {
    match line.iter().position(|byte| *byte == b'#') {
        Some(index) => line.get(..index).unwrap_or(&[]),
        None => line,
    }
}

#[cfg(test)]
#[path = "linereader_ut.rs"]
mod tests;
