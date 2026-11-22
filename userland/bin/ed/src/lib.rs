// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `ed` line-editor subset.
//!
//! Addresses are one-based lines with `.`, `$`, marks, `/re/` and `?re?`; ranges
//! use a comma. The editor is script-driven, which keeps the command model
//! host-testable. Shell escapes (`!`) are explicitly unsupported.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_libc_regex::consts::REG_BASIC;
use raven_libc_regex::locale::ByteLocale;
use raven_libc_regex::{RegMatch, Regex, regcomp, regexec};

/// Why an editor command failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// No current line is available.
    NoCurrent,
    /// An address is outside the buffer or malformed.
    BadAddress,
    /// A command letter is unknown.
    UnknownCommand,
    /// A regular expression is invalid.
    BadRegex,
    /// A command is malformed.
    BadCommand,
    /// File I/O failed.
    Io,
    /// The command uses a shell escape or another unsupported feature.
    Unsupported,
    /// A command input block is missing its terminating `.`.
    MissingTerminator,
}

/// File access used by `r`, `w` and `e`.
pub trait Io {
    /// Read a text file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be read.
    fn read(&mut self, path: &str) -> Result<String, Error>;
    /// Write a text file.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Io`] when the file cannot be written.
    fn write(&mut self, path: &str, data: &str) -> Result<(), Error>;
}

/// A no-I/O backend for callers that do not use file commands.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoIo;

impl Io for NoIo {
    fn read(&mut self, _path: &str) -> Result<String, Error> {
        Err(Error::Unsupported)
    }
    fn write(&mut self, _path: &str, _data: &str) -> Result<(), Error> {
        Err(Error::Unsupported)
    }
}

/// Parsed `ed` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Suppress the `!`/diagnostic prompts.
    pub silent: bool,
    /// Optional file to read at startup.
    pub file: Option<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An unknown option was supplied.
    UnknownOption,
    /// More than one file operand was supplied.
    ExtraOperand,
}

/// Parse `ed [-s] [file]`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option or multiple file operands.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let mut silent = false;
    let mut file = None;
    let mut options = true;
    for argument in args {
        if options && argument == "--" {
            options = false;
            continue;
        }
        if options && argument == "-s" {
            silent = true;
            continue;
        }
        if options && argument.starts_with('-') && argument.len() > 1 {
            return Err(UsageError::UnknownOption);
        }
        if file.is_some() {
            return Err(UsageError::ExtraOperand);
        }
        file = Some(argument);
    }
    Ok(Options { silent, file })
}

/// A script-driven editor buffer.
#[derive(Clone, Debug)]
pub struct Editor {
    lines: Vec<String>,
    current: usize,
    filename: Option<String>,
    modified: bool,
    marks: BTreeMap<char, usize>,
    undo: Option<Snapshot>,
    output: String,
    quit: bool,
    pending: Option<Pending>,
}

#[derive(Clone, Debug)]
struct Snapshot {
    lines: Vec<String>,
    current: usize,
    modified: bool,
}

/// A multiline `a`, `i` or `c` block waiting for its terminating `.`.
#[derive(Clone, Debug)]
enum Pending {
    Insert {
        at: usize,
        text: Vec<String>,
    },
    Change {
        first: usize,
        last: usize,
        text: Vec<String>,
    },
}

