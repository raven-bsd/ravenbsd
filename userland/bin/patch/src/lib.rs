// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `patch`: parse and apply unified diffs.
//!
//! The applier is intentionally strict: context and removed lines must match at the exact
//! expected offset. There is no context fuzz, no ed/context-diff support and no interactive
//! prompting. A failed hunk rejects the whole invocation before any file is written.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Why a patch or command line was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchError {
    /// The command line is invalid.
    Usage,
    /// The patch text is malformed or contains no file diff.
    InvalidPatch,
    /// A file named by the patch does not exist and the hunk does not create it.
    MissingFile(String),
    /// A hunk's context or removed lines do not match the target.
    HunkMismatch {
        /// Target file.
        file: String,
        /// One-based hunk number in the patch file.
        hunk: usize,
    },
    /// A file could not be written or removed.
    Io(String),
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Usage => formatter.write_str("invalid usage"),
            Self::InvalidPatch => formatter.write_str("malformed patch"),
            Self::MissingFile(path) => write!(formatter, "{path}: file to patch not found"),
            Self::HunkMismatch { file, hunk } => {
                write!(formatter, "{file}: hunk #{hunk} does not apply")
            }
            Self::Io(path) => write!(formatter, "{path}: I/O error"),
        }
    }
}

/// Apply options.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Number of leading path components stripped by `-p`.
    pub strip: usize,
    /// Apply the patch in reverse (`-R`).
    pub reverse: bool,
    /// Accept `-f`; matching remains strict.
    pub force: bool,
    /// Accept `-u`; only unified diffs are supported.
    pub unified: bool,
}

/// A parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invocation {
    /// Apply options.
    pub options: Options,
    /// Patch-file operands.
    pub patch_files: Vec<String>,
    /// `-i patchfile`, if present.
    pub input: Option<String>,
}

/// Parse `-i`, `-pN`, `-R`, `-u`, `-f`, `--` and patch-file operands.
pub fn parse_args<I, S>(arguments: I) -> Result<Invocation, PatchError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let arguments: Vec<String> = arguments
        .into_iter()
        .map(|value| value.as_ref().to_string())
        .collect();
    let mut invocation = Invocation::default();
    let mut index = 0usize;
    let mut end_of_options = false;
    while index < arguments.len() {
        let argument = &arguments[index];
        if end_of_options {
            invocation.patch_files.push(argument.clone());
            index = index.saturating_add(1);
            continue;
        }
        if argument == "--" {
            end_of_options = true;
            index = index.saturating_add(1);
            continue;
        }
        if argument == "-R" {
            invocation.options.reverse = true;
            index = index.saturating_add(1);
            continue;
        }
        if argument == "-u" {
            invocation.options.unified = true;
            index = index.saturating_add(1);
            continue;
        }
        if argument == "-f" {
            invocation.options.force = true;
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = argument.strip_prefix("-p") {
            let value = if value.is_empty() {
                index = index.saturating_add(1);
                arguments.get(index).ok_or(PatchError::Usage)?.as_str()
            } else {
                value
            };
            invocation.options.strip = parse_strip(value)?;
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = argument.strip_prefix("-i") {
            if invocation.input.is_some() {
                return Err(PatchError::Usage);
            }
            let value = if value.is_empty() {
                index = index.saturating_add(1);
                arguments.get(index).ok_or(PatchError::Usage)?.as_str()
            } else {
                value
            };
            invocation.input = Some(value.to_string());
            index = index.saturating_add(1);
            continue;
        }
        if argument.starts_with('-') && argument.len() > 1 {
            return Err(PatchError::Usage);
        }
        invocation.patch_files.push(argument.clone());
        index = index.saturating_add(1);
    }
    if invocation.input.is_some() && !invocation.patch_files.is_empty() {
        return Err(PatchError::Usage);
    }
    Ok(invocation)
}

fn parse_strip(value: &str) -> Result<usize, PatchError> {
    value.parse::<usize>().map_err(|_| PatchError::Usage)
}

/// File operations used by [`apply`].
pub trait FileSystem {
    /// Read `path`, or return `None` when it does not exist.
    fn read(&mut self, path: &str) -> Option<String>;
    /// Write `contents` to `path`; return false on error.
    fn write(&mut self, path: &str, contents: &str) -> bool;
    /// Remove `path`; return false on error.
    fn remove(&mut self, path: &str) -> bool;
}

/// What one patch invocation did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// File diffs applied.
    pub files: usize,
    /// Files created.
    pub created: usize,
    /// Files deleted.
    pub deleted: usize,
    /// Hunks applied.
    pub hunks: usize,
}

