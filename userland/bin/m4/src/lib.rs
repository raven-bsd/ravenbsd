// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded POSIX `m4` macro processor.
//!
//! The processor implements the macro expansion core: quoting, comments, the standard built-ins
//! listed in [`Processor::new`], `-D`/`-U`/`-I`, and file includes through [`FileProvider`].
//! `syscmd` and `esyscmd` are deliberately unsupported and return [`M4Error::Unsupported`].

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Default beginning quote.
const DEFAULT_QUOTE_BEGIN: &str = "`";
/// Default ending quote.
const DEFAULT_QUOTE_END: &str = "'";
/// Default comment beginning.
const DEFAULT_COMMENT_BEGIN: &str = "#";
/// Default comment ending.
const DEFAULT_COMMENT_END: &str = "\n";
/// Maximum nested expansion depth.
const MAX_DEPTH: usize = 128;

/// Why a script was rejected or could not be processed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum M4Error {
    /// The invocation or the macro syntax is invalid.
    Usage,
    /// A built-in is intentionally not implemented.
    Unsupported(&'static str),
    /// An included file could not be opened.
    Include(String),
    /// The macro recursion limit was exceeded.
    Recursion,
}

/// One command-line definition action, kept in command-line order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitAction {
    /// Define (or replace) a macro with a literal replacement.
    Define(String, String),
    /// Remove every definition of a macro.
    Undefine(String),
}

/// Options applied before the first input byte is processed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// `-D` and `-U` actions in command-line order.
    pub actions: Vec<InitAction>,
    /// `-I` include search directories in command-line order.
    pub include_dirs: Vec<String>,
}

/// A parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invocation {
    /// Initial definitions and include directories.
    pub options: Options,
    /// Input file operands; `-` means standard input.
    pub operands: Vec<String>,
}

/// Read files for `include`.
pub trait FileProvider {
    /// Return the text of `path`, or `None` when it cannot be opened.
    fn read_file(&mut self, path: &str) -> Option<String>;
}

/// Parse `-D`, `-U`, `-I`, `--` and file operands.
///
/// The parser accepts both attached (`-Dname=value`) and separate (`-D name=value`) spellings.
pub fn parse_args<I, S>(arguments: I) -> Result<Invocation, M4Error>
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
            invocation.operands.push(argument.clone());
            index = index.saturating_add(1);
            continue;
        }
        if argument == "--" {
            end_of_options = true;
            index = index.saturating_add(1);
            continue;
        }
        if argument == "-" {
            invocation.operands.push(argument.clone());
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = argument.strip_prefix("-D") {
            let value = if value.is_empty() {
                index = index.saturating_add(1);
                arguments.get(index).ok_or(M4Error::Usage)?.as_str()
            } else {
                value
            };
            invocation.options.actions.push(parse_define(value)?);
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = argument.strip_prefix("-U") {
            let value = if value.is_empty() {
                index = index.saturating_add(1);
                arguments.get(index).ok_or(M4Error::Usage)?.as_str()
            } else {
                value
            };
            if !valid_name(value) {
                return Err(M4Error::Usage);
            }
            invocation
                .options
                .actions
                .push(InitAction::Undefine(value.to_string()));
            index = index.saturating_add(1);
            continue;
        }
        if let Some(value) = argument.strip_prefix("-I") {
            let value = if value.is_empty() {
                index = index.saturating_add(1);
                arguments.get(index).ok_or(M4Error::Usage)?.as_str()
            } else {
                value
            };
            if value.is_empty() {
                return Err(M4Error::Usage);
            }
            invocation.options.include_dirs.push(value.to_string());
            index = index.saturating_add(1);
            continue;
        }
        if argument.starts_with('-') && argument.len() > 1 {
            return Err(M4Error::Usage);
        }
        invocation.operands.push(argument.clone());
        index = index.saturating_add(1);
    }
    Ok(invocation)
}

fn parse_define(value: &str) -> Result<InitAction, M4Error> {
    let (name, replacement) = value.split_once('=').unwrap_or((value, ""));
    if !valid_name(name) {
        return Err(M4Error::Usage);
    }
    Ok(InitAction::Define(
        name.to_string(),
        replacement.to_string(),
    ))
}

fn valid_name(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
}

/// One macro definition on a `pushdef` stack.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Definition {
    Builtin(Builtin),
    User(String),
}

/// The bounded built-in set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Builtin {
    Define,
    Undefine,
    Defn,
    Pushdef,
    Popdef,
    Dnl,
    Include,
    Changequote,
    Changecom,
    Ifdef,
    Ifelse,
    Incr,
    Decr,
    Eval,
    Substr,
    Index,
    Translit,
    Divert,
    Undivert,
    Syscmd,
    Esyscmd,
}

