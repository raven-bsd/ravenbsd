// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line tokenizer, parser and variable expansion for `rsh` (K37).
//!
//! The crate is pure: it turns one input line into a [`List`] of commands and expands their
//! words against a [`Vars`] table. It performs no I/O and has no clock, so every rule of
//! `architecture/16-first-shell.md` section 4 is host-testable.
//!
//! Supported: words (single quotes, double quotes, backslash), `#` comments, the connectors `;`,
//! `&&` and `||`, and the redirections `>`, `>>` and `<`. A pipe is parsed as an error, not
//! silently ignored: the shell reports `rsh: pipes are not supported yet`. Variable expansion
//! understands `$NAME`, `${NAME}` and `$?`; single quotes and a backslash suppress it.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Longest accepted input line, in bytes.
pub const MAX_LINE: usize = 1024;
/// Maximum number of words in one command.
pub const MAX_WORDS: usize = 64;
/// Maximum number of redirections in one command.
pub const MAX_REDIRS: usize = 8;

/// The kind of a redirection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirKind {
    /// `>`: truncate and write.
    Out,
    /// `>>`: append.
    Append,
    /// `<`: read.
    In,
}

/// One redirection and its target word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirection {
    /// Direction of the redirection.
    pub kind: RedirKind,
    /// Target path, unexpanded.
    pub target: Word,
}

/// A piece of a word: text that is copied verbatim, or text where `$...` expands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    /// Quoted or escaped text, copied as-is.
    Literal(String),
    /// Unquoted text, where `$NAME`, `${NAME}` and `$?` expand.
    Expandable(String),
}

/// One shell word, as a sequence of segments.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Word {
    /// The segments in source order.
    pub segments: Vec<Segment>,
}

impl Word {
    /// A word made of one literal segment.
    pub fn literal(text: &str) -> Self {
        Self {
            segments: alloc::vec![Segment::Literal(text.to_string())],
        }
    }

    /// A word made of one expandable segment.
    pub fn expandable(text: &str) -> Self {
        Self {
            segments: alloc::vec![Segment::Expandable(text.to_string())],
        }
    }

    /// Append literal text.
    pub fn push_literal(&mut self, text: &str) {
        match self.segments.last_mut() {
            Some(Segment::Literal(current)) => current.push_str(text),
            _ => self.segments.push(Segment::Literal(text.to_string())),
        }
    }

    /// Append expandable text.
    pub fn push_expandable(&mut self, text: &str) {
        match self.segments.last_mut() {
            Some(Segment::Expandable(current)) => current.push_str(text),
            _ => self.segments.push(Segment::Expandable(text.to_string())),
        }
    }

    /// Concatenate the segments without expanding anything (diagnostics and tests).
    pub fn render(&self) -> String {
        let mut text = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Literal(value) | Segment::Expandable(value) => text.push_str(value),
            }
        }
        text
    }

    /// True when the word has no text at all (for example `''`).
    pub fn is_blank(&self) -> bool {
        self.segments.iter().all(|segment| match segment {
            Segment::Literal(value) | Segment::Expandable(value) => value.is_empty(),
        })
    }
}

/// One command: words and redirections in source order.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Command {
    /// The command words; the first one is the command name.
    pub words: Vec<Word>,
    /// The redirections.
    pub redirs: Vec<Redirection>,
}

/// How a command is joined to the previous one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    /// `;` or the start of the list: run unconditionally.
    Next,
    /// `&&`: run when the previous status is zero.
    And,
    /// `||`: run when the previous status is nonzero.
    Or,
}

/// One command with its connector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndOr {
    /// Connector to the previous command.
    pub connector: Connector,
    /// The command itself.
    pub command: Command,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct List {
    /// The commands in source order.
    pub items: Vec<AndOr>,
}

impl List {
    /// True for an empty line (or one that only had a comment).
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

/// A tokenizer or parser failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    /// The line had no command.
    Empty,
    /// A quote was not closed.
    UnterminatedQuote(char),
    /// A backslash ended the line.
    TrailingBackslash,
    /// A pipe was used.
    PipeUnsupported,
    /// A redirection had no target.
    MissingRedirectionTarget,
    /// An operator appeared where a command was expected.
    UnexpectedOperator(&'static str),
    /// More words than [`MAX_WORDS`].
    TooManyWords,
    /// More redirections than [`MAX_REDIRS`].
    TooManyRedirs,
}

impl ParseError {
    /// The message the shell prints after `rsh: `.
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "empty command",
            Self::UnterminatedQuote(_) => "unterminated quote",
            Self::TrailingBackslash => "trailing backslash",
            Self::PipeUnsupported => "pipes are not supported yet",
            Self::MissingRedirectionTarget => "redirection needs a target",
            Self::UnexpectedOperator(_) => "syntax error",
            Self::TooManyWords => "too many words",
            Self::TooManyRedirs => "too many redirections",
        }
    }
}

