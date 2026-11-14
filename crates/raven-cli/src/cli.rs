// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `raven` parser, global flags and output context.

use std::ffi::{OsStr, OsString};
use std::io::Write;

use lexopt::{Arg, Parser};
use raven_output::{Color, ErrorEnvelope, Exit, Format, Record};

use crate::commands;

/// The process streams and the two environment signals the parser needs.
pub struct Io<'a> {
    /// Standard output.
    pub stdout: &'a mut dyn Write,
    /// Standard error.
    pub stderr: &'a mut dyn Write,
    /// Whether stdout is a terminal (the human mode may use colour).
    pub is_tty: bool,
    /// Whether `NO_COLOR` is present in the environment.
    pub no_color: bool,
}

/// The per-command output context.
pub struct Context<'a> {
    /// The selected output mode.
    pub format: Format,
    /// Whether human output may use colour.
    pub color: bool,
    /// Extra human diagnostics on stderr; no effect on JSON.
    pub verbose: bool,
    /// Mutating commands print a plan and change nothing.
    pub dry_run: bool,
    /// Assume yes for a destructive action.
    pub yes: bool,
    stdout: &'a mut dyn Write,
    stderr: &'a mut dyn Write,
}

impl Context<'_> {
    /// Write normal human output; quiet mode suppresses it.
    pub fn write_stdout(&mut self, text: &str) -> Exit {
        if self.format == Format::Quiet {
            return Exit::Success;
        }
        match self.stdout.write_all(text.as_bytes()) {
            Ok(()) => Exit::Success,
            Err(_) => Exit::Failure,
        }
    }

    /// Write one record as a JSON line; quiet mode suppresses it.
    ///
    /// Human-mode commands must write their own columns; calling this in human mode is a
    /// programming error and returns [`Exit::Failure`] without output.
    pub fn emit_record(&mut self, record: &Record) -> Exit {
        match self.format {
            Format::Quiet => Exit::Success,
            Format::Json => {
                let mut line = record.to_json();
                line.push('\n');
                match self.stdout.write_all(line.as_bytes()) {
                    Ok(()) => Exit::Success,
                    Err(_) => Exit::Failure,
                }
            }
            Format::Human => Exit::Failure,
        }
    }

    /// Emit an operational error (`exit 1`) without context.
    pub fn fail(&mut self, code: &str, message: &str) -> Exit {
        self.fail_context(code, message, None)
    }

    /// Emit an operational error (`exit 1`) with one context pair.
    pub fn fail_context(
        &mut self,
        code: &str,
        message: &str,
        context: Option<(&str, &str)>,
    ) -> Exit {
        self.emit_error(code, message, context, Exit::Failure)
    }

    /// Emit a usage error (`exit 2`) without context.
    pub fn usage(&mut self, message: &str) -> Exit {
        self.usage_context(message, None)
    }

    /// Emit a usage error (`exit 2`) with one context pair.
    pub fn usage_context(&mut self, message: &str, context: Option<(&str, &str)>) -> Exit {
        self.emit_error("EUSAGE", message, context, Exit::Usage)
    }

    fn emit_error(
        &mut self,
        code: &str,
        message: &str,
        context: Option<(&str, &str)>,
        status: Exit,
    ) -> Exit {
        let mut envelope = ErrorEnvelope::new(code, message);
        if let Some((key, value)) = context {
            envelope = envelope.with_context(key, value);
        }
        let result = if self.format == Format::Json {
            let mut line = envelope.to_json();
            line.push('\n');
            self.stderr.write_all(line.as_bytes())
        } else {
            writeln!(self.stderr, "raven: {message}")
        };
        if result.is_err() {
            return Exit::Failure;
        }
        status
    }
}

#[derive(Clone, Copy, Debug)]
struct Globals {
    json: bool,
    quiet: bool,
    verbose: bool,
    dry_run: bool,
    yes: bool,
    help: bool,
    version: bool,
    color: Color,
}

impl Default for Globals {
    fn default() -> Self {
        Self {
            json: false,
            quiet: false,
            verbose: false,
            dry_run: false,
            yes: false,
            help: false,
            version: false,
            color: Color::Auto,
        }
    }
}