/// Parse and apply unified `patch_text` to `files`.
///
/// Every file is validated and transformed in memory first; a mismatch anywhere returns an error
/// without writing or removing any file.
pub fn apply(
    patch_text: &str,
    options: &Options,
    files: &mut dyn FileSystem,
) -> Result<Summary, PatchError> {
    let patch = parse_patch(patch_text)?;
    apply_files(patch, options, files)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FilePatch {
    old_name: String,
    new_name: String,
    hunks: Vec<Hunk>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Hunk {
    old_start: usize,
    old_count: usize,
    new_start: usize,
    new_count: usize,
    lines: Vec<HunkLine>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HunkLine {
    kind: HunkKind,
    text: String,
    no_newline: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HunkKind {
    Context,
    Remove,
    Add,
}

fn parse_patch(text: &str) -> Result<Vec<FilePatch>, PatchError> {
    let mut lines: Vec<&str> = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
        .collect();
    if lines.last() == Some(&"") {
        lines.pop();
    }
    let mut patches = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let Some(old_header) = lines[index].strip_prefix("--- ") else {
            index = index.saturating_add(1);
            continue;
        };
        let old_name = header_name(old_header)?;
        index = index.saturating_add(1);
        let new_header = lines.get(index).ok_or(PatchError::InvalidPatch)?;
        let new_name = header_name(
            new_header
                .strip_prefix("+++ ")
                .ok_or(PatchError::InvalidPatch)?,
        )?;
        index = index.saturating_add(1);
        let mut hunks = Vec::new();
        while index < lines.len() {
            let line = lines[index];
            if line.starts_with("--- ") {
                break;
            }
            let Some(header) = line.strip_prefix("@@ ") else {
                index = index.saturating_add(1);
                continue;
            };
            let (hunk, next) = parse_hunk(&lines, index, header)?;
            hunks.push(hunk);
            index = next;
        }
        if hunks.is_empty() {
            return Err(PatchError::InvalidPatch);
        }
        patches.push(FilePatch {
            old_name,
            new_name,
            hunks,
        });
    }
    if patches.is_empty() {
        return Err(PatchError::InvalidPatch);
    }
    Ok(patches)
}

fn header_name(value: &str) -> Result<String, PatchError> {
    let name = value.split('\t').next().unwrap_or("").trim();
    if name.is_empty() {
        return Err(PatchError::InvalidPatch);
    }
    Ok(name.to_string())
}

fn parse_hunk(lines: &[&str], start: usize, header: &str) -> Result<(Hunk, usize), PatchError> {
    let mut parts = header.split_whitespace();
    let old_range = parts.next().ok_or(PatchError::InvalidPatch)?;
    let new_range = parts.next().ok_or(PatchError::InvalidPatch)?;
    let old_range = old_range
        .strip_prefix('-')
        .ok_or(PatchError::InvalidPatch)?;
    let new_range = new_range
        .strip_prefix('+')
        .ok_or(PatchError::InvalidPatch)?;
    let (old_start, old_count) = parse_range(old_range)?;
    let (new_start, new_count) = parse_range(new_range)?;
    let mut index = start.saturating_add(1);
    let mut hunk_lines: Vec<HunkLine> = Vec::new();
    let mut old_seen = 0usize;
    let mut new_seen = 0usize;
    while old_seen < old_count || new_seen < new_count {
        let line = *lines.get(index).ok_or(PatchError::InvalidPatch)?;
        if line.starts_with('\\') {
            let previous = hunk_lines.last_mut().ok_or(PatchError::InvalidPatch)?;
            previous.no_newline = true;
            index = index.saturating_add(1);
            continue;
        }
        let first = line.chars().next().ok_or(PatchError::InvalidPatch)?;
        let (kind, text) = match first {
            ' ' => (HunkKind::Context, &line[1..]),
            '-' => (HunkKind::Remove, &line[1..]),
            '+' => (HunkKind::Add, &line[1..]),
            _ => return Err(PatchError::InvalidPatch),
        };
        match kind {
            HunkKind::Context => {
                old_seen = old_seen.saturating_add(1);
                new_seen = new_seen.saturating_add(1);
            }
            HunkKind::Remove => old_seen = old_seen.saturating_add(1),
            HunkKind::Add => new_seen = new_seen.saturating_add(1),
        }
        hunk_lines.push(HunkLine {
            kind,
            text: text.to_string(),
            no_newline: false,
        });
        index = index.saturating_add(1);
    }
    if let Some(line) = lines.get(index)
        && line.starts_with('\\')
    {
        let previous = hunk_lines.last_mut().ok_or(PatchError::InvalidPatch)?;
        previous.no_newline = true;
        index = index.saturating_add(1);
    }
    if let Some(line) = lines.get(index) {
        let is_next_header = line.starts_with("@@ ") || line.starts_with("--- ");
        let is_preamble = line.starts_with("diff ") || line.starts_with("index ");
        let looks_like_body = line.starts_with(' ')
            || line.starts_with('+')
            || line.starts_with('\\')
            || (line.starts_with('-') && !line.starts_with("--- "));
        if looks_like_body && !is_next_header && !is_preamble {
            return Err(PatchError::InvalidPatch);
        }
    }
    Ok((
        Hunk {
            old_start,
            old_count,
            new_start,
            new_count,
            lines: hunk_lines,
        },
        index,
    ))
}

fn parse_range(value: &str) -> Result<(usize, usize), PatchError> {
    let (start, count) = match value.split_once(',') {
        Some((start, count)) => (start, count),
        None => (value, "1"),
    };
    let start = start
        .parse::<usize>()
        .map_err(|_| PatchError::InvalidPatch)?;
    let count = count
        .parse::<usize>()
        .map_err(|_| PatchError::InvalidPatch)?;
    Ok((start, count))
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TextFile {
    lines: Vec<String>,
    end_newline: bool,
}

impl TextFile {
    fn from_text(text: &str) -> Self {
        if text.is_empty() {
            return Self {
                lines: Vec::new(),
                end_newline: false,
            };
        }
        let end_newline = text.ends_with('\n');
        let mut lines: Vec<String> = text.split('\n').map(ToString::to_string).collect();
        if end_newline {
            lines.pop();
        }
        Self { lines, end_newline }
    }

    fn render(&self) -> String {
        let mut output = self.lines.join("\n");
        if self.end_newline && !self.lines.is_empty() {
            output.push('\n');
        }
        output
    }
}

fn apply_files(
    patches: Vec<FilePatch>,
    options: &Options,
    files: &mut dyn FileSystem,
) -> Result<Summary, PatchError> {
    let mut summary = Summary::default();
    let mut pending: BTreeMap<String, Option<TextFile>> = BTreeMap::new();
    for mut file_patch in patches {
        if options.reverse {
            file_patch = reverse_file(file_patch);
        }
        let delete = file_patch.new_name == "/dev/null";
        let create = file_patch.old_name == "/dev/null";
        let target = target_name(&file_patch, options.strip)?;
        let existing = match pending.get(&target) {
            Some(Some(file)) => Some(file.clone()),
            Some(None) => return Err(PatchError::MissingFile(target.clone())),
            None => files.read(&target).map(|text| TextFile::from_text(&text)),
        };
        let existed = existing.is_some();
        let mut file = match existing {
            Some(file) => file,
            None => {
                let creates = create
                    || file_patch
                        .hunks
                        .first()
                        .is_some_and(|hunk| hunk.old_start == 0 && hunk.old_count == 0);
                if !creates {
                    return Err(PatchError::MissingFile(target.clone()));
                }
                TextFile {
                    lines: Vec::new(),
                    end_newline: false,
                }
            }
        };
        if delete && !existed {
            return Err(PatchError::MissingFile(target.clone()));
        }
        let mut offset = 0isize;
        for (number, hunk) in file_patch.hunks.iter().enumerate() {
            apply_hunk(
                &mut file,
                hunk,
                &mut offset,
                &target,
                number.saturating_add(1),
            )?;
            summary.hunks = summary.hunks.saturating_add(1);
        }
        if !delete && !existed {
            summary.created = summary.created.saturating_add(1);
        }
        if delete {
            summary.deleted = summary.deleted.saturating_add(1);
        }
        summary.files = summary.files.saturating_add(1);
        pending.insert(target, if delete { None } else { Some(file) });
    }
    for (path, contents) in pending {
        match contents {
            Some(file) => {
                if !files.write(&path, &file.render()) {
                    return Err(PatchError::Io(path));
                }
            }
            None => {
                if !files.remove(&path) {
                    return Err(PatchError::Io(path));
                }
            }
        }
    }
    Ok(summary)
}

fn target_name(file: &FilePatch, strip: usize) -> Result<String, PatchError> {
    let name = if file.new_name == "/dev/null" {
        file.old_name.as_str()
    } else {
        file.new_name.as_str()
    };
    if name == "/dev/null" {
        return Err(PatchError::InvalidPatch);
    }
    strip_components(name, strip)
}

fn strip_components(name: &str, count: usize) -> Result<String, PatchError> {
    if name.is_empty() {
        return Err(PatchError::InvalidPatch);
    }
    if name.split('/').any(|component| component == "..") {
        return Err(PatchError::InvalidPatch);
    }
    if count == 0 {
        return Ok(name.to_string());
    }
    let mut parts = name.split('/');
    for _ in 0..count {
        parts.next().ok_or(PatchError::InvalidPatch)?;
    }
    let remaining: Vec<&str> = parts.collect();
    if remaining.is_empty() || remaining.iter().all(|part| part.is_empty()) {
        return Err(PatchError::InvalidPatch);
    }
    Ok(remaining.join("/"))
}

fn reverse_file(mut file: FilePatch) -> FilePatch {
    core::mem::swap(&mut file.old_name, &mut file.new_name);
    for hunk in &mut file.hunks {
        core::mem::swap(&mut hunk.old_start, &mut hunk.new_start);
        core::mem::swap(&mut hunk.old_count, &mut hunk.new_count);
        for line in &mut hunk.lines {
            line.kind = match line.kind {
                HunkKind::Context => HunkKind::Context,
                HunkKind::Remove => HunkKind::Add,
                HunkKind::Add => HunkKind::Remove,
            };
        }
    }
    file
}

fn apply_hunk(
    file: &mut TextFile,
    hunk: &Hunk,
    offset: &mut isize,
    target: &str,
    number: usize,
) -> Result<(), PatchError> {
    let old_count = hunk
        .lines
        .iter()
        .filter(|line| line.kind != HunkKind::Add)
        .count();
    let new_count = hunk
        .lines
        .iter()
        .filter(|line| line.kind != HunkKind::Remove)
        .count();
    if old_count != hunk.old_count || new_count != hunk.new_count {
        return Err(PatchError::InvalidPatch);
    }
    let base = if hunk.old_start == 0 {
        0isize
    } else {
        isize::try_from(hunk.old_start.saturating_sub(1)).map_err(|_| PatchError::InvalidPatch)?
    };
    let start = base.saturating_add(*offset);
    if start < 0 {
        return Err(PatchError::HunkMismatch {
            file: target.to_string(),
            hunk: number,
        });
    }
    let start = usize::try_from(start).map_err(|_| PatchError::InvalidPatch)?;
    let end = start
        .checked_add(old_count)
        .ok_or(PatchError::InvalidPatch)?;
    if end > file.lines.len() {
        return Err(PatchError::HunkMismatch {
            file: target.to_string(),
            hunk: number,
        });
    }
    let mut cursor = start;
    for line in &hunk.lines {
        if line.kind == HunkKind::Add {
            continue;
        }
        let actual = file
            .lines
            .get(cursor)
            .ok_or_else(|| PatchError::HunkMismatch {
                file: target.to_string(),
                hunk: number,
            })?;
        if actual != &line.text {
            return Err(PatchError::HunkMismatch {
                file: target.to_string(),
                hunk: number,
            });
        }
        cursor = cursor.saturating_add(1);
    }
    let mut lines = Vec::new();
    lines.extend(file.lines.get(..start).unwrap_or(&[]).iter().cloned());
    let mut last_no_newline = false;
    let mut last_was_add = false;
    for line in &hunk.lines {
        match line.kind {
            HunkKind::Context | HunkKind::Add => {
                lines.push(line.text.clone());
                last_no_newline = line.no_newline;
                last_was_add = line.kind == HunkKind::Add;
            }
            HunkKind::Remove => {}
        }
    }
    let reaches_end = end == file.lines.len();
    lines.extend(file.lines.get(end..).unwrap_or(&[]).iter().cloned());
    file.lines = lines;
    if reaches_end {
        if last_no_newline {
            file.end_newline = false;
        } else if last_was_add {
            file.end_newline = true;
        }
    }
    *offset = (*offset).saturating_add(
        isize::try_from(new_count).unwrap_or(isize::MAX)
            - isize::try_from(old_count).unwrap_or(isize::MAX),
    );
    Ok(())
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