/// A tokenizer result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Token {
    /// A word.
    Word(Word),
    /// `;`
    Semi,
    /// `&&`
    AndIf,
    /// `||`
    OrIf,
    /// `|`
    Pipe,
    /// `>`, `>>` or `<`
    Redir(RedirKind),
}

/// Split one line into tokens.
pub fn tokenize(line: &str) -> Result<Vec<Token>, ParseError> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' | b'\n' => index += 1,
            b'#' => break,
            b';' => {
                tokens.push(Token::Semi);
                index += 1;
            }
            b'|' => {
                if bytes.get(index + 1) == Some(&b'|') {
                    tokens.push(Token::OrIf);
                    index += 2;
                } else {
                    tokens.push(Token::Pipe);
                    index += 1;
                }
            }
            b'&' => {
                if bytes.get(index + 1) == Some(&b'&') {
                    tokens.push(Token::AndIf);
                    index += 2;
                } else {
                    return Err(ParseError::UnexpectedOperator("&"));
                }
            }
            b'>' => {
                if bytes.get(index + 1) == Some(&b'>') {
                    tokens.push(Token::Redir(RedirKind::Append));
                    index += 2;
                } else {
                    tokens.push(Token::Redir(RedirKind::Out));
                    index += 1;
                }
            }
            b'<' => {
                tokens.push(Token::Redir(RedirKind::In));
                index += 1;
            }
            _ => {
                let (word, consumed) = read_word(bytes, index)?;
                tokens.push(Token::Word(word));
                index = consumed;
            }
        }
    }
    Ok(tokens)
}

/// Read one word starting at `start`; returns it and the index after it.
fn read_word(bytes: &[u8], start: usize) -> Result<(Word, usize), ParseError> {
    let mut word = Word::default();
    let mut index = start;
    while index < bytes.len() {
        match bytes[index] {
            b' ' | b'\t' | b'\r' | b'\n' => break,
            b';' | b'|' | b'&' | b'>' | b'<' => break,
            b'\'' => {
                let (text, next) = read_single_quoted(bytes, index)?;
                word.push_literal(&text);
                index = next;
            }
            b'"' => {
                let (segments, next) = read_double_quoted(bytes, index)?;
                for segment in segments {
                    match segment {
                        Segment::Literal(text) => word.push_literal(&text),
                        Segment::Expandable(text) => word.push_expandable(&text),
                    }
                }
                index = next;
            }
            b'\\' => {
                let Some(escaped) = bytes.get(index + 1) else {
                    return Err(ParseError::TrailingBackslash);
                };
                let text = [*escaped];
                word.push_literal(core::str::from_utf8(&text).unwrap_or(""));
                index += 2;
            }
            _ => {
                let begin = index;
                while index < bytes.len()
                    && !matches!(
                        bytes[index],
                        b' ' | b'\t'
                            | b'\r'
                            | b'\n'
                            | b';'
                            | b'|'
                            | b'&'
                            | b'>'
                            | b'<'
                            | b'\''
                            | b'"'
                            | b'\\'
                    )
                {
                    index += 1;
                }
                let text = core::str::from_utf8(&bytes[begin..index]).unwrap_or("");
                word.push_expandable(text);
            }
        }
    }
    Ok((word, index))
}

fn read_single_quoted(bytes: &[u8], start: usize) -> Result<(String, usize), ParseError> {
    let mut text = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        if bytes[index] == b'\'' {
            return Ok((text, index + 1));
        }
        text.push(bytes[index] as char);
        index += 1;
    }
    Err(ParseError::UnterminatedQuote('\''))
}

fn read_double_quoted(bytes: &[u8], start: usize) -> Result<(Vec<Segment>, usize), ParseError> {
    let mut segments = Vec::new();
    let mut literal = String::new();
    let mut expandable = String::new();
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => {
                flush_segment(&mut segments, &mut literal, &mut expandable);
                return Ok((segments, index + 1));
            }
            b'\\' => {
                let Some(escaped) = bytes.get(index + 1) else {
                    return Err(ParseError::TrailingBackslash);
                };
                match escaped {
                    b'"' | b'\\' | b'$' => {
                        // A quoted escape is literal text, so expansion cannot reach it.
                        if !expandable.is_empty() {
                            segments.push(Segment::Expandable(core::mem::take(&mut expandable)));
                        }
                        literal.push(*escaped as char);
                        index += 2;
                    }
                    _ => {
                        expandable.push('\\');
                        index += 1;
                    }
                }
            }
            byte => {
                if !literal.is_empty() {
                    segments.push(Segment::Literal(core::mem::take(&mut literal)));
                }
                expandable.push(byte as char);
                index += 1;
            }
        }
    }
    Err(ParseError::UnterminatedQuote('"'))
}

fn flush_segment(segments: &mut Vec<Segment>, literal: &mut String, expandable: &mut String) {
    if !literal.is_empty() {
        segments.push(Segment::Literal(core::mem::take(literal)));
    }
    if !expandable.is_empty() {
        segments.push(Segment::Expandable(core::mem::take(expandable)));
    }
}