/// What the scanner should do with a macro result.
enum Invoke {
    /// Rescan the expansion in the current mode.
    Expand(String),
    /// Scan the contents of an included file in the current sink.
    File(String),
    /// Discard input through the next newline.
    SkipLine,
    /// The built-in already performed its side effect.
    Done,
}

/// Where expanded text goes: the output stream or an argument buffer.
enum SinkState {
    Emit,
    Arguments {
        arguments: Vec<String>,
        current: String,
        depth: usize,
        at_start: bool,
    },
}

struct ScanResult {
    position: usize,
    closed: bool,
    skip_line: bool,
}

/// A macro-processing session. Definitions and diversions persist across [`Processor::process`].
pub struct Processor<'a> {
    provider: &'a mut dyn FileProvider,
    macros: BTreeMap<String, Vec<Definition>>,
    quote_begin: Vec<char>,
    quote_end: Vec<char>,
    comment_begin: Vec<char>,
    comment_end: Vec<char>,
    include_dirs: Vec<String>,
    diversion: i32,
    diversions: BTreeMap<i32, String>,
    stdout: String,
}

impl<'a> Processor<'a> {
    /// Create a processor with the standard built-ins and apply `options`.
    ///
    /// The built-in set is `define`, `undefine`, `defn`, `pushdef`, `popdef`, `dnl`, `include`,
    /// `changequote`, `changecom`, `ifdef`, `ifelse`, `incr`, `decr`, `eval`, `substr`, `index`,
    /// `translit`, `divert` and `undivert`.
    pub fn new(options: &Options, provider: &'a mut dyn FileProvider) -> Self {
        let mut processor = Self {
            provider,
            macros: BTreeMap::new(),
            quote_begin: characters(DEFAULT_QUOTE_BEGIN),
            quote_end: characters(DEFAULT_QUOTE_END),
            comment_begin: characters(DEFAULT_COMMENT_BEGIN),
            comment_end: characters(DEFAULT_COMMENT_END),
            include_dirs: options.include_dirs.clone(),
            diversion: 0,
            diversions: BTreeMap::new(),
            stdout: String::new(),
        };
        processor.install_builtins();
        for action in &options.actions {
            match action {
                InitAction::Define(name, value) => {
                    processor.define_replacement(name, value.clone());
                }
                InitAction::Undefine(name) => processor.undefine_all(name),
            }
        }
        processor
    }

    /// Process one input source, as one file operand would be processed.
    pub fn process(&mut self, input: &str) -> Result<(), M4Error> {
        let characters: Vec<char> = input.chars().collect();
        let mut sink = SinkState::Emit;
        let result = self.scan_chars(&characters, 0, 0, &mut sink)?;
        if result.closed {
            return Err(M4Error::Usage);
        }
        Ok(())
    }

    /// Return the final output, appending any remaining diversions in numerical order.
    pub fn finish(mut self) -> String {
        let numbers: Vec<i32> = self.diversions.keys().copied().collect();
        for number in numbers {
            if let Some(text) = self.diversions.remove(&number) {
                self.stdout.push_str(&text);
            }
        }
        self.stdout
    }

    fn install_builtins(&mut self) {
        const BUILTINS: [(&str, Builtin); 21] = [
            ("define", Builtin::Define),
            ("undefine", Builtin::Undefine),
            ("defn", Builtin::Defn),
            ("pushdef", Builtin::Pushdef),
            ("popdef", Builtin::Popdef),
            ("dnl", Builtin::Dnl),
            ("include", Builtin::Include),
            ("changequote", Builtin::Changequote),
            ("changecom", Builtin::Changecom),
            ("ifdef", Builtin::Ifdef),
            ("ifelse", Builtin::Ifelse),
            ("incr", Builtin::Incr),
            ("decr", Builtin::Decr),
            ("eval", Builtin::Eval),
            ("substr", Builtin::Substr),
            ("index", Builtin::Index),
            ("translit", Builtin::Translit),
            ("divert", Builtin::Divert),
            ("undivert", Builtin::Undivert),
            ("syscmd", Builtin::Syscmd),
            ("esyscmd", Builtin::Esyscmd),
        ];
        for (name, builtin) in BUILTINS {
            self.macros
                .insert(name.to_string(), alloc::vec![Definition::Builtin(builtin)]);
        }
    }

    fn scan_str(
        &mut self,
        input: &str,
        depth: usize,
        sink: &mut SinkState,
    ) -> Result<ScanResult, M4Error> {
        let characters: Vec<char> = input.chars().collect();
        self.scan_chars(&characters, 0, depth, sink)
    }

