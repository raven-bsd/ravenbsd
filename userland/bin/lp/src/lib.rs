// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `lp` client model.
//!
//! The client parses the Issue 8 options, resolves the destination in the required
//! `-d`, `LPDEST`, `PRINTER` order and writes a synchronous request to an explicitly named
//! local output path. A logical printer name has no service behind it in this image, so it is
//! reported as unavailable instead of pretending that a page was printed.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Usage syntax printed after a usage error.
pub const USAGE: &str = "[-c] [-d dest] [-n copies] [-msw] [-o option]... [-t title] [file ...]";

/// Diagnostic when `-d`, `LPDEST` and `PRINTER` all fail to name a destination.
pub const NO_DESTINATION: &str = "lp: no print destination configured";

/// Parsed `lp` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// `-c`: keep the input available to the output operation.
    pub copy: bool,
    /// `-d dest`: explicit destination before the environment lookup.
    pub destination: Option<String>,
    /// `-n copies`: positive copy count.
    pub copies: u32,
    /// `-m`: mail the user after printing.
    pub mail: bool,
    /// `-s`: suppress informative messages; diagnostics still go to standard error.
    pub silent: bool,
    /// `-t title`: banner title.
    pub title: Option<String>,
    /// `-w`: notify the user after printing.
    pub write_terminal: bool,
    /// `-o option`: printer-dependent options in command-line order.
    pub printer_options: Vec<String>,
    /// File operands; a lone `-` selects standard input.
    pub files: Vec<String>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            copy: false,
            destination: None,
            copies: 1,
            mail: false,
            silent: false,
            title: None,
            write_terminal: false,
            printer_options: Vec::new(),
            files: Vec::new(),
        }
    }
}

/// Why an argument list is invalid.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// An option character outside the `lp` option set.
    UnknownOption(char),
    /// An option that needs a value was the last argument.
    MissingOptionArgument(char),
    /// `-n` did not receive a positive decimal integer in the `u32` range.
    BadCopies,
    /// `-d` received an empty destination name.
    EmptyDestination,
}

impl UsageError {
    /// Return the stable diagnostic for this error.
    pub fn message(&self) -> String {
        match self {
            Self::UnknownOption(option) => format!("lp: unknown option -- '{option}'"),
            Self::MissingOptionArgument(option) => {
                format!("lp: option requires an argument -- '{option}'")
            }
            Self::BadCopies => "lp: -n requires a positive decimal integer".to_string(),
            Self::EmptyDestination => "lp: -d requires a non-empty destination".to_string(),
        }
    }
}

/// Parse `lp [-c] [-d dest] [-n copies] [-msw] [-o option]... [-t title] [file ...]`.
///
/// Short options may be grouped (`-cmsw`); an option that takes a value consumes either the rest
/// of its argument (`-dlp0`, `-n3`) or the next argument. `--` ends option processing. A lone `-`
/// is an operand that selects standard input.
///
/// # Errors
///
/// Returns [`UsageError`] for an unknown option, a missing option argument, a zero or malformed
/// copy count, or an empty `-d` destination.
pub fn parse<'a>(args: impl IntoIterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut options = Options::default();
    let mut arguments = args.into_iter();
    let mut options_enabled = true;
    while let Some(argument) = arguments.next() {
        if options_enabled && argument == "--" {
            options_enabled = false;
            continue;
        }
        if !options_enabled || argument == "-" || !argument.starts_with('-') {
            options.files.push(argument.to_string());
            continue;
        }
        let characters = argument.char_indices().skip(1);
        for (index, option) in characters {
            match option {
                'c' => options.copy = true,
                'm' => options.mail = true,
                's' => options.silent = true,
                'w' => options.write_terminal = true,
                'd' | 'n' | 'o' | 't' => {
                    let start = index.saturating_add(option.len_utf8());
                    let value = match argument.get(start..) {
                        Some("") | None => match arguments.next() {
                            Some(value) => value,
                            None => return Err(UsageError::MissingOptionArgument(option)),
                        },
                        Some(value) => value,
                    };
                    match option {
                        'd' => {
                            if value.is_empty() {
                                return Err(UsageError::EmptyDestination);
                            }
                            options.destination = Some(value.to_string());
                        }
                        'n' => options.copies = parse_copies(value)?,
                        'o' => options.printer_options.push(value.to_string()),
                        't' => options.title = Some(value.to_string()),
                        _ => return Err(UsageError::UnknownOption(option)),
                    }
                    break;
                }
                other => return Err(UsageError::UnknownOption(other)),
            }
        }
    }
    Ok(options)
}