/// Parse one line into a command list.
pub fn parse(line: &str) -> Result<List, ParseError> {
    if line.len() > MAX_LINE {
        return Err(ParseError::TooManyWords);
    }
    let tokens = tokenize(line)?;
    let mut list = List::default();
    let mut current = Command::default();
    let mut connector = Connector::Next;
    let mut pending_redir: Option<RedirKind> = None;
    let mut expect_command = true;

    for token in tokens {
        match token {
            Token::Word(word) => {
                if let Some(kind) = pending_redir.take() {
                    if current.redirs.len() == MAX_REDIRS {
                        return Err(ParseError::TooManyRedirs);
                    }
                    current.redirs.push(Redirection { kind, target: word });
                } else {
                    if current.words.len() == MAX_WORDS {
                        return Err(ParseError::TooManyWords);
                    }
                    current.words.push(word);
                }
                expect_command = false;
            }
            Token::Redir(kind) => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                pending_redir = Some(kind);
            }
            Token::Pipe => return Err(ParseError::PipeUnsupported),
            Token::Semi | Token::AndIf | Token::OrIf => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                if current.words.is_empty() {
                    if matches!(token, Token::Semi) && list.items.is_empty() && expect_command {
                        // A leading `;` is a syntax error, not an empty command.
                        return Err(ParseError::UnexpectedOperator(";"));
                    }
                    return Err(ParseError::UnexpectedOperator("operator"));
                }
                list.items.push(AndOr {
                    connector,
                    command: core::mem::take(&mut current),
                });
                connector = match token {
                    Token::AndIf => Connector::And,
                    Token::OrIf => Connector::Or,
                    _ => Connector::Next,
                };
                expect_command = true;
            }
        }
    }
    if pending_redir.is_some() {
        return Err(ParseError::MissingRedirectionTarget);
    }
    if !current.words.is_empty() {
        list.items.push(AndOr {
            connector,
            command: current,
        });
    }
    if list.items.is_empty() {
        return Err(ParseError::Empty);
    }
    Ok(list)
}

/// The variable table used by [`expand`].
pub trait Vars {
    /// Look up `name`; `None` expands to nothing.
    fn get(&self, name: &str) -> Option<String>;
}

/// A variable table backed by a vector of pairs, for tests and simple callers.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Table {
    /// The `NAME=value` pairs in insertion order.
    pub pairs: Vec<(String, String)>,
}

impl Table {
    /// An empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set `name` to `value`.
    pub fn set(&mut self, name: &str, value: &str) {
        if let Some(entry) = self.pairs.iter_mut().find(|entry| entry.0 == name) {
            entry.1 = value.to_string();
            return;
        }
        self.pairs.push((name.to_string(), value.to_string()));
    }

    /// Remove `name`.
    pub fn unset(&mut self, name: &str) {
        self.pairs.retain(|entry| entry.0 != name);
    }
}

impl Vars for Table {
    fn get(&self, name: &str) -> Option<String> {
        self.pairs
            .iter()
            .find(|entry| entry.0 == name)
            .map(|entry| entry.1.clone())
    }
}

/// Expand one word against `vars`; `status` is the value of `$?`.
pub fn expand(word: &Word, vars: &dyn Vars, status: i32) -> String {
    let mut text = String::new();
    for segment in &word.segments {
        match segment {
            Segment::Literal(value) => text.push_str(value),
            Segment::Expandable(value) => text.push_str(&expand_text(value, vars, status)),
        }
    }
    text
}

fn expand_text(text: &str, vars: &dyn Vars, status: i32) -> String {
    let bytes = text.as_bytes();
    let mut output = String::new();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'$' {
            output.push(bytes[index] as char);
            index += 1;
            continue;
        }
        match bytes.get(index + 1) {
            Some(b'?') => {
                output.push_str(&status.to_string());
                index += 2;
            }
            Some(b'{') => {
                let mut end = index + 2;
                while end < bytes.len() && bytes[end] != b'}' {
                    end += 1;
                }
                if end >= bytes.len() {
                    // Unterminated `${`: the `$` stays literal.
                    output.push('$');
                    index += 1;
                    continue;
                }
                let name = core::str::from_utf8(&bytes[index + 2..end]).unwrap_or("");
                if let Some(value) = vars.get(name) {
                    output.push_str(&value);
                }
                index = end + 1;
            }
            Some(byte) if is_name_start(*byte) => {
                let mut end = index + 1;
                while end < bytes.len() && is_name_byte(bytes[end]) {
                    end += 1;
                }
                let name = core::str::from_utf8(&bytes[index + 1..end]).unwrap_or("");
                if let Some(value) = vars.get(name) {
                    output.push_str(&value);
                }
                index = end;
            }
            _ => {
                output.push('$');
                index += 1;
            }
        }
    }
    output
}

fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