    fn scan_chars(
        &mut self,
        characters: &[char],
        start: usize,
        depth: usize,
        sink: &mut SinkState,
    ) -> Result<ScanResult, M4Error> {
        if depth > MAX_DEPTH {
            return Err(M4Error::Recursion);
        }
        let mut position = start;
        while position < characters.len() {
            if self.quotes_enabled() && starts_with_at(characters, position, &self.quote_begin) {
                position = position.saturating_add(self.quote_begin.len());
                let (text, next) = self.consume_quoted(characters, position)?;
                self.sink_quoted(sink, &text);
                position = next;
                continue;
            }
            if self.comments_enabled() && starts_with_at(characters, position, &self.comment_begin)
            {
                let start_comment = position;
                position = position.saturating_add(self.comment_begin.len());
                while position < characters.len()
                    && !starts_with_at(characters, position, &self.comment_end)
                {
                    position = position.saturating_add(1);
                }
                if position < characters.len() {
                    position = position.saturating_add(self.comment_end.len());
                }
                let text: String = characters[start_comment..position].iter().collect();
                self.sink_quoted(sink, &text);
                continue;
            }
            let character = characters[position];
            if is_name_start(character) {
                let start_name = position;
                position = position.saturating_add(1);
                while position < characters.len() && is_name_char(characters[position]) {
                    position = position.saturating_add(1);
                }
                let name: String = characters[start_name..position].iter().collect();
                if !self.is_defined(&name) {
                    self.sink_text(sink, &name);
                    continue;
                }
                let mut arguments = Vec::new();
                if position < characters.len() && characters[position] == '(' {
                    let (collected, next) =
                        self.collect_arguments(characters, position.saturating_add(1), depth)?;
                    arguments = collected;
                    position = next;
                }
                match self.invoke(&name, &arguments)? {
                    Invoke::Expand(text) | Invoke::File(text) => {
                        let result = self.scan_str(&text, depth.saturating_add(1), sink)?;
                        if result.closed {
                            return Ok(ScanResult {
                                position,
                                closed: true,
                                skip_line: false,
                            });
                        }
                        if result.skip_line && !skip_to_newline(characters, &mut position) {
                            return Ok(ScanResult {
                                position,
                                closed: false,
                                skip_line: true,
                            });
                        }
                    }
                    Invoke::SkipLine => {
                        if !skip_to_newline(characters, &mut position) {
                            return Ok(ScanResult {
                                position,
                                closed: false,
                                skip_line: true,
                            });
                        }
                    }
                    Invoke::Done => {}
                }
                continue;
            }
            match character {
                '(' => self.sink_open(sink),
                ')' => {
                    let closed = self.sink_close(sink);
                    if closed {
                        return Ok(ScanResult {
                            position: position.saturating_add(1),
                            closed: true,
                            skip_line: false,
                        });
                    }
                }
                ',' => self.sink_comma(sink),
                character => self.sink_character(sink, character),
            }
            position = position.saturating_add(1);
        }
        Ok(ScanResult {
            position,
            closed: false,
            skip_line: false,
        })
    }

    fn collect_arguments(
        &mut self,
        characters: &[char],
        start: usize,
        depth: usize,
    ) -> Result<(Vec<String>, usize), M4Error> {
        let mut sink = SinkState::Arguments {
            arguments: Vec::new(),
            current: String::new(),
            depth: 1,
            at_start: true,
        };
        let result = self.scan_chars(characters, start, depth.saturating_add(1), &mut sink)?;
        if !result.closed {
            return Err(M4Error::Usage);
        }
        let SinkState::Arguments {
            mut arguments,
            current,
            ..
        } = sink
        else {
            return Err(M4Error::Usage);
        };
        arguments.push(current);
        Ok((arguments, result.position))
    }

    fn sink_text(&mut self, sink: &mut SinkState, text: &str) {
        match sink {
            SinkState::Emit => self.emit(text),
            SinkState::Arguments {
                current, at_start, ..
            } => {
                if *at_start {
                    let trimmed = text.trim_start_matches(|character: char| {
                        matches!(character, ' ' | '\t' | '\n' | '\r')
                    });
                    if trimmed.is_empty() {
                        return;
                    }
                    current.push_str(trimmed);
                    *at_start = false;
                } else {
                    current.push_str(text);
                }
            }
        }
    }

    fn sink_quoted(&mut self, sink: &mut SinkState, text: &str) {
        match sink {
            SinkState::Emit => self.emit(text),
            SinkState::Arguments {
                current, at_start, ..
            } => {
                current.push_str(text);
                *at_start = false;
            }
        }
    }

    fn sink_character(&mut self, sink: &mut SinkState, character: char) {
        match sink {
            SinkState::Emit => self.emit_character(character),
            SinkState::Arguments {
                current, at_start, ..
            } => {
                if *at_start && matches!(character, ' ' | '\t' | '\n' | '\r') {
                    return;
                }
                current.push(character);
                *at_start = false;
            }
        }
    }

