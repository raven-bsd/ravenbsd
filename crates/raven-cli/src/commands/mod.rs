// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Command metadata and dispatch.
//!
//! K76 and K78 add one module and one [`COMMANDS`] row for each new command; the parser core in
//! [`crate::cli`] does not change.

use std::ffi::OsString;

use raven_output::Exit;

use crate::Context;

pub mod completion;
pub mod help;
pub mod reserved;
pub mod theme;
pub mod version;

/// Static metadata for one command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CommandMeta {
    /// The command word after `raven`.
    pub name: &'static str,
    /// One-line summary used by help and completion.
    pub summary: &'static str,
    /// The usage line shown by `raven help <command>`.
    pub usage: &'static str,
    /// Whether the command is recognized but its subsystem is not in this build.
    pub reserved: bool,
}

/// One command module.
pub trait Command: Sync {
    /// Metadata for help, completion and dispatch.
    fn meta(&self) -> &'static CommandMeta;

    /// Run the command with the arguments left after the command word and global flags.
    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit;
}

/// The command table, in the order shown by `raven help`.
pub static COMMANDS: &[&dyn Command] = &[
    &version::VERSION_COMMAND,
    &help::HELP_COMMAND,
    &completion::COMPLETION_COMMAND,
    &crate::cmd::status::STATUS_COMMAND,
    &crate::cmd::doctor::DOCTOR_COMMAND,
    &crate::cmd::svc::SVC_COMMAND,
    &crate::cmd::config::CONFIG_COMMAND,
    &theme::THEME_COMMAND,
    &crate::cmd::net::NET_COMMAND,
    &reserved::LOG,
    &reserved::PKG,
    &reserved::USER,
    &reserved::IMAGE,
];

/// Find one command by its name.
#[must_use]
pub fn command(name: &str) -> Option<&'static dyn Command> {
    COMMANDS
        .iter()
        .copied()
        .find(|command| command.meta().name == name)
}
