// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `uux` remote command construction and queueing.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use uucp::config::Config;
use uucp::job::{JobKind, NamedFile};
use uucp::queue::{self, QueueError};
use uucp::spool::{Spool, SpoolError};

/// Parsed `uux` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `-a user`: requesting user.
    pub user: Option<String>,
    /// `-g grade`: grade byte, default `n`.
    pub grade: u8,
    /// `-j`: print the queued job id.
    pub print_id: bool,
    /// `-n`: never mail completion.
    pub no_mail: bool,
    /// `-z`: mail on success.
    pub mail_on_success: bool,
    /// `-b`/`-c`/`-C`: copy semantics. The queue always carries the input payload.
    pub copy: bool,
    /// `-x debug`: parsed debug level, not interpreted.
    pub debug: Option<String>,
    /// The command string.
    pub command: String,
}

/// Why a `uux` command line is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option is unknown.
    UnknownOption(char),
    /// An option that takes a value has none.
    MissingValue(char),
    /// A grade is not one byte.
    BadGrade,
    /// No command string was supplied.
    MissingCommand,
    /// A quote or backslash is not terminated.
    BadQuote,
}

/// Why a `uux` operation failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UuxError {
    /// The spool filesystem failed.
    Spool(SpoolError),
    /// The queue failed.
    Queue(QueueError),
    /// The command string has no `system!` prefix.
    NotRemote,
    /// The destination system is not configured.
    UnknownSystem(String),
    /// An input redirection has no file name.
    MissingInput,
    /// The command string is malformed.
    InvalidCommand,
}

impl core::fmt::Display for UuxError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Spool(error) => write!(formatter, "spool: {error:?}"),
            Self::Queue(error) => write!(formatter, "{error}"),
            Self::NotRemote => formatter.write_str("command has no `system!` prefix"),
            Self::UnknownSystem(system) => write!(formatter, "unknown system `{system}`"),
            Self::MissingInput => formatter.write_str("input redirection needs a file"),
            Self::InvalidCommand => formatter.write_str("invalid command string"),
        }
    }
}

impl From<SpoolError> for UuxError {
    fn from(error: SpoolError) -> Self {
        Self::Spool(error)
    }
}

impl From<QueueError> for UuxError {
    fn from(error: QueueError) -> Self {
        Self::Queue(error)
    }
}

/// One `<` input reference.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    /// File name, or `-` for standard input.
    pub name: String,
    /// True when the input is `-`.
    pub stdin: bool,
}

/// A parsed remote command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// Remote system.
    pub system: String,
    /// Command after the `system!` prefix.
    pub command: String,
    /// `<` inputs in order.
    pub inputs: Vec<Input>,
    /// `>`/`>>` output names in order.
    pub outputs: Vec<String>,
}

/// Parse `uux [-bcCjnz] [-a user] [-g grade] [-x debug] command-string`.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a missing option value, a multi-byte grade or a
/// missing command.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Options, UsageError> {
    let arguments: Vec<&str> = args.into_iter().collect();
    let mut options = Options {
        user: None,
        grade: b'n',
        print_id: false,
        no_mail: false,
        mail_on_success: false,
        copy: false,
        debug: None,
        command: String::new(),
    };
    let mut index = 0usize;
    let mut options_enabled = true;
    while index < arguments.len() {
        let argument = *arguments.get(index).ok_or(UsageError::MissingCommand)?;
        if options_enabled && argument == "--" {
            options_enabled = false;
            index = index.saturating_add(1);
            continue;
        }
        if options_enabled && argument.starts_with('-') && argument.len() > 1 {
            let bytes = argument.as_bytes();
            let mut offset = 1usize;
            while offset < bytes.len() {
                let option = char::from(*bytes.get(offset).ok_or(UsageError::MissingValue('?'))?);
                match option {
                    'b' | 'c' | 'C' => options.copy = true,
                    'j' => options.print_id = true,
                    'n' => options.no_mail = true,
                    'z' => options.mail_on_success = true,
                    'a' | 'g' | 'x' => {
                        let attached = argument.get(offset.saturating_add(1)..).unwrap_or("");
                        let value = if attached.is_empty() {
                            index = index.saturating_add(1);
                            *arguments
                                .get(index)
                                .ok_or(UsageError::MissingValue(option))?
                        } else {
                            attached
                        };
                        match option {
                            'a' => options.user = Some(value.to_string()),
                            'g' => {
                                let grade = value.as_bytes();
                                if grade.len() != 1 {
                                    return Err(UsageError::BadGrade);
                                }
                                options.grade =
                                    grade.first().copied().ok_or(UsageError::BadGrade)?;
                            }
                            'x' => options.debug = Some(value.to_string()),
                            _ => {}
                        }
                        offset = bytes.len();
                        continue;
                    }
                    _ => return Err(UsageError::UnknownOption(option)),
                }
                offset = offset.saturating_add(1);
            }
            index = index.saturating_add(1);
            continue;
        }
        options.command = arguments
            .get(index..)
            .ok_or(UsageError::MissingCommand)?
            .join(" ");
        break;
    }
    if options.command.is_empty() {
        return Err(UsageError::MissingCommand);
    }
    Ok(options)
}