    fn sink_open(&mut self, sink: &mut SinkState) {
        match sink {
            SinkState::Emit => self.emit_character('('),
            SinkState::Arguments {
                current,
                depth,
                at_start,
                ..
            } => {
                *depth = (*depth).saturating_add(1);
                current.push('(');
                *at_start = false;
            }
        }
    }

    fn sink_close(&mut self, sink: &mut SinkState) -> bool {
        match sink {
            SinkState::Emit => {
                self.emit_character(')');
                false
            }
            SinkState::Arguments { current, depth, .. } => {
                *depth = (*depth).saturating_sub(1);
                if *depth == 0 {
                    true
                } else {
                    current.push(')');
                    false
                }
            }
        }
    }

    fn sink_comma(&mut self, sink: &mut SinkState) {
        match sink {
            SinkState::Emit => self.emit_character(','),
            SinkState::Arguments {
                arguments,
                current,
                depth,
                at_start,
            } => {
                if *depth == 1 {
                    arguments.push(core::mem::take(current));
                    *at_start = true;
                } else {
                    current.push(',');
                    *at_start = false;
                }
            }
        }
    }

    fn consume_quoted(
        &self,
        characters: &[char],
        mut position: usize,
    ) -> Result<(String, usize), M4Error> {
        let start = position;
        if self.quote_begin == self.quote_end {
            while position < characters.len()
                && !starts_with_at(characters, position, &self.quote_end)
            {
                position = position.saturating_add(1);
            }
            if position >= characters.len() {
                return Err(M4Error::Usage);
            }
            let text: String = characters[start..position].iter().collect();
            return Ok((text, position.saturating_add(self.quote_end.len())));
        }
        let mut depth = 1usize;
        while position < characters.len() {
            if starts_with_at(characters, position, &self.quote_begin) {
                depth = depth.saturating_add(1);
                position = position.saturating_add(self.quote_begin.len());
                continue;
            }
            if starts_with_at(characters, position, &self.quote_end) {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let text: String = characters[start..position].iter().collect();
                    return Ok((text, position.saturating_add(self.quote_end.len())));
                }
                position = position.saturating_add(self.quote_end.len());
                continue;
            }
            position = position.saturating_add(1);
        }
        Err(M4Error::Usage)
    }

    fn invoke(&mut self, name: &str, arguments: &[String]) -> Result<Invoke, M4Error> {
        let definition = self.top_definition(name).cloned().ok_or(M4Error::Usage)?;
        match definition {
            Definition::User(replacement) => Ok(Invoke::Expand(self.substitute(
                &replacement,
                name,
                arguments,
            ))),
            Definition::Builtin(builtin) => self.invoke_builtin(builtin, arguments),
        }
    }

    fn invoke_builtin(
        &mut self,
        builtin: Builtin,
        arguments: &[String],
    ) -> Result<Invoke, M4Error> {
        match builtin {
            Builtin::Define => {
                let (name, replacement) = definition_arguments(arguments)?;
                self.define_replacement(name, replacement.to_string());
                Ok(Invoke::Done)
            }
            Builtin::Pushdef => {
                let (name, replacement) = definition_arguments(arguments)?;
                self.macros
                    .entry(name.to_string())
                    .or_default()
                    .push(Definition::User(replacement.to_string()));
                Ok(Invoke::Done)
            }
            Builtin::Undefine => {
                for name in arguments {
                    if valid_name(name) {
                        self.undefine_all(name);
                    }
                }
                Ok(Invoke::Done)
            }
            Builtin::Popdef => {
                for name in arguments {
                    self.pop_definition(name);
                }
                Ok(Invoke::Done)
            }
            Builtin::Defn => {
                let mut result = String::new();
                for name in arguments {
                    if let Some(Definition::User(replacement)) = self.top_definition(name) {
                        result.push_str(&self.quote(replacement));
                    }
                }
                Ok(Invoke::Expand(result))
            }
            Builtin::Dnl => Ok(Invoke::SkipLine),
            Builtin::Include => {
                let name = arguments.first().ok_or(M4Error::Usage)?;
                let contents = self
                    .find_include(name)
                    .ok_or_else(|| M4Error::Include(name.clone()))?;
                Ok(Invoke::File(contents))
            }
            Builtin::Changequote => {
                match arguments.len() {
                    0 => {
                        self.quote_begin = characters(DEFAULT_QUOTE_BEGIN);
                        self.quote_end = characters(DEFAULT_QUOTE_END);
                    }
                    1 => {
                        self.quote_begin = characters(&arguments[0]);
                        self.quote_end = characters(DEFAULT_QUOTE_END);
                    }
                    _ => {
                        self.quote_begin = characters(&arguments[0]);
                        self.quote_end = if arguments[1].is_empty() {
                            characters(DEFAULT_QUOTE_END)
                        } else {
                            characters(&arguments[1])
                        };
                    }
                }
                Ok(Invoke::Done)
            }
            Builtin::Changecom => {
                match arguments.len() {
                    0 => {
                        self.comment_begin.clear();
                        self.comment_end.clear();
                    }
                    1 => {
                        self.comment_begin = characters(&arguments[0]);
                        self.comment_end = characters(DEFAULT_COMMENT_END);
                    }
                    _ => {
                        self.comment_begin = characters(&arguments[0]);
                        self.comment_end = if arguments[1].is_empty() {
                            characters(DEFAULT_COMMENT_END)
                        } else {
                            characters(&arguments[1])
                        };
                    }
                }
                Ok(Invoke::Done)
            }
            Builtin::Ifdef => {
                let name = arguments.first().ok_or(M4Error::Usage)?;
                let chosen = if self.is_defined(name) {
                    arguments.get(1)
                } else {
                    arguments.get(2)
                };
                Ok(Invoke::Expand(chosen.cloned().unwrap_or_default()))
            }
            Builtin::Ifelse => Ok(Invoke::Expand(ifelse(arguments))),
            Builtin::Incr => {
                let value = parse_integer(arguments.first())?;
                let value = value.checked_add(1).ok_or(M4Error::Usage)?;
                Ok(Invoke::Expand(format!("{value}")))
            }
            Builtin::Decr => {
                let value = parse_integer(arguments.first())?;
                let value = value.checked_sub(1).ok_or(M4Error::Usage)?;
                Ok(Invoke::Expand(format!("{value}")))
            }
            Builtin::Eval => {
                let expression = arguments.first().ok_or(M4Error::Usage)?;
                let value = evaluate(expression)?;
                Ok(Invoke::Expand(format!("{value}")))
            }
            Builtin::Substr => Ok(Invoke::Expand(substring(arguments)?)),
            Builtin::Index => Ok(Invoke::Expand(index_of(arguments)?)),
            Builtin::Translit => Ok(Invoke::Expand(transliterate(arguments)?)),
            Builtin::Divert => {
                let number = match arguments.first() {
                    Some(value) => parse_i32(value)?,
                    None => 0,
                };
                self.diversion = number;
                Ok(Invoke::Done)
            }
            Builtin::Undivert => {
                self.undivert(arguments)?;
                Ok(Invoke::Done)
            }
            Builtin::Syscmd => Err(M4Error::Unsupported("syscmd")),
            Builtin::Esyscmd => Err(M4Error::Unsupported("esyscmd")),
        }
    }

    fn undivert(&mut self, arguments: &[String]) -> Result<(), M4Error> {
        if arguments.is_empty() {
            let numbers: Vec<i32> = self.diversions.keys().copied().collect();
            for number in numbers {
                if let Some(text) = self.diversions.remove(&number) {
                    self.emit(&text);
                }
            }
            return Ok(());
        }
        for argument in arguments {
            if argument.trim().is_empty() {
                continue;
            }
            let number = parse_i32(argument)?;
            if number > 0
                && let Some(text) = self.diversions.remove(&number)
            {
                self.emit(&text);
            }
        }
        Ok(())
    }

    fn find_include(&mut self, name: &str) -> Option<String> {
        if name.is_empty() {
            return None;
        }
        if name.starts_with('/') || name.contains(':') {
            return self.provider.read_file(name);
        }
        let directories = self.include_dirs.clone();
        for directory in directories {
            if directory.is_empty() {
                continue;
            }
            let candidate = if directory.ends_with('/') {
                format!("{directory}{name}")
            } else {
                format!("{directory}/{name}")
            };
            if let Some(contents) = self.provider.read_file(&candidate) {
                return Some(contents);
            }
        }
        self.provider.read_file(name)
    }

    fn substitute(&self, replacement: &str, name: &str, arguments: &[String]) -> String {
        let characters: Vec<char> = replacement.chars().collect();
        let mut result = String::new();
        let mut position = 0usize;
        while position < characters.len() {
            if characters[position] != '$' {
                result.push(characters[position]);
                position = position.saturating_add(1);
                continue;
            }
            position = position.saturating_add(1);
            if position >= characters.len() {
                result.push('$');
                break;
            }
            match characters[position] {
                '0' => result.push_str(name),
                '1'..='9' => {
                    let index = usize::from(characters[position] as u8 - b'0');
                    if let Some(argument) = arguments.get(index.saturating_sub(1)) {
                        result.push_str(argument);
                    }
                }
                '#' => result.push_str(&arguments.len().to_string()),
                '*' => result.push_str(&arguments.join(",")),
                '@' => {
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 {
                            result.push(',');
                        }
                        result.push_str(&self.quote(argument));
                    }
                }
                '$' => result.push('$'),
                other => {
                    result.push('$');
                    result.push(other);
                }
            }
            position = position.saturating_add(1);
        }
        result
    }

    fn quote(&self, text: &str) -> String {
        if !self.quotes_enabled() {
            return text.to_string();
        }
        let mut result = String::new();
        result.extend(self.quote_begin.iter());
        result.push_str(text);
        result.extend(self.quote_end.iter());
        result
    }

    fn top_definition(&self, name: &str) -> Option<&Definition> {
        self.macros.get(name).and_then(|stack| stack.last())
    }

    fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    fn define_replacement(&mut self, name: &str, replacement: String) {
        match self.macros.get_mut(name) {
            Some(stack) => match stack.last_mut() {
                Some(top) => *top = Definition::User(replacement),
                None => stack.push(Definition::User(replacement)),
            },
            None => {
                self.macros
                    .insert(name.to_string(), alloc::vec![Definition::User(replacement)]);
            }
        }
    }

    fn undefine_all(&mut self, name: &str) {
        self.macros.remove(name);
    }

    fn pop_definition(&mut self, name: &str) {
        let remove = match self.macros.get_mut(name) {
            Some(stack) => {
                stack.pop();
                stack.is_empty()
            }
            None => false,
        };
        if remove {
            self.macros.remove(name);
        }
    }

    fn quotes_enabled(&self) -> bool {
        !self.quote_begin.is_empty() && !self.quote_end.is_empty()
    }

    fn comments_enabled(&self) -> bool {
        !self.comment_begin.is_empty() && !self.comment_end.is_empty()
    }

    fn emit(&mut self, text: &str) {
        match self.diversion {
            0 => self.stdout.push_str(text),
            number if number > 0 => self.diversions.entry(number).or_default().push_str(text),
            _ => {}
        }
    }

    fn emit_character(&mut self, character: char) {
        match self.diversion {
            0 => self.stdout.push(character),
            number if number > 0 => self.diversions.entry(number).or_default().push(character),
            _ => {}
        }
    }
}

