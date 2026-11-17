// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX `write` delivery model.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;

/// Parsed `write` command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Options {
    /// Target user.
    pub user: String,
    /// Optional target terminal.
    pub tty: Option<String>,
}

/// Why an argument list is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsageError {
    /// The user operand is missing.
    MissingUser,
    /// More than one terminal operand was supplied.
    ExtraOperand,
}

/// Why delivery failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The user is not in the account database.
    UnknownUser,
    /// The user has no logged-in terminal.
    NotLoggedIn,
    /// The target has `mesg n`.
    MesgOff,
    /// The terminal write failed.
    Send,
}

/// Directory lookup seam.
pub trait Directory {
    /// Whether the user exists.
    fn known(&self, user: &str) -> bool;
}

/// Terminal lookup/delivery seam.
pub trait Terminal {
    /// Whether the user has a logged-in terminal (optionally the named one).
    fn logged_in(&mut self, user: &str, tty: Option<&str>) -> bool;
    /// Whether the target accepts messages.
    fn mesg_enabled(&mut self, user: &str, tty: Option<&str>) -> bool;
    /// Send the message.
    fn send(&mut self, user: &str, tty: Option<&str>, message: &str) -> bool;
}

/// Parse `write user [tty]`.
///
/// # Errors
///
/// Returns [`UsageError`] for a missing user or an extra operand.
pub fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, UsageError> {
    let args: Vec<String> = args.into_iter().collect();
    match args.as_slice() {
        [] => Err(UsageError::MissingUser),
        [user] => Ok(Options {
            user: user.clone(),
            tty: None,
        }),
        [user, tty] => Ok(Options {
            user: user.clone(),
            tty: Some(tty.clone()),
        }),
        _ => Err(UsageError::ExtraOperand),
    }
}

/// Deliver one message, returning the number of bytes sent.
///
/// # Errors
///
/// Returns [`Error::UnknownUser`], [`Error::NotLoggedIn`], [`Error::MesgOff`]
/// or [`Error::Send`].
pub fn deliver(
    options: &Options,
    message: &str,
    directory: &dyn Directory,
    terminal: &mut dyn Terminal,
) -> Result<usize, Error> {
    if !directory.known(&options.user) {
        return Err(Error::UnknownUser);
    }
    if !terminal.logged_in(&options.user, options.tty.as_deref()) {
        return Err(Error::NotLoggedIn);
    }
    if !terminal.mesg_enabled(&options.user, options.tty.as_deref()) {
        return Err(Error::MesgOff);
    }
    let text = normalize(message);
    if !terminal.send(&options.user, options.tty.as_deref(), &text) {
        return Err(Error::Send);
    }
    Ok(text.len())
}

/// Convert local newlines to terminal CRLF and keep every line.
#[must_use]
pub fn normalize(message: &str) -> String {
    let mut text = String::new();
    for line in message.lines() {
        text.push_str(line);
        text.push_str("\r\n");
    }
    text
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