/// Run one `raven` command line (without `argv[0]`).
///
/// All argument errors return [`Exit::Usage`]; command failures return [`Exit::Failure`].
pub fn run<I>(args: I, io: &mut Io<'_>) -> Exit
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let mut globals = Globals::default();
    let mut command: Option<OsString> = None;
    let mut parser = Parser::from_args(args);

    loop {
        let argument = match parser.next() {
            Ok(Some(argument)) => argument,
            Ok(None) => break,
            Err(error) => return usage_failure(io, globals.json, error.to_string()),
        };
        match argument {
            Arg::Long("json") => globals.json = true,
            Arg::Long("quiet") => globals.quiet = true,
            Arg::Long("verbose") => globals.verbose = true,
            Arg::Long("dry-run") => globals.dry_run = true,
            Arg::Long("yes") => globals.yes = true,
            Arg::Short('h') | Arg::Long("help") => globals.help = true,
            Arg::Short('V') | Arg::Long("version") => globals.version = true,
            Arg::Long("color") => {
                let value = match parser.value() {
                    Ok(value) => value,
                    Err(error) => return usage_failure(io, globals.json, error.to_string()),
                };
                if let Err(message) = set_color(&mut globals, &value) {
                    return usage_failure(io, globals.json, message);
                }
            }
            Arg::Value(value) => {
                command = Some(value);
                break;
            }
            other => {
                return usage_failure(
                    io,
                    globals.json,
                    format!("unknown option `{}`", display_arg(&other)),
                );
            }
        }
    }

    let mut command_args = if command.is_some() {
        match parser.raw_args() {
            Ok(raw) => raw.collect::<Vec<OsString>>(),
            Err(error) => return usage_failure(io, globals.json, error.to_string()),
        }
    } else {
        Vec::new()
    };

    if let Err(message) = extract_command_globals(&mut globals, &mut command_args) {
        return usage_failure(io, globals.json, message);
    }

    if globals.json && globals.quiet {
        return usage_failure(
            io,
            true,
            String::from("--json and --quiet are mutually exclusive"),
        );
    }

    let format = if globals.quiet {
        Format::Quiet
    } else if globals.json {
        Format::Json
    } else {
        Format::Human
    };
    let color = globals.color.enabled(io.is_tty, io.no_color);
    let mut context = Context {
        format,
        color,
        verbose: globals.verbose,
        dry_run: globals.dry_run,
        yes: globals.yes,
        stdout: &mut *io.stdout,
        stderr: &mut *io.stderr,
    };

    if globals.help {
        let name = command
            .as_ref()
            .map(|value| value.to_string_lossy().into_owned());
        return commands::help::show(&mut context, name.as_deref());
    }
    if globals.version {
        return commands::version::show(&mut context);
    }

    let Some(command) = command else {
        if format == Format::Human {
            return commands::help::show(&mut context, None);
        }
        return context.usage("missing command");
    };
    let name = command.to_string_lossy().into_owned();
    let Some(selected) = commands::command(&name) else {
        let message = format!("unknown command `{name}`");
        return context.usage_context(&message, Some(("command", &name)));
    };
    selected.run(&mut context, &command_args)
}

fn set_color(globals: &mut Globals, value: &OsStr) -> Result<(), String> {
    let Some(text) = value.to_str() else {
        return Err(String::from("--color needs a UTF-8 value"));
    };
    let Some(color) = Color::parse(text) else {
        return Err(format!(
            "invalid --color value `{text}` (want auto, always or never)"
        ));
    };
    globals.color = color;
    Ok(())
}

fn extract_command_globals(globals: &mut Globals, args: &mut Vec<OsString>) -> Result<(), String> {
    let mut rest = Vec::with_capacity(args.len());
    let mut options = true;
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy().into_owned();
        if options {
            match text.as_str() {
                "--" => {
                    options = false;
                    index += 1;
                    continue;
                }
                "--json" => {
                    globals.json = true;
                    index += 1;
                    continue;
                }
                "--quiet" => {
                    globals.quiet = true;
                    index += 1;
                    continue;
                }
                "--verbose" => {
                    globals.verbose = true;
                    index += 1;
                    continue;
                }
                "--dry-run" => {
                    globals.dry_run = true;
                    index += 1;
                    continue;
                }
                "--yes" => {
                    globals.yes = true;
                    index += 1;
                    continue;
                }
                "-h" | "--help" => {
                    globals.help = true;
                    index += 1;
                    continue;
                }
                "-V" | "--version" => {
                    globals.version = true;
                    index += 1;
                    continue;
                }
                "--color" => {
                    let Some(value) = args.get(index + 1) else {
                        return Err(String::from("--color needs a value"));
                    };
                    set_color(globals, value)?;
                    index += 2;
                    continue;
                }
                color if color.starts_with("--color=") => {
                    let value = &color["--color=".len()..];
                    let Some(parsed) = Color::parse(value) else {
                        return Err(format!(
                            "invalid --color value `{value}` (want auto, always or never)"
                        ));
                    };
                    globals.color = parsed;
                    index += 1;
                    continue;
                }
                _ => {}
            }
        }
        rest.push(args[index].clone());
        index += 1;
    }
    *args = rest;
    Ok(())
}

fn usage_failure(io: &mut Io<'_>, json: bool, message: String) -> Exit {
    let envelope = ErrorEnvelope::new("EUSAGE", &message);
    let result = if json {
        let mut line = envelope.to_json();
        line.push('\n');
        io.stderr.write_all(line.as_bytes())
    } else {
        writeln!(io.stderr, "raven: {message}")
    };
    let _ = result;
    Exit::Usage
}

fn display_arg(argument: &Arg<'_>) -> String {
    match argument {
        Arg::Short(name) => format!("-{name}"),
        Arg::Long(name) => format!("--{name}"),
        Arg::Value(value) => value.to_string_lossy().into_owned(),
    }
}
