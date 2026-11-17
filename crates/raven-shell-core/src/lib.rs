// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command-line tokenizer, parser and variable expansion for `rsh` (K37).
//!
//! The crate is pure: it turns one input line (or a short script with here-documents) into a
//! [`List`] of pipelines and commands and expands their words against a [`Vars`] table. It
//! performs no I/O and has no clock, so every rule is host-testable.
//!
//! Supported: words (single quotes, double quotes, backslash), `#` comments, the connectors `;`,
//! `&&` and `||`, pipelines (`|`), the redirections `<`, `>`, `>>`, `2>`, `2>&1`, here-documents
//! (`<<`, `<<-`) and command lists. Variable expansion understands `$NAME`, `${NAME}` and `$?`;
//! single quotes and a backslash suppress it.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Longest accepted command line, in bytes.
pub const MAX_LINE: usize = 1024;
/// Largest accepted script, including here-document bodies, in bytes.
pub const MAX_SCRIPT: usize = 64 * 1024;
/// Maximum number of words in one command.
pub const MAX_WORDS: usize = 64;
/// Maximum number of redirections in one command.
pub const MAX_REDIRS: usize = 8;
/// Maximum number of commands in one pipeline.
pub const MAX_PIPELINE: usize = 8;

/// The kind of a redirection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirKind {
    /// `<`: read.
    In,
    /// `>`: truncate and write.
    Out,
    /// `>>`: append.
    Append,
    /// `<<`: here-document.
    HereDoc,
    /// `2>&1`: duplicate a descriptor.
    Dup,
}

/// One redirection and its target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Redirection {
    /// Descriptor being redirected (0, 1 or 2).
    pub fd: u8,
    /// Direction of the redirection.
    pub kind: RedirKind,
    /// Target path or here-document delimiter, unexpanded.
    pub target: Word,
    /// Target descriptor for [`RedirKind::Dup`].
    pub dup: Option<u8>,
    /// Here-document body, without the terminating delimiter line.
    pub body: Option<String>,
    /// `<<-`: strip leading tabs from the body and the delimiter.
    pub strip_tabs: bool,
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

impl Command {
    /// Render the command back to parseable shell source.
    ///
    /// Literal segments are quoted and expandable segments are passed through, so the rendered
    /// text has the same expansion and redirection semantics as the parsed command. Here-document
    /// bodies follow the command line, separated by newlines.
    pub fn render(&self) -> String {
        let mut text = String::new();
        for (index, word) in self.words.iter().enumerate() {
            if index > 0 {
                text.push(' ');
            }
            text.push_str(&render_word(word));
        }
        let mut heredocs = Vec::new();
        for redir in &self.redirs {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&render_redirection(redir));
            if redir.kind == RedirKind::HereDoc {
                heredocs.push(redir);
            }
        }
        for redir in heredocs {
            text.push('\n');
            if let Some(body) = &redir.body {
                text.push_str(body);
            }
            text.push_str(&redir.target.render());
            text.push('\n');
        }
        text
    }
}

/// One pipeline: one or more commands connected by `|`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Pipeline {
    /// The commands in source order.
    pub commands: Vec<Command>,
}

/// How a pipeline is joined to the previous one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connector {
    /// `;` or the start of the list: run unconditionally.
    Next,
    /// `&&`: run when the previous status is zero.
    And,
    /// `||`: run when the previous status is nonzero.
    Or,
}