impl Editor {
    /// An empty editor.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lines: Vec::new(),
            current: 0,
            filename: None,
            modified: false,
            marks: BTreeMap::new(),
            undo: None,
            output: String::new(),
            quit: false,
            pending: None,
        }
    }

    /// The current buffer.
    #[must_use]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// The current line (one-based, zero for empty).
    #[must_use]
    pub const fn current(&self) -> usize {
        self.current
    }

    /// Whether the buffer was modified.
    #[must_use]
    pub const fn modified(&self) -> bool {
        self.modified
    }

    /// Take accumulated command output.
    pub fn take_output(&mut self) -> String {
        core::mem::take(&mut self.output)
    }

    /// Whether `q` has been executed.
    #[must_use]
    pub const fn quit(&self) -> bool {
        self.quit
    }

    /// Whether an `a`, `i` or `c` block is still waiting for its `.`.
    #[must_use]
    pub const fn input_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// Execute a newline-separated command script.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for an invalid address/command, a bad regular
    /// expression, an unsupported shell escape or a file I/O failure.
    pub fn execute(&mut self, script: &str, io: &mut dyn Io) -> Result<(), Error> {
        self.quit = false;
        self.pending = None;
        for line in script.lines() {
            if self.quit {
                break;
            }
            self.feed_line(line, io)?;
        }
        if self.pending.is_some() {
            return Err(Error::MissingTerminator);
        }
        Ok(())
    }

    /// Process one input line, preserving an open `a`/`i`/`c` block between calls.
    ///
    /// This is the console path: the caller can flush output and stop on `q`
    /// without waiting for an end-of-input marker.
    ///
    /// # Errors
    ///
    /// Returns [`Error`] for an invalid address/command, a bad regular
    /// expression, an unsupported shell escape or a file I/O failure.
    pub fn feed_line(&mut self, line: &str, io: &mut dyn Io) -> Result<(), Error> {
        if self.pending.is_some() {
            if line == "." {
                let pending = self.pending.take().ok_or(Error::BadCommand)?;
                self.commit(pending);
            } else if let Some(Pending::Insert { text, .. } | Pending::Change { text, .. }) =
                self.pending.as_mut()
            {
                text.push(line.to_string());
            }
            return Ok(());
        }
        self.dispatch(line, io)
    }

    fn commit(&mut self, pending: Pending) {
        match pending {
            Pending::Insert { at, text } => {
                self.snapshot();
                let start = at.min(self.lines.len());
                self.insert_lines(start, text);
                self.current = start.saturating_add(1).min(self.lines.len());
                self.modified = true;
            }
            Pending::Change { first, last, text } => {
                self.snapshot();
                self.lines.drain(first - 1..last);
                self.insert_lines(first - 1, text);
                self.current = first.min(self.lines.len());
                self.modified = true;
            }
        }
    }

    fn dispatch(&mut self, line: &str, io: &mut dyn Io) -> Result<(), Error> {
        if line.is_empty() {
            if self.current < self.lines.len() {
                self.current += 1;
            }
            return Ok(());
        }
        if line.starts_with('#') {
            return Ok(());
        }
        if line.starts_with('!') {
            return Err(Error::Unsupported);
        }
        let (address, command, rest) = split_command(line)?;
        match command {
            'a' => {
                let at = self.single(address, true)?;
                self.pending = Some(Pending::Insert {
                    at: at.min(self.lines.len()),
                    text: Vec::new(),
                });
            }
            'i' => {
                let at = self.single(address, true)?;
                self.pending = Some(Pending::Insert {
                    at: at.saturating_sub(1).min(self.lines.len()),
                    text: Vec::new(),
                });
            }
            'c' => {
                let (first, last) = self.range(address, false)?;
                self.pending = Some(Pending::Change {
                    first,
                    last,
                    text: Vec::new(),
                });
            }
            'd' => {
                let (first, last) = self.range(address, false)?;
                self.snapshot();
                self.lines.drain(first - 1..last);
                self.current = first.min(self.lines.len());
                self.modified = true;
            }
            'p' => self.print(address, false)?,
            'n' => self.print(address, true)?,
            'l' => self.print_list(address)?,
            's' => self.substitute(address, rest)?,
            'g' | 'v' => self.global(address, command == 'v', rest, io)?,
            'm' | 't' => self.move_copy(address, command, rest)?,
            'j' => self.join(address)?,
            'k' => self.mark(address, rest)?,
            '=' => self.print_number(address)?,
            'u' => self.undo()?,
            'q' => self.quit = true,
            'f' => self.file_name(rest)?,
            'e' => self.edit(rest, io)?,
            'r' => self.read(rest, address, io)?,
            'w' => self.write(rest, address, io)?,
            '!' => return Err(Error::Unsupported),
            _ => return Err(Error::UnknownCommand),
        }
        Ok(())
    }

    fn snapshot(&mut self) {
        self.undo = Some(Snapshot {
            lines: self.lines.clone(),
            current: self.current,
            modified: self.modified,
        });
    }

    fn undo(&mut self) -> Result<(), Error> {
        let snapshot = self.undo.take().ok_or(Error::BadCommand)?;
        self.lines = snapshot.lines;
        self.current = snapshot.current;
        self.modified = snapshot.modified;
        Ok(())
    }

    fn insert_lines(&mut self, at: usize, text: Vec<String>) {
        let count = text.len();
        for (offset, line) in text.into_iter().enumerate() {
            self.lines.insert(at + offset, line);
        }
        if count > 0 {
            self.current = at + count;
        }
    }

    fn single(&self, address: &str, allow_zero: bool) -> Result<usize, Error> {
        if address.is_empty() {
            if self.current == 0 && !allow_zero {
                return Err(Error::NoCurrent);
            }
            return Ok(self.current);
        }
        let value = self.address(address)?;
        if value == 0 && !allow_zero {
            return Err(Error::BadAddress);
        }
        Ok(value)
    }

    fn range(&self, address: &str, whole: bool) -> Result<(usize, usize), Error> {
        if address == "%" {
            if self.lines.is_empty() {
                return Err(Error::NoCurrent);
            }
            return Ok((1, self.lines.len()));
        }
        if address.is_empty() {
            if whole {
                if self.lines.is_empty() {
                    return Err(Error::NoCurrent);
                }
                return Ok((1, self.lines.len()));
            }
            if self.current == 0 {
                return Err(Error::NoCurrent);
            }
            return Ok((self.current, self.current));
        }
        if let Some((left, right)) = split_range(address) {
            let first = self.address(left)?;
            let last = self.address(right)?;
            if first == 0 || last == 0 || first > last || last > self.lines.len() {
                return Err(Error::BadAddress);
            }
            return Ok((first, last));
        }
        let line = self.address(address)?;
        if line == 0 || line > self.lines.len() {
            return Err(Error::BadAddress);
        }
        Ok((line, line))
    }

    fn address(&self, text: &str) -> Result<usize, Error> {
        let text = text.trim();
        if text == "." {
            return Ok(self.current);
        }
        if text == "$" {
            return Ok(self.lines.len());
        }
        if let Some(mark) = text.strip_prefix('\'') {
            let name = mark.chars().next().ok_or(Error::BadAddress)?;
            return self.marks.get(&name).copied().ok_or(Error::BadAddress);
        }
        if let Some(pattern) = text
            .strip_prefix('/')
            .and_then(|value| value.strip_suffix('/'))
        {
            return self.search(pattern, true);
        }
        if let Some(pattern) = text
            .strip_prefix('?')
            .and_then(|value| value.strip_suffix('?'))
        {
            return self.search(pattern, false);
        }
        if let Some((base, offset)) = split_offset(text) {
            let base = self.address(base)?;
            let offset: isize = offset.parse().map_err(|_| Error::BadAddress)?;
            let result = isize::try_from(base).map_err(|_| Error::BadAddress)? + offset;
            if result < 0 {
                return Err(Error::BadAddress);
            }
            return usize::try_from(result).map_err(|_| Error::BadAddress);
        }
        text.parse().map_err(|_| Error::BadAddress)
    }

    fn search(&self, pattern: &str, forward: bool) -> Result<usize, Error> {
        let regex = compile(pattern)?;
        let start = if forward {
            self.current.saturating_add(1)
        } else {
            self.current.saturating_sub(1)
        };
        if forward {
            for index in start..=self.lines.len() {
                if let Some(line) = self.lines.get(index.wrapping_sub(1))
                    && matches(&regex, line)?
                {
                    return Ok(index);
                }
            }
        } else {
            for index in (1..=start).rev() {
                if let Some(line) = self.lines.get(index - 1)
                    && matches(&regex, line)?
                {
                    return Ok(index);
                }
            }
        }
        Err(Error::BadAddress)
    }

    fn print(&mut self, address: &str, numbered: bool) -> Result<(), Error> {
        let (first, last) = self.range(address, false)?;
        for index in first..=last {
            let line = self.lines.get(index - 1).cloned().unwrap_or_default();
            if numbered {
                self.output.push_str(&format!("{index}\t{line}\n"));
            } else {
                self.output.push_str(&line);
                self.output.push('\n');
            }
        }
        self.current = last;
        Ok(())
    }

    fn print_list(&mut self, address: &str) -> Result<(), Error> {
        let (first, last) = self.range(address, false)?;
        for index in first..=last {
            let line = self.lines.get(index - 1).cloned().unwrap_or_default();
            self.output.push_str(&escape(&line));
            self.output.push_str("$\n");
        }
        self.current = last;
        Ok(())
    }

    fn print_number(&mut self, address: &str) -> Result<(), Error> {
        let line = self.single(address, false)?;
        self.output.push_str(&format!("{line}\n"));
        Ok(())
    }

    fn substitute(&mut self, address: &str, rest: &str) -> Result<(), Error> {
        let (pattern, replacement, flags) = parse_substitute(rest)?;
        let regex = compile(&pattern)?;
        let (first, last) = self.range(address, false)?;
        let global = flags.contains('g');
        let nth = flags
            .chars()
            .filter(|flag| flag.is_ascii_digit())
            .collect::<String>()
            .parse::<usize>()
            .ok();
        let print = flags.contains('p');
        self.snapshot();
        let mut changed = false;
        for index in first..=last {
            let line = self.lines.get(index - 1).cloned().unwrap_or_default();
            let (result, did_change) = replace_line(&regex, &line, &replacement, global, nth)?;
            if did_change {
                self.lines[index - 1] = result;
                changed = true;
                self.current = index;
                if print {
                    let line = self.lines.get(index - 1).cloned().unwrap_or_default();
                    self.output.push_str(&line);
                    self.output.push('\n');
                }
            }
        }
        if !changed {
            return Err(Error::BadRegex);
        }
        self.modified = true;
        Ok(())
    }

    fn global(
        &mut self,
        address: &str,
        invert: bool,
        rest: &str,
        io: &mut dyn Io,
    ) -> Result<(), Error> {
        let (pattern, command) = split_global(rest)?;
        let regex = compile(&pattern)?;
        let (first, last) = self.range(address, true)?;
        let mut indices = Vec::new();
        for index in first..=last {
            let line = self.lines.get(index - 1).cloned().unwrap_or_default();
            if matches(&regex, &line)? != invert {
                indices.push(index);
            }
        }
        if command.starts_with('d') {
            indices.reverse();
        }
        for index in indices {
            self.current = index;
            if self.range("", false).is_err() {
                continue;
            }
            self.dispatch(&command, io)?;
            if self.pending.is_some() {
                self.pending = None;
                return Err(Error::MissingTerminator);
            }
        }
        Ok(())
    }

    fn move_copy(&mut self, address: &str, command: char, rest: &str) -> Result<(), Error> {
        let (first, last) = self.range(address, false)?;
        let target = self.address(rest.trim())?;
        if target == 0 || target > self.lines.len() + 1 {
            return Err(Error::BadAddress);
        }
        if command == 'm' && target >= first && target <= last {
            return Err(Error::BadAddress);
        }
        self.snapshot();
        let block: Vec<String> = self.lines.drain(first - 1..last).collect();
        let insert = if target >= first && command == 'm' {
            target.saturating_sub(block.len())
        } else {
            target
        };
        self.insert_lines(insert, block);
        self.modified = true;
        Ok(())
    }

    fn join(&mut self, address: &str) -> Result<(), Error> {
        let (first, last) = self.range(address, false)?;
        if first == last {
            return Ok(());
        }
        self.snapshot();
        let joined = self.lines[first - 1..last].join(" ");
        self.lines.drain(first - 1..last);
        self.lines.insert(first - 1, joined);
        self.current = first;
        self.modified = true;
        Ok(())
    }

    fn mark(&mut self, address: &str, rest: &str) -> Result<(), Error> {
        let name = rest.chars().next().ok_or(Error::BadCommand)?;
        let line = self.single(address, false)?;
        self.marks.insert(name, line);
        Ok(())
    }

    fn file_name(&mut self, rest: &str) -> Result<(), Error> {
        let rest = rest.trim();
        if rest.is_empty() {
            let name = self.filename.clone().unwrap_or_default();
            self.output.push_str(&name);
            self.output.push('\n');
        } else {
            self.filename = Some(rest.to_string());
        }
        Ok(())
    }

    fn edit(&mut self, rest: &str, io: &mut dyn Io) -> Result<(), Error> {
        let path = rest.trim();
        if path.is_empty() {
            return Err(Error::BadCommand);
        }
        let text = io.read(path)?;
        self.snapshot();
        self.lines = split_lines(&text);
        self.current = if self.lines.is_empty() {
            0
        } else {
            self.lines.len()
        };
        self.filename = Some(path.to_string());
        self.modified = false;
        Ok(())
    }

    fn read(&mut self, rest: &str, address: &str, io: &mut dyn Io) -> Result<(), Error> {
        let path = rest.trim();
        if path.is_empty() {
            return Err(Error::BadCommand);
        }
        let text = io.read(path)?;
        let at = self.single(address, true)?;
        let lines = split_lines(&text);
        self.snapshot();
        self.insert_lines(at.min(self.lines.len()), lines);
        self.modified = true;
        Ok(())
    }

    fn write(&mut self, rest: &str, address: &str, io: &mut dyn Io) -> Result<(), Error> {
        let path = rest.trim();
        let path = if path.is_empty() {
            self.filename.clone().ok_or(Error::BadCommand)?
        } else {
            path.to_string()
        };
        let (first, last) = self.range(address, true)?;
        let mut data = String::new();
        for line in &self.lines[first - 1..last] {
            data.push_str(line);
            data.push('\n');
        }
        io.write(&path, &data)?;
        self.filename = Some(path);
        self.modified = false;
        Ok(())
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

fn split_lines(text: &str) -> Vec<String> {
    text.lines().map(ToString::to_string).collect()
}

fn split_command(line: &str) -> Result<(&str, char, &str), Error> {
    let bytes = line.as_bytes();
    let mut index = 0usize;
    let mut delimiter = 0u8;
    while index < bytes.len() {
        let byte = bytes[index];
        if delimiter != 0 {
            if byte == delimiter && (index == 0 || bytes[index - 1] != b'\\') {
                delimiter = 0;
            }
            index += 1;
            continue;
        }
        if byte == b'/' || byte == b'?' {
            delimiter = byte;
            index += 1;
            continue;
        }
        if byte == b'\'' {
            index = index.saturating_add(2);
            continue;
        }
        if b"acdijnplstmgvkwrefq=u!".contains(&byte) {
            let command = char::from(byte);
            return Ok((&line[..index], command, &line[index + 1..]));
        }
        index += 1;
    }
    Err(Error::UnknownCommand)
}

fn split_range(address: &str) -> Option<(&str, &str)> {
    let bytes = address.as_bytes();
    let mut delimiter = 0u8;
    for (index, byte) in bytes.iter().enumerate() {
        if delimiter != 0 {
            if *byte == delimiter && (index == 0 || bytes[index - 1] != b'\\') {
                delimiter = 0;
            }
            continue;
        }
        if *byte == b'/' || *byte == b'?' {
            delimiter = *byte;
            continue;
        }
        if *byte == b',' {
            return Some((&address[..index], &address[index + 1..]));
        }
    }
    None
}

fn split_offset(text: &str) -> Option<(&str, &str)> {
    let mut index = text.len();
    while index > 0 {
        let byte = text.as_bytes()[index - 1];
        if byte == b'+' || byte == b'-' {
            let base = &text[..index - 1];
            if base.is_empty() {
                return None;
            }
            return Some((base, &text[index - 1..]));
        }
        if !byte.is_ascii_digit() {
            return None;
        }
        index -= 1;
    }
    None
}

fn parse_substitute(rest: &str) -> Result<(String, String, String), Error> {
    let bytes = rest.as_bytes();
    let delimiter = *bytes.first().ok_or(Error::BadCommand)?;
    let mut parts = Vec::new();
    let mut start = 1usize;
    let mut index = 1usize;
    while index < bytes.len() {
        if bytes[index] == delimiter && (index == 0 || bytes[index - 1] != b'\\') {
            parts.push(rest[start..index].to_string());
            start = index + 1;
            if parts.len() == 2 {
                break;
            }
        }
        index += 1;
    }
    if parts.len() != 2 {
        return Err(Error::BadCommand);
    }
    Ok((
        parts[0].clone(),
        parts[1].clone(),
        rest[start..].to_string(),
    ))
}

fn split_global(rest: &str) -> Result<(String, String), Error> {
    let bytes = rest.as_bytes();
    let delimiter = *bytes.first().ok_or(Error::BadCommand)?;
    let mut index = 1usize;
    while index < bytes.len() {
        if bytes[index] == delimiter && bytes[index - 1] != b'\\' {
            let pattern = rest[1..index].to_string();
            let command = rest[index + 1..].to_string();
            return Ok((
                pattern,
                if command.is_empty() {
                    "p".to_string()
                } else {
                    command
                },
            ));
        }
        index += 1;
    }
    Err(Error::BadCommand)
}

fn compile(pattern: &str) -> Result<Regex, Error> {
    regcomp(pattern.as_bytes(), REG_BASIC, &ByteLocale).map_err(|_| Error::BadRegex)
}

fn matches(regex: &Regex, line: &str) -> Result<bool, Error> {
    match regexec(regex, line.as_bytes(), &mut [], 0, &ByteLocale) {
        Ok(()) => Ok(true),
        Err(error) if error.0 == raven_libc_regex::consts::REG_NOMATCH => Ok(false),
        Err(_) => Err(Error::BadRegex),
    }
}

fn replace_line(
    regex: &Regex,
    line: &str,
    replacement: &str,
    global: bool,
    nth: Option<usize>,
) -> Result<(String, bool), Error> {
    let subject = line.as_bytes();
    let mut slots = vec![RegMatch::default(); regex.nsub().saturating_add(1)];
    let target = nth.unwrap_or(if global { usize::MAX } else { 1 });
    if target == 0 {
        return Err(Error::BadCommand);
    }
    let mut result = Vec::new();
    let mut cursor = 0usize;
    let mut copied = 0usize;
    let mut occurrence = 0usize;
    let mut changed = false;
    while cursor <= subject.len() {
        if regexec(regex, &subject[cursor..], &mut slots, 0, &ByteLocale).is_err() {
            break;
        }
        let start = cursor.saturating_add(slots[0].rm_so.max(0) as usize);
        let end = cursor.saturating_add(slots[0].rm_eo.max(0) as usize);
        if end < start || end > subject.len() {
            break;
        }
        occurrence = occurrence.saturating_add(1);
        result.extend_from_slice(subject.get(copied..start).unwrap_or(&[]));
        let replace = occurrence == target || (nth.is_none() && global);
        if replace {
            result.extend_from_slice(expand(replacement, &subject[cursor..], &slots).as_slice());
            changed = true;
        } else {
            result.extend_from_slice(subject.get(start..end).unwrap_or(&[]));
        }
        copied = end;
        if occurrence == target && !global {
            break;
        }
        if end == start {
            if end >= subject.len() {
                break;
            }
            let step = next_boundary(subject, end);
            result.extend_from_slice(subject.get(end..step).unwrap_or(&[]));
            copied = step;
            cursor = step;
        } else {
            cursor = end;
        }
    }
    result.extend_from_slice(subject.get(copied..).unwrap_or(&[]));
    Ok((String::from_utf8_lossy(&result).into_owned(), changed))
}

fn next_boundary(subject: &[u8], index: usize) -> usize {
    let mut next = index.saturating_add(1);
    while next < subject.len() && (subject.get(next).copied().unwrap_or(0) & 0xc0) == 0x80 {
        next = next.saturating_add(1);
    }
    next
}

fn expand(replacement: &str, subject: &[u8], slots: &[RegMatch]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut chars = replacement.chars();
    while let Some(character) = chars.next() {
        if character == '&' {
            if let Some(slot) = slots.first() {
                output.extend_from_slice(
                    subject
                        .get(slot.rm_so.max(0) as usize..slot.rm_eo.max(0) as usize)
                        .unwrap_or(&[]),
                );
            }
        } else if character == '\\' {
            match chars.next() {
                Some(digit @ '1'..='9') => {
                    let index = digit.to_digit(10).unwrap_or(0) as usize;
                    if let Some(slot) = slots.get(index) {
                        output.extend_from_slice(
                            subject
                                .get(slot.rm_so.max(0) as usize..slot.rm_eo.max(0) as usize)
                                .unwrap_or(&[]),
                        );
                    }
                }
                Some(next) => output.extend_from_slice(next.to_string().as_bytes()),
                None => output.push(b'\\'),
            }
        } else {
            let mut buffer = [0u8; 4];
            output.extend_from_slice(character.encode_utf8(&mut buffer).as_bytes());
        }
    }
    output
}

fn escape(line: &str) -> String {
    let mut output = String::new();
    for character in line.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '\t' => output.push_str("\\t"),
            '$' => output.push_str("\\$"),
            other => output.push(other),
        }
    }
    output
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
