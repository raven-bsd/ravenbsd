// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The binary-side guard for the interactive `raven config` TUI.
//!
//! The library config command keeps the scriptable `check|show|get` surface. Only the binary
//! opens the TUI, and only for a TTY with no subcommand and no machine-readable flag.

use std::ffi::OsString;
use std::io::Write;

/// The accepted interactive invocation.
pub(crate) struct Request {
    pub(crate) dry_run: bool,
    pub(crate) color: bool,
}

/// Return a request when `args` is an interactive `raven config` invocation.
pub(crate) fn request(args: &[OsString], is_tty: bool) -> Option<Request> {
    if !is_tty {
        return None;
    }
    let mut positional = Vec::new();
    let mut dry_run = false;
    let mut machine = false;
    let mut help = false;
    let mut color_never = false;
    let mut color_always = false;
    let mut index = 0;
    while index < args.len() {
        let text = args[index].to_string_lossy();
        match text.as_ref() {
            "--json" | "--quiet" => machine = true,
            "--verbose" | "--yes" => {}
            "--dry-run" => dry_run = true,
            "-h" | "--help" | "-V" | "--version" => help = true,
            "--color" => {
                let value = args.get(index + 1)?.to_string_lossy();
                match value.as_ref() {
                    "never" => color_never = true,
                    "always" => color_always = true,
                    "auto" => {}
                    _ => return None,
                }
                index += 2;
                continue;
            }
            other if other.starts_with("--color=") => match &other["--color=".len()..] {
                "never" => color_never = true,
                "always" => color_always = true,
                "auto" => {}
                _ => return None,
            },
            other if other.starts_with('-') => return None,
            other => positional.push(other.to_string()),
        }
        index += 1;
    }
    if machine || help || positional.len() != 1 || positional[0] != "config" {
        return None;
    }
    let no_color = std::env::var_os("NO_COLOR").is_some()
        || std::env::var_os("TERM").is_some_and(|value| value.to_str() == Some("dumb"));
    let color = if color_never {
        false
    } else if color_always {
        true
    } else {
        is_tty && !no_color
    };
    Some(Request { dry_run, color })
}

/// A target console reader over the `tty:console` scheme.
#[cfg(target_os = "ravenbsd")]
struct ConsoleInput(raven_tty::Console);

#[cfg(target_os = "ravenbsd")]
impl std::io::Read for ConsoleInput {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        self.0
            .read(buffer)
            .map_err(|error| std::io::Error::other(format!("{error}")))
    }
}

/// Run the TUI and map its outcome to a process exit code.
pub(crate) fn run<W: Write>(writer: &mut W, request: &Request) -> i32 {
    // The real console is the `tty:console` scheme; `consd` owns its line discipline. The guard
    // saves the canonical/echo settings and switches raw through the K36 protocol. A plain POSIX
    // tty uses the termios guard instead; a non-TTY qtest keeps the canonical parser.
    #[cfg(target_os = "ravenbsd")]
    let use_console =
        std::env::var_os("RAVEN_TUI_RAW").is_some() || std::env::var_os("RAVEN_TUI").is_none();
    #[cfg(target_os = "ravenbsd")]
    let (mut input, _termios): (Box<dyn std::io::Read>, Option<raven_tty::RawMode>) = if use_console
    {
        match raven_tty::Console::open() {
            Ok(mut console) => {
                if let Err(error) = console.enable_raw() {
                    let _ = writeln!(writer, "raven config: console raw mode: {error}");
                }
                (Box::new(ConsoleInput(console)), None)
            }
            Err(error) => {
                let _ = writeln!(writer, "raven config: console open: {error}");
                match raven_tty::RawMode::enable(0) {
                    Ok(guard) => (Box::new(std::io::stdin()), Some(guard)),
                    Err(error) => {
                        let _ = writeln!(writer, "raven config: termios raw mode: {error}");
                        (Box::new(std::io::stdin()), None)
                    }
                }
            }
        }
    } else {
        (Box::new(std::io::stdin()), None)
    };
    #[cfg(not(target_os = "ravenbsd"))]
    let mut input: Box<dyn std::io::Read> = Box::new(std::io::stdin());
    let options = raven_tui::Options {
        dry_run: request.dry_run,
        color: request.color,
        width: 100,
    };
    // The default root is the live `/etc`. The qtest sets a writable ramfs root because the
    // admin-test initfs is read-only; the release/UFS profile does not need the override.
    let root = std::env::var_os("RAVEN_CONFIG_ROOT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from("/"));
    match raven_tui::run(&root, options, &mut input, writer) {
        Ok(raven_tui::Outcome::Saved(_))
        | Ok(raven_tui::Outcome::DryRun(_))
        | Ok(raven_tui::Outcome::Cancelled) => 0,
        Ok(raven_tui::Outcome::Error(error)) => {
            let _ = writeln!(writer, "raven config: error: {error}");
            1
        }
        Err(error) => {
            let _ = writeln!(writer, "raven config: error: {error}");
            1
        }
    }
}

#[cfg(test)]
#[path = "tui_ut.rs"]
mod tests;