/// One pipeline with its connector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AndOr {
    /// Connector to the previous pipeline.
    pub connector: Connector,
    /// The pipeline itself.
    pub pipeline: Pipeline,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct List {
    /// The pipelines in source order.
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
    /// A redirection had no target.
    MissingRedirectionTarget,
    /// An operator appeared where a command was expected.
    UnexpectedOperator(&'static str),
    /// More words than [`MAX_WORDS`].
    TooManyWords,
    /// More redirections than [`MAX_REDIRS`].
    TooManyRedirs,
    /// More commands than [`MAX_PIPELINE`].
    TooManyStages,
    /// The input ended before a here-document delimiter.
    IncompleteHereDoc,
    /// A redirection descriptor or target was invalid.
    InvalidRedirection,
}

impl ParseError {
    /// The message the shell prints after `rsh: `.
    pub fn message(&self) -> &'static str {
        match self {
            Self::Empty => "empty command",
            Self::UnterminatedQuote(_) => "unterminated quote",
            Self::TrailingBackslash => "trailing backslash",
            Self::MissingRedirectionTarget => "redirection needs a target",
            Self::UnexpectedOperator(_) => "syntax error",
            Self::TooManyWords => "too many words",
            Self::TooManyRedirs => "too many redirections",
            Self::TooManyStages => "too many pipeline stages",
            Self::IncompleteHereDoc => "here-document is not terminated",
            Self::InvalidRedirection => "invalid redirection",
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
    /// `<`, `>`, `>>`, `<<` or `<<-`.
    Redir {
        /// Descriptor being redirected.
        fd: u8,
        /// Direction of the redirection.
        kind: RedirKind,
        /// `<<-`: strip leading tabs.
        strip_tabs: bool,
    },
    /// `2>&1` or `>&2`.
    Dup {
        /// Descriptor being redirected.
        fd: u8,
        /// Target descriptor.
        target: u8,
    },
}

/// Split one line into tokens; a newline ends the command line and here-document bodies follow it.
pub fn tokenize(line: &str) -> Result<Vec<Token>, ParseError> {
    tokenize_line(line).map(|(tokens, _)| tokens)
}

fn tokenize_line(line: &str) -> Result<(Vec<Token>, usize), ParseError> {
    let bytes = line.as_bytes();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        match byte {
            b' ' | b'\t' | b'\r' => index += 1,
            b'\n' => return Ok((tokens, index)),
            b'#' => {
                while index < bytes.len() && bytes[index] != b'\n' {
                    index += 1;
                }
                return Ok((tokens, index));
            }
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
            b'>' | b'<' => {
                let (token, next) = read_redirection(bytes, index, None)?;
                tokens.push(token);
                index = next;
            }
            b'0'..=b'9' => {
                let (value, digits) = read_digits(bytes, index);
                if digits > 0
                    && digits <= 3
                    && matches!(bytes.get(index + digits), Some(b'>' | b'<'))
                {
                    let fd = u8::try_from(value).map_err(|_| ParseError::InvalidRedirection)?;
                    let (token, next) = read_redirection(bytes, index + digits, Some(fd))?;
                    tokens.push(token);
                    index = next;
                } else {
                    let (word, next) = read_word(bytes, index)?;
                    tokens.push(Token::Word(word));
                    index = next;
                }
            }
            _ => {
                let (word, next) = read_word(bytes, index)?;
                tokens.push(Token::Word(word));
                index = next;
            }
        }
    }
    Ok((tokens, index))
}

/// Read a decimal descriptor prefix at `start`; returns the value and the number of digits.
fn read_digits(bytes: &[u8], start: usize) -> (u32, usize) {
    let mut value = 0u32;
    let mut digits = 0usize;
    while let Some(byte) = bytes.get(start + digits) {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .saturating_mul(10)
            .saturating_add(u32::from(byte - b'0'));
        digits += 1;
    }
    (value, digits)
}

/// Parse a redirection operator at `index`; `lead_fd` comes from a `2>` style prefix.
fn read_redirection(
    bytes: &[u8],
    index: usize,
    lead_fd: Option<u8>,
) -> Result<(Token, usize), ParseError> {
    match bytes.get(index) {
        Some(b'>') => {
            let fd = lead_fd.unwrap_or(1);
            let (kind, mut next) = if bytes.get(index + 1) == Some(&b'>') {
                (RedirKind::Append, index + 2)
            } else {
                (RedirKind::Out, index + 1)
            };
            if bytes.get(next) == Some(&b'&') {
                let (target, digits) = read_digits(bytes, next + 1);
                if digits == 0 || digits > 3 {
                    return Err(ParseError::InvalidRedirection);
                }
                let target = u8::try_from(target).map_err(|_| ParseError::InvalidRedirection)?;
                next += 1 + digits;
                return Ok((Token::Dup { fd, target }, next));
            }
            Ok((
                Token::Redir {
                    fd,
                    kind,
                    strip_tabs: false,
                },
                next,
            ))
        }
        Some(b'<') => {
            let fd = lead_fd.unwrap_or(0);
            if bytes.get(index + 1) == Some(&b'<') {
                let mut next = index + 2;
                let strip_tabs = bytes.get(next) == Some(&b'-');
                if strip_tabs {
                    next += 1;
                }
                return Ok((
                    Token::Redir {
                        fd,
                        kind: RedirKind::HereDoc,
                        strip_tabs,
                    },
                    next,
                ));
            }
            Ok((
                Token::Redir {
                    fd,
                    kind: RedirKind::In,
                    strip_tabs: false,
                },
                index + 1,
            ))
        }
        _ => Err(ParseError::InvalidRedirection),
    }
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

/// Parse one line (or a short script with here-documents) into a command list.
pub fn parse(input: &str) -> Result<List, ParseError> {
    if input.len() > MAX_SCRIPT {
        return Err(ParseError::TooManyWords);
    }
    let (tokens, line_end) = tokenize_line(input)?;
    if line_end > MAX_LINE {
        return Err(ParseError::TooManyWords);
    }
    let mut list = List::default();
    let mut pipeline = Pipeline::default();
    let mut command = Command::default();
    let mut connector = Connector::Next;
    let mut pending_redir: Option<usize> = None;
    let mut expect_command = true;

    for token in tokens {
        match token {
            Token::Word(word) => {
                if let Some(index) = pending_redir.take() {
                    command.redirs[index].target = word;
                } else {
                    if command.words.len() == MAX_WORDS {
                        return Err(ParseError::TooManyWords);
                    }
                    command.words.push(word);
                }
                expect_command = false;
            }
            Token::Redir {
                fd,
                kind,
                strip_tabs,
            } => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                if command.redirs.len() == MAX_REDIRS {
                    return Err(ParseError::TooManyRedirs);
                }
                command.redirs.push(Redirection {
                    fd,
                    kind,
                    target: Word::default(),
                    dup: None,
                    body: None,
                    strip_tabs,
                });
                pending_redir = Some(command.redirs.len() - 1);
                expect_command = false;
            }
            Token::Dup { fd, target } => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                if command.redirs.len() == MAX_REDIRS {
                    return Err(ParseError::TooManyRedirs);
                }
                command.redirs.push(Redirection {
                    fd,
                    kind: RedirKind::Dup,
                    target: Word::default(),
                    dup: Some(target),
                    body: None,
                    strip_tabs: false,
                });
                expect_command = false;
            }
            Token::Pipe => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                if command.words.is_empty() {
                    return Err(ParseError::UnexpectedOperator("|"));
                }
                if pipeline.commands.len() == MAX_PIPELINE {
                    return Err(ParseError::TooManyStages);
                }
                pipeline.commands.push(core::mem::take(&mut command));
                expect_command = true;
            }
            Token::Semi | Token::AndIf | Token::OrIf => {
                if pending_redir.is_some() {
                    return Err(ParseError::MissingRedirectionTarget);
                }
                if command.words.is_empty() {
                    if matches!(token, Token::Semi)
                        && list.items.is_empty()
                        && pipeline.commands.is_empty()
                        && expect_command
                    {
                        // A leading `;` is a syntax error, not an empty command.
                        return Err(ParseError::UnexpectedOperator(";"));
                    }
                    return Err(ParseError::UnexpectedOperator("operator"));
                }
                pipeline.commands.push(core::mem::take(&mut command));
                list.items.push(AndOr {
                    connector,
                    pipeline: core::mem::take(&mut pipeline),
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
    if !command.words.is_empty() {
        pipeline.commands.push(command);
    } else if !pipeline.commands.is_empty() {
        return Err(ParseError::UnexpectedOperator("|"));
    }
    if !pipeline.commands.is_empty() {
        list.items.push(AndOr {
            connector,
            pipeline,
        });
    }
    if list.items.is_empty() {
        return Err(ParseError::Empty);
    }

    let heredocs = here_doc_indices(&list);
    let mut body_pos = line_end.saturating_add(1);
    for (item, stage, redir) in heredocs {
        let (delimiter, strip_tabs) = {
            let entry = &list.items[item].pipeline.commands[stage].redirs[redir];
            (entry.target.render(), entry.strip_tabs)
        };
        let (body, next) = collect_here_doc(input, body_pos, &delimiter, strip_tabs)?;
        list.items[item].pipeline.commands[stage].redirs[redir].body = Some(body);
        body_pos = next;
    }
    if body_pos < input.len() {
        let rest = &input[body_pos..];
        if !rest.trim().is_empty() {
            let mut more = parse(rest)?;
            list.items.append(&mut more.items);
        }
    }
    Ok(list)
}

/// Collect the indices of every here-document redirection in source order.
fn here_doc_indices(list: &List) -> Vec<(usize, usize, usize)> {
    let mut indices = Vec::new();
    for (item_index, item) in list.items.iter().enumerate() {
        for (stage_index, command) in item.pipeline.commands.iter().enumerate() {
            for (redir_index, redir) in command.redirs.iter().enumerate() {
                if redir.kind == RedirKind::HereDoc {
                    indices.push((item_index, stage_index, redir_index));
                }
            }
        }
    }
    indices
}

/// Read one here-document body up to its delimiter line.
fn collect_here_doc(
    input: &str,
    start: usize,
    delimiter: &str,
    strip_tabs: bool,
) -> Result<(String, usize), ParseError> {
    let mut body = String::new();
    let mut position = start;
    while position <= input.len() {
        if position == input.len() {
            return Err(ParseError::IncompleteHereDoc);
        }
        let line_end = input[position..]
            .find('\n')
            .map_or(input.len(), |offset| position + offset);
        let source_line = &input[position..line_end];
        let line = if strip_tabs {
            source_line.trim_start_matches('\t')
        } else {
            source_line
        };
        if line == delimiter {
            let next = if line_end < input.len() {
                line_end + 1
            } else {
                line_end
            };
            return Ok((body, next));
        }
        if strip_tabs {
            body.push_str(line);
        } else {
            body.push_str(source_line);
        }
        body.push('\n');
        if line_end == input.len() {
            return Err(ParseError::IncompleteHereDoc);
        }
        position = line_end + 1;
    }
    Err(ParseError::IncompleteHereDoc)
}

/// Render a word with literal segments quoted and expandable segments passed through.
fn render_word(word: &Word) -> String {
    if word.segments.is_empty() {
        return "''".to_string();
    }
    let mut text = String::new();
    for segment in &word.segments {
        match segment {
            Segment::Literal(value) => text.push_str(&quote(value)),
            Segment::Expandable(value) => text.push_str(value),
        }
    }
    text
}

/// Render one redirection in parseable source form.
fn render_redirection(redir: &Redirection) -> String {
    match redir.kind {
        RedirKind::In => format!(
            "{}{} {}",
            fd_prefix(redir.fd, 0),
            "<",
            render_word(&redir.target)
        ),
        RedirKind::Out => format!(
            "{}{} {}",
            fd_prefix(redir.fd, 1),
            ">",
            render_word(&redir.target)
        ),
        RedirKind::Append => format!(
            "{}{} {}",
            fd_prefix(redir.fd, 1),
            ">>",
            render_word(&redir.target)
        ),
        RedirKind::HereDoc => format!(
            "{}{}{} {}",
            fd_prefix(redir.fd, 0),
            "<<",
            if redir.strip_tabs { "-" } else { "" },
            quote(&redir.target.render())
        ),
        RedirKind::Dup => format!(
            "{}{}{}",
            fd_prefix(redir.fd, 1),
            ">&",
            redir.dup.unwrap_or(1)
        ),
    }
}

/// The descriptor prefix (`2` for fd 2, empty for the default fd).
fn fd_prefix(fd: u8, default: u8) -> String {
    if fd == default {
        String::new()
    } else {
        fd.to_string()
    }
}

/// Single-quote text for the shell.
fn quote(text: &str) -> String {
    if text.is_empty() {
        return "''".to_string();
    }
    let mut quoted = String::from("'");
    for byte in text.chars() {
        if byte == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(byte);
        }
    }
    quoted.push('\'');
    quoted
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
