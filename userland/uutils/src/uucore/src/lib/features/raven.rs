// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The shared RavenBSD output surface for the selected uutils tools.
//!
//! One theme source (`raven-theme`) and one NDJSON envelope (`raven-output`) are used by every
//! tool that gains `--json`. Human output stays in each tool; this module only supplies the
//! common argument, colour decision, clap help styles and JSON writer.

use std::io::{self, IsTerminal, Write};

use clap::builder::styling::{AnsiColor, Effects, Styles};
use clap::{Arg, ArgAction, ArgMatches, Command};
pub use raven_output::{ErrorEnvelope, Exit, Record, SCHEMA, Value};
use raven_theme::Decision;

/// The `--json` flag id.
pub const JSON: &str = "raven-json";
/// The `--color` flag id.
pub const COLOR: &str = "raven-color";

/// Add the `--json` mode to a command.
#[must_use]
pub fn add_json(command: Command) -> Command {
    command.arg(
        Arg::new(JSON)
            .long("json")
            .help("Write one NDJSON record per result")
            .action(ArgAction::SetTrue),
    )
}

/// Add the `--color` policy to a command.
#[must_use]
pub fn add_color(command: Command) -> Command {
    command.arg(
        Arg::new(COLOR)
            .long("color")
            .value_name("WHEN")
            .value_parser(["auto", "always", "never"])
            .default_value("auto")
            .help("Colour the human output: auto, always or never"),
    )
}

/// Add both shared output flags.
#[must_use]
pub fn add_output(command: Command) -> Command {
    add_color(add_json(command))
}

/// Whether the JSON mode was requested.
#[must_use]
pub fn json(matches: &ArgMatches) -> bool {
    matches.get_flag(JSON)
}

/// The requested colour policy.
#[must_use]
pub fn color(matches: &ArgMatches) -> Decision {
    matches
        .get_one::<String>(COLOR)
        .and_then(|value| Decision::parse(value))
        .unwrap_or_default()
}

/// Resolve whether human output may carry ANSI.
#[must_use]
pub fn color_enabled(matches: &ArgMatches, json: bool) -> bool {
    color(matches).enabled(
        io::stdout().is_terminal(),
        std::env::var_os("NO_COLOR").is_some(),
        std::env::var("TERM").is_ok_and(|term| term == "dumb"),
        json,
    )
}

/// Apply the Raven Violet styles to a clap command when the environment allows colour.
///
/// `--help` is parsed before the normal options, so the client-visible `--color` flag is
/// honoured by a small argv scan; `NO_COLOR`, `TERM=dumb`, `CLICOLOR_FORCE` and the TTY state
/// complete the decision. JSON output never reaches this path.
#[must_use]
pub fn style(command: Command) -> Command {
    if env_color_enabled() {
        command.color(clap::ColorChoice::Always).styles(
            Styles::styled()
                .header(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
                .usage(AnsiColor::Magenta.on_default().effects(Effects::BOLD))
                .literal(AnsiColor::Cyan.on_default())
                .placeholder(AnsiColor::BrightBlack.on_default()),
        )
    } else {
        command.color(clap::ColorChoice::Never)
    }
}

/// Return the semantic palette used by the tools.
#[must_use]
pub fn palette() -> raven_theme::Palette {
    raven_theme::Palette::default()
}

/// The `LS_COLORS` value for the Raven Violet palette when the environment has none.
#[must_use]
pub fn default_ls_colors() -> String {
    raven_theme::adapters::ls_colors(&palette(), raven_theme::Depth::Ansi256)
}

/// Write records as NDJSON on stdout.
pub fn write_records(records: &[Record]) -> io::Result<()> {
    let mut writer = io::stdout().lock();
    for record in records {
        writeln!(writer, "{}", record.to_json())?;
    }
    Ok(())
}

/// Write one JSON error envelope on stderr and return the failure exit code.
pub fn json_error(tool: &str, code: &str, message: &str) -> i32 {
    let envelope = ErrorEnvelope::new(code, message).with_context("tool", tool);
    let mut writer = io::stderr().lock();
    let _ = writeln!(writer, "{}", envelope.to_json());
    Exit::Failure.code()
}

fn env_color_enabled() -> bool {
    let mut forced = false;
    let mut denied = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = if let Some(value) = arg.strip_prefix("--color=") {
            Some(value.to_owned())
        } else if arg == "--color" {
            args.next()
        } else {
            None
        };
        match value.as_deref() {
            Some("always") => forced = true,
            Some("never") => denied = true,
            _ => {}
        }
    }
    if forced {
        return true;
    }
    if denied {
        return false;
    }
    if std::env::var_os("NO_COLOR").is_some() {
        return false;
    }
    if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
        return false;
    }
    if std::env::var("CLICOLOR_FORCE").is_ok_and(|value| value != "0") {
        return true;
    }
    io::stdout().is_terminal()
}

#[cfg(test)]
#[path = "raven_ut.rs"]
mod tests;
