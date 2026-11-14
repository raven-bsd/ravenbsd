// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven help` and the `-h`/`--help` flags.

use std::ffi::OsString;
use std::fmt::Write as _;

use raven_output::Exit;

use crate::Context;
use crate::commands::{COMMANDS, Command, CommandMeta};

static HELP_META: CommandMeta = CommandMeta {
    name: "help",
    summary: "show this help or help for a command",
    usage: "raven help [<command>]",
    reserved: false,
};

pub(crate) static HELP_COMMAND: Help = Help;

/// The `help` command.
pub(crate) struct Help;

impl Command for Help {
    fn meta(&self) -> &'static CommandMeta {
        &HELP_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        match args {
            [] => show(context, None),
            [name] => {
                let name = name.to_string_lossy();
                show(context, Some(&name))
            }
            _ => context.usage("help accepts at most one command"),
        }
    }
}

/// Show general help or help for one command.
pub(crate) fn show(context: &mut Context<'_>, command: Option<&str>) -> Exit {
    let Some(command) = command else {
        return context.write_stdout(&general_help());
    };
    let Some(selected) = crate::command(command) else {
        let message = format!("unknown command `{command}`");
        return context.usage_context(&message, Some(("command", command)));
    };
    context.write_stdout(&command_help(selected.meta()))
}

fn general_help() -> String {
    let mut text = String::new();
    let _ = writeln!(text, "raven - RavenBSD administration client");
    let _ = writeln!(text);
    let _ = writeln!(text, "Usage: raven [OPTIONS] <COMMAND> [ARGUMENTS]");
    let _ = writeln!(text);
    let _ = writeln!(text, "Commands:");
    for command in COMMANDS {
        let meta = command.meta();
        let _ = writeln!(text, "  {:<14}{}", meta.name, meta.summary);
    }
    let _ = writeln!(text);
    text.push_str(global_options());
    text
}

fn command_help(meta: &CommandMeta) -> String {
    let mut text = String::new();
    let _ = writeln!(text, "Usage: {}", meta.usage);
    let _ = writeln!(text);
    let _ = writeln!(text, "{}", meta.summary);
    if meta.reserved {
        let _ = writeln!(text);
        let _ = writeln!(
            text,
            "This command is reserved; it exits 1 with EUNAVAILABLE in this build."
        );
    }
    let _ = writeln!(text);
    text.push_str(global_options());
    text
}

fn global_options() -> &'static str {
    "Global options:
  --json             emit NDJSON records
  --quiet            suppress normal output
  --verbose          extra diagnostics on stderr
  --color=<policy>   auto, always or never (human output only)
  --dry-run          print a plan and change nothing
  --yes              assume yes for a destructive action
  -h, --help         show this help
  -V, --version      print the version
"
}
