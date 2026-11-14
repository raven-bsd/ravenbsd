// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `raven completion` script generation from the command metadata.

use std::ffi::OsString;
use std::fmt::Write as _;

use raven_output::Exit;

use crate::Context;
use crate::commands::{COMMANDS, Command, CommandMeta};

static COMPLETION_META: CommandMeta = CommandMeta {
    name: "completion",
    summary: "print a shell completion script",
    usage: "raven completion <bash|zsh|fish>",
    reserved: false,
};

pub(crate) static COMPLETION_COMMAND: Completion = Completion;

/// The `completion` command.
pub(crate) struct Completion;

impl Command for Completion {
    fn meta(&self) -> &'static CommandMeta {
        &COMPLETION_META
    }

    fn run(&self, context: &mut Context<'_>, args: &[OsString]) -> Exit {
        let [shell] = args else {
            return context.usage("completion needs exactly one shell: bash, zsh or fish");
        };
        let shell = shell.to_string_lossy();
        let Some(script) = completion_script(&shell) else {
            return context.usage("completion shell must be bash, zsh or fish");
        };
        context.write_stdout(&script)
    }
}

fn completion_script(shell: &str) -> Option<String> {
    match shell {
        "bash" => Some(bash_script()),
        "zsh" => Some(zsh_script()),
        "fish" => Some(fish_script()),
        _ => None,
    }
}

fn names() -> String {
    let mut text = String::new();
    for command in COMMANDS {
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(command.meta().name);
    }
    text
}

fn bash_script() -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# bash completion for raven");
    let _ = writeln!(text, "_raven() {{");
    let _ = writeln!(text, "    local cur commands");
    let _ = writeln!(text, "    cur=\"${{COMP_WORDS[COMP_CWORD]}}\"");
    let _ = writeln!(text, "    commands=\"{}\"", names());
    let _ = writeln!(text, "    if [ \"$COMP_CWORD\" -eq 1 ]; then");
    let _ = writeln!(
        text,
        "        COMPREPLY=( $(compgen -W \"$commands\" -- \"$cur\") )"
    );
    let _ = writeln!(text, "        return 0");
    let _ = writeln!(text, "    fi");
    let _ = writeln!(text, "    COMPREPLY=()");
    let _ = writeln!(text, "}}");
    let _ = writeln!(text, "complete -F _raven raven");
    text
}

fn zsh_script() -> String {
    let mut text = String::new();
    let _ = writeln!(text, "#compdef raven");
    let _ = writeln!(text, "_raven() {{");
    let _ = writeln!(text, "    local -a commands");
    let _ = writeln!(text, "    commands=(");
    for command in COMMANDS {
        let meta = command.meta();
        let _ = writeln!(text, "        '{}:{}'", meta.name, meta.summary);
    }
    let _ = writeln!(text, "    )");
    let _ = writeln!(text, "    _describe 'command' commands");
    let _ = writeln!(text, "}}");
    let _ = writeln!(text, "_raven \"$@\"");
    text
}

fn fish_script() -> String {
    let mut text = String::new();
    let _ = writeln!(text, "# fish completion for raven");
    for command in COMMANDS {
        let meta = command.meta();
        let _ = writeln!(
            text,
            "complete -c raven -f -n '__fish_use_subcommand' -a {} -d '{}'",
            meta.name, meta.summary
        );
    }
    text
}