fn parse_copies(value: &str) -> Result<u32, UsageError> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(UsageError::BadCopies);
    }
    match value.parse::<u32>() {
        Ok(copies) if copies > 0 => Ok(copies),
        _ => Err(UsageError::BadCopies),
    }
}

/// Resolve the destination with the POSIX precedence `-d`, `LPDEST`, `PRINTER`.
///
/// Empty environment values are treated as unset, which lets a shell inherit a null variable
/// without hiding a later candidate.
#[must_use]
pub fn resolve_destination(
    cli: Option<&str>,
    lpdest: Option<&str>,
    printer: Option<&str>,
) -> Option<String> {
    [cli, lpdest, printer]
        .into_iter()
        .flatten()
        .find(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// Return the operands that supply input, defaulting to standard input.
#[must_use]
pub fn input_names(files: &[String]) -> Vec<&str> {
    if files.is_empty() {
        Vec::from(["-"])
    } else {
        files.iter().map(String::as_str).collect()
    }
}

/// How a resolved destination is reached.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DestinationKind {
    /// An absolute path or an explicit RavenBSD scheme path that can be opened directly.
    LocalPath,
    /// A logical printer name; this image has no print service for it.
    Named,
}

/// Classify a resolved destination.
///
/// POSIX destination names are implementation-defined. RavenBSD treats an absolute path or an
/// explicit scheme path (`tty:console`) as a local output destination and every other spelling as
/// a logical printer name. A relative path is deliberately not local, so a request cannot write
/// into the current directory by accident.
#[must_use]
pub fn destination_kind(destination: &str) -> DestinationKind {
    let scheme = destination.find(':').is_some_and(|index| {
        destination
            .get(..index)
            .is_some_and(|prefix| !prefix.contains('/'))
    });
    if destination.starts_with('/') || scheme {
        DestinationKind::LocalPath
    } else {
        DestinationKind::Named
    }
}

/// Why a destination could not accept a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubmitError {
    /// The destination is a logical printer and no print service is configured.
    NoService,
    /// A local destination could not be opened or written; holds the errno description.
    Device(&'static str),
}

impl SubmitError {
    /// Return the stable diagnostic for this refusal.
    pub fn message(self, destination: &str) -> String {
        match self {
            Self::NoService => format!("lp: destination '{destination}' is not available"),
            Self::Device(error) => {
                format!("lp: destination '{destination}' is not available: {error}")
            }
        }
    }
}

/// One complete input document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    /// Operand spelling; `-` means standard input.
    pub name: String,
    /// Copied bytes in input order.
    pub data: Vec<u8>,
}

/// A complete print request after destination resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Request {
    /// Parsed command line.
    pub options: Options,
    /// Resolved destination.
    pub destination: String,
    /// Input documents in operand order.
    pub documents: Vec<Document>,
}

/// Iterate over the bytes to write, one document at a time and one complete copy at a time.
///
/// Every document is written as an integral whole; a multi-copy request repeats the full document
/// sequence rather than interleaving portions of different files.
pub fn copy_order(request: &Request) -> impl Iterator<Item = &[u8]> + '_ {
    (0..request.options.copies).flat_map(move |_| {
        request
            .documents
            .iter()
            .map(|document| document.data.as_slice())
    })
}

/// Informative message written after a successful request when `-w` is set.
#[must_use]
pub fn printed_message(destination: &str) -> String {
    format!("lp: request printed on '{destination}'")
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