/// Run one input source through a fresh processor.
pub fn run(
    input: &str,
    options: &Options,
    provider: &mut dyn FileProvider,
) -> Result<String, M4Error> {
    let mut processor = Processor::new(options, provider);
    processor.process(input)?;
    Ok(processor.finish())
}

fn characters(text: &str) -> Vec<char> {
    text.chars().collect()
}

fn skip_to_newline(characters: &[char], position: &mut usize) -> bool {
    while *position < characters.len() {
        let newline = characters[*position] == '\n';
        *position = (*position).saturating_add(1);
        if newline {
            return true;
        }
    }
    false
}

fn starts_with_at(characters: &[char], position: usize, pattern: &[char]) -> bool {
    !pattern.is_empty()
        && position.saturating_add(pattern.len()) <= characters.len()
        && characters.get(position..position.saturating_add(pattern.len())) == Some(pattern)
}

fn is_name_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_name_char(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

fn definition_arguments(arguments: &[String]) -> Result<(&str, &str), M4Error> {
    let name = arguments.first().ok_or(M4Error::Usage)?;
    if name.is_empty() {
        return Err(M4Error::Usage);
    }
    let replacement = arguments.get(1).map(String::as_str).unwrap_or("");
    Ok((name, replacement))
}

fn ifelse(arguments: &[String]) -> String {
    let mut index = 0usize;
    while index.saturating_add(2) < arguments.len() {
        if arguments[index] == arguments[index.saturating_add(1)] {
            return arguments
                .get(index.saturating_add(2))
                .cloned()
                .unwrap_or_default();
        }
        index = index.saturating_add(3);
        if index.saturating_add(1) == arguments.len() {
            return arguments.get(index).cloned().unwrap_or_default();
        }
    }
    String::new()
}

fn parse_integer(value: Option<&String>) -> Result<i64, M4Error> {
    let value = value.ok_or(M4Error::Usage)?;
    value.trim().parse::<i64>().map_err(|_| M4Error::Usage)
}

fn parse_i32(value: &str) -> Result<i32, M4Error> {
    value.trim().parse::<i32>().map_err(|_| M4Error::Usage)
}

fn substring(arguments: &[String]) -> Result<String, M4Error> {
    let text = arguments.first().ok_or(M4Error::Usage)?;
    let start = parse_integer(arguments.get(1))?;
    if start < 0 {
        return Ok(String::new());
    }
    let start = usize::try_from(start).unwrap_or(usize::MAX);
    let length = match arguments.get(2) {
        Some(value) => parse_integer(Some(value))?,
        None => i64::MAX,
    };
    if length <= 0 {
        return Ok(String::new());
    }
    let length = usize::try_from(length).unwrap_or(usize::MAX);
    Ok(text.chars().skip(start).take(length).collect())
}

fn index_of(arguments: &[String]) -> Result<String, M4Error> {
    let text = arguments.first().ok_or(M4Error::Usage)?;
    let needle = arguments.get(1).ok_or(M4Error::Usage)?;
    let text: Vec<char> = text.chars().collect();
    let needle: Vec<char> = needle.chars().collect();
    if needle.is_empty() {
        return Ok("0".to_string());
    }
    for position in 0..=text.len().saturating_sub(needle.len()) {
        if text.get(position..position.saturating_add(needle.len())) == Some(needle.as_slice()) {
            return Ok(format!("{position}"));
        }
    }
    Ok("-1".to_string())
}

fn transliterate(arguments: &[String]) -> Result<String, M4Error> {
    let text = arguments.first().ok_or(M4Error::Usage)?;
    let from: Vec<char> = arguments
        .get(1)
        .map(|value| value.chars().collect())
        .unwrap_or_default();
    let to: Vec<char> = arguments
        .get(2)
        .map(|value| value.chars().collect())
        .unwrap_or_default();
    let mut result = String::new();
    for character in text.chars() {
        match from.iter().position(|candidate| *candidate == character) {
            Some(position) => {
                if let Some(replacement) = to.get(position) {
                    result.push(*replacement);
                }
            }
            None => result.push(character),
        }
    }
    Ok(result)
}

fn evaluate(expression: &str) -> Result<i64, M4Error> {
    let mut parser = Evaluator {
        characters: expression.chars().collect(),
        position: 0,
    };
    let value = parser.ternary()?;
    parser.skip_space();
    if parser.position == parser.characters.len() {
        Ok(value)
    } else {
        Err(M4Error::Usage)
    }
}

struct Evaluator {
    characters: Vec<char>,
    position: usize,
}

impl Evaluator {
    fn ternary(&mut self) -> Result<i64, M4Error> {
        let condition = self.logical_or()?;
        if self.eat("?") {
            let yes = self.ternary()?;
            self.expect(":")?;
            let no = self.ternary()?;
            return Ok(if condition != 0 { yes } else { no });
        }
        Ok(condition)
    }

    fn logical_or(&mut self) -> Result<i64, M4Error> {
        let mut value = self.logical_and()?;
        while self.eat("||") {
            let right = self.logical_and()?;
            value = i64::from(value != 0 || right != 0);
        }
        Ok(value)
    }

    fn logical_and(&mut self) -> Result<i64, M4Error> {
        let mut value = self.bit_or()?;
        while self.eat("&&") {
            let right = self.bit_or()?;
            value = i64::from(value != 0 && right != 0);
        }
        Ok(value)
    }

    fn bit_or(&mut self) -> Result<i64, M4Error> {
        let mut value = self.bit_xor()?;
        while self.eat_single('|', '|') {
            let right = self.bit_xor()?;
            value |= right;
        }
        Ok(value)
    }

    fn bit_xor(&mut self) -> Result<i64, M4Error> {
        let mut value = self.bit_and()?;
        while self.eat_single('^', '^') {
            let right = self.bit_and()?;
            value ^= right;
        }
        Ok(value)
    }

    fn bit_and(&mut self) -> Result<i64, M4Error> {
        let mut value = self.equality()?;
        while self.eat_single('&', '&') {
            let right = self.equality()?;
            value &= right;
        }
        Ok(value)
    }

    fn equality(&mut self) -> Result<i64, M4Error> {
        let mut value = self.relational()?;
        loop {
            if self.eat("==") {
                value = i64::from(value == self.relational()?);
            } else if self.eat("!=") {
                value = i64::from(value != self.relational()?);
            } else {
                return Ok(value);
            }
        }
    }

    fn relational(&mut self) -> Result<i64, M4Error> {
        let mut value = self.shift()?;
        loop {
            if self.eat("<=") {
                value = i64::from(value <= self.shift()?);
            } else if self.eat(">=") {
                value = i64::from(value >= self.shift()?);
            } else if self.eat_single('<', '<') {
                value = i64::from(value < self.shift()?);
            } else if self.eat_single('>', '>') {
                value = i64::from(value > self.shift()?);
            } else {
                return Ok(value);
            }
        }
    }

    fn shift(&mut self) -> Result<i64, M4Error> {
        let mut value = self.additive()?;
        loop {
            if self.eat("<<") {
                let amount = self.additive()?;
                let amount = u32::try_from(amount).map_err(|_| M4Error::Usage)?;
                value = value.checked_shl(amount).ok_or(M4Error::Usage)?;
            } else if self.eat(">>") {
                let amount = self.additive()?;
                let amount = u32::try_from(amount).map_err(|_| M4Error::Usage)?;
                value = value.checked_shr(amount).ok_or(M4Error::Usage)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn additive(&mut self) -> Result<i64, M4Error> {
        let mut value = self.multiplicative()?;
        loop {
            if self.eat("+") {
                value = value
                    .checked_add(self.multiplicative()?)
                    .ok_or(M4Error::Usage)?;
            } else if self.eat("-") {
                value = value
                    .checked_sub(self.multiplicative()?)
                    .ok_or(M4Error::Usage)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn multiplicative(&mut self) -> Result<i64, M4Error> {
        let mut value = self.unary()?;
        loop {
            if self.eat_single('*', '*') {
                value = value.checked_mul(self.unary()?).ok_or(M4Error::Usage)?;
            } else if self.eat("/") {
                let divisor = self.unary()?;
                if divisor == 0 {
                    return Err(M4Error::Usage);
                }
                value = value.checked_div(divisor).ok_or(M4Error::Usage)?;
            } else if self.eat("%") {
                let divisor = self.unary()?;
                if divisor == 0 {
                    return Err(M4Error::Usage);
                }
                value = value.checked_rem(divisor).ok_or(M4Error::Usage)?;
            } else {
                return Ok(value);
            }
        }
    }

    fn unary(&mut self) -> Result<i64, M4Error> {
        if self.eat("+") {
            return self.unary();
        }
        if self.eat("-") {
            return self.unary()?.checked_neg().ok_or(M4Error::Usage);
        }
        if self.eat("!") {
            return Ok(i64::from(self.unary()? == 0));
        }
        if self.eat("~") {
            return Ok(!self.unary()?);
        }
        self.power()
    }

    fn power(&mut self) -> Result<i64, M4Error> {
        let value = self.primary()?;
        if self.eat("**") {
            let exponent = self.unary()?;
            let exponent = u32::try_from(exponent).map_err(|_| M4Error::Usage)?;
            return value.checked_pow(exponent).ok_or(M4Error::Usage);
        }
        Ok(value)
    }

    fn primary(&mut self) -> Result<i64, M4Error> {
        self.skip_space();
        if self.eat("(") {
            let value = self.ternary()?;
            self.expect(")")?;
            return Ok(value);
        }
        self.number()
    }

    fn number(&mut self) -> Result<i64, M4Error> {
        self.skip_space();
        let mut radix = 10u32;
        if self.peek() == Some('0') {
            match self.peek_at(1) {
                Some('x' | 'X') => {
                    radix = 16;
                    self.position = self.position.saturating_add(2);
                }
                Some('b' | 'B') => {
                    radix = 2;
                    self.position = self.position.saturating_add(2);
                }
                Some(character) if character.is_ascii_digit() => {
                    radix = 8;
                    self.position = self.position.saturating_add(1);
                }
                _ => {}
            }
        }
        let start = self.position;
        while let Some(character) = self.peek() {
            if character.is_digit(radix) {
                self.position = self.position.saturating_add(1);
            } else {
                break;
            }
        }
        if self.position == start {
            return Err(M4Error::Usage);
        }
        let digits: String = self.characters[start..self.position].iter().collect();
        i64::from_str_radix(&digits, radix).map_err(|_| M4Error::Usage)
    }

    fn eat(&mut self, text: &str) -> bool {
        self.skip_space();
        let pattern: Vec<char> = text.chars().collect();
        if starts_with_at(&self.characters, self.position, &pattern) {
            self.position = self.position.saturating_add(pattern.len());
            true
        } else {
            false
        }
    }

    fn eat_single(&mut self, character: char, forbidden: char) -> bool {
        self.skip_space();
        if self.peek() == Some(character) && self.peek_at(1) != Some(forbidden) {
            self.position = self.position.saturating_add(1);
            true
        } else {
            false
        }
    }

    fn expect(&mut self, text: &str) -> Result<(), M4Error> {
        if self.eat(text) {
            Ok(())
        } else {
            Err(M4Error::Usage)
        }
    }

    fn peek(&self) -> Option<char> {
        self.characters.get(self.position).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<char> {
        self.characters
            .get(self.position.saturating_add(offset))
            .copied()
    }

    fn skip_space(&mut self) {
        while let Some(character) = self.peek() {
            if character.is_ascii_whitespace() {
                self.position = self.position.saturating_add(1);
            } else {
                break;
            }
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