/// Parse the `system!command` string and its `<`, `>`, `>>` redirections.
///
/// # Errors
///
/// Returns [`UuxError::NotRemote`] without a `system!` prefix, [`UuxError::InvalidCommand`] for an
/// empty system/command, [`UuxError::MissingInput`] for an incomplete redirection, or
/// [`UuxError::Spool`]-independent quote errors are reported as [`UuxError::InvalidCommand`].
pub fn parse_command_spec(command: &str) -> Result<CommandSpec, UuxError> {
    let Some(separator) = command.find('!') else {
        return Err(UuxError::NotRemote);
    };
    let system = command.get(..separator).ok_or(UuxError::InvalidCommand)?;
    let body = command
        .get(separator.saturating_add(1)..)
        .ok_or(UuxError::InvalidCommand)?;
    if system.is_empty()
        || body.is_empty()
        || system.contains('/')
        || system.chars().any(char::is_whitespace)
    {
        return Err(UuxError::InvalidCommand);
    }
    let words = split_words(body).map_err(|_| UuxError::InvalidCommand)?;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut index = 0usize;
    while index < words.len() {
        let word = words.get(index).ok_or(UuxError::InvalidCommand)?;
        match word.as_str() {
            "<" => {
                let name = words
                    .get(index.saturating_add(1))
                    .ok_or(UuxError::MissingInput)?;
                if name.is_empty() {
                    return Err(UuxError::MissingInput);
                }
                inputs.push(Input {
                    name: name.clone(),
                    stdin: name == "-",
                });
                index = index.saturating_add(2);
            }
            ">" | ">>" => {
                let name = words
                    .get(index.saturating_add(1))
                    .ok_or(UuxError::MissingInput)?;
                if name.is_empty() {
                    return Err(UuxError::MissingInput);
                }
                outputs.push(name.clone());
                index = index.saturating_add(2);
            }
            _ => index = index.saturating_add(1),
        }
    }
    Ok(CommandSpec {
        system: system.to_string(),
        command: body.to_string(),
        inputs,
        outputs,
    })
}

/// Whether the command string reads `< -`.
///
/// # Errors
///
/// Returns [`UuxError`] when the command cannot be parsed.
pub fn reads_stdin(command: &str) -> Result<bool, UuxError> {
    Ok(parse_command_spec(command)?
        .inputs
        .iter()
        .any(|input| input.stdin))
}

/// Queue one remote command.
///
/// `stdin` is used only when the command contains `< -`.
///
/// # Errors
///
/// Returns [`UuxError`] for an unknown peer, a missing input file or a spool/queue failure.
pub fn run(
    options: &Options,
    config: &Config,
    spool: &mut dyn Spool,
    default_user: &str,
    stdin: &[u8],
) -> Result<u64, UuxError> {
    let spec = parse_command_spec(&options.command)?;
    if !config.peer_known(&spec.system) {
        return Err(UuxError::UnknownSystem(spec.system));
    }
    let mut files = Vec::new();
    let mut standard_input = Vec::new();
    for input in &spec.inputs {
        if input.stdin {
            standard_input = stdin.to_vec();
            continue;
        }
        let data = spool.read(&input.name)?;
        files.push(NamedFile {
            name: input.name.clone(),
            data,
        });
    }
    let user = options.user.as_deref().unwrap_or(default_user);
    let job = queue::enqueue(
        spool,
        config,
        user,
        &spec.system,
        options.grade,
        JobKind::Command {
            command: spec.command,
            stdin: standard_input,
            files,
            outputs: spec.outputs,
        },
    )?;
    Ok(job.id)
}

fn split_words(text: &str) -> Result<Vec<String>, ()> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut started = false;
    for character in text.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            started = true;
            continue;
        }
        if character == '\\' {
            escaped = true;
            started = true;
            continue;
        }
        if let Some(active) = quote {
            if character == active {
                quote = None;
            } else {
                current.push(character);
            }
            started = true;
            continue;
        }
        match character {
            '\'' | '"' => {
                quote = Some(character);
                started = true;
            }
            value if value.is_whitespace() => {
                if started {
                    words.push(current.clone());
                    current.clear();
                    started = false;
                }
            }
            value => {
                current.push(value);
                started = true;
            }
        }
    }
    if escaped || quote.is_some() {
        return Err(());
    }
    if started {
        words.push(current);
    }
    Ok(words)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
