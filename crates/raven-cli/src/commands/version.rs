// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven version`.

use std::ffi::OsString;

use raven_output::{Exit, Format, Record};

use crate::Context;
use crate::commands::{Command, CommandMeta};

static VERSION_META: CommandMeta = CommandMeta {
    name: "version",
    summary: "print version, schema and target information",
    usage: "raven version [--json]",
    reserved: false,
};

pub(crate) static VERSION_COMMAND: Version = Version;

/// The `version` command.
pub(crate) struct Version;

impl Command for Version {
    fn meta(&self) -> &'static CommandMeta {
        &VERSION_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        if !args.is_empty() {
            return context.usage("version takes no arguments");
        }
        show(context)
    }
}

/// Print the version in the selected mode.
pub(crate) fn show(context: &mut Context<'_>) -> Exit {
    if context.format != Format::Json {
        return context.write_stdout(&format!("raven {} ({})\n", crate::VERSION, crate::TARGET));
    }
    let record = Record::new("version")
        .string("version", crate::VERSION)
        .u64("schema_version", 1)
        .string("target", crate::TARGET)
        .null("build")
        .null("boot_id");
    context.emit_record(&record)
}
