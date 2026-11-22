// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The reserved command names.
//!
//! These names are part of the public namespace. K74 recognizes them and returns
//! `EUNAVAILABLE` with exit 1; it never prints placeholder status, doctor or log data. K76 and
//! K78 replace the commands they own by adding a module and a [`crate::COMMANDS`] row.

use std::ffi::OsString;

use raven_output::Exit;

use crate::Context;
use crate::commands::{Command, CommandMeta};

pub(crate) struct ReservedCommand {
    meta: &'static CommandMeta,
}

impl Command for ReservedCommand {
    fn meta(&self) -> &'static CommandMeta {
        self.meta
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        if let Some(argument) = args
            .iter()
            .find(|argument| argument.to_string_lossy().starts_with('-'))
        {
            let message = format!(
                "unknown option `{}` for `{}`",
                argument.to_string_lossy(),
                self.meta.name
            );
            return context.usage(&message);
        }
        let message = format!("`{}` is not available in this build", self.meta.name);
        context.fail_context("EUNAVAILABLE", &message, Some(("command", self.meta.name)))
    }
}

macro_rules! reserved_command {
    ($command:ident, $meta:ident, $name:literal, $summary:literal, $usage:literal) => {
        static $meta: CommandMeta = CommandMeta {
            name: $name,
            summary: $summary,
            usage: $usage,
            reserved: true,
        };
        pub(crate) static $command: ReservedCommand = ReservedCommand { meta: &$meta };
    };
}

reserved_command!(
    LOG,
    LOG_META,
    "log",
    "reserved: log access",
    "raven log <command>"
);
reserved_command!(
    PKG,
    PKG_META,
    "pkg",
    "reserved: package access",
    "raven pkg <command>"
);
reserved_command!(
    USER,
    USER_META,
    "user",
    "reserved: user identities",
    "raven user <command>"
);
reserved_command!(
    IMAGE,
    IMAGE_META,
    "image",
    "reserved: image management",
    "raven image <command>"
);
