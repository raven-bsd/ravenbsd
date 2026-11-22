// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party `raven config` TUI: state machine, renderer and network drop-in writer.
//!
//! The binary decides whether the terminal is interactive and calls [`run`]; the state machine
//! and renderer are host-testable without a terminal. K198 owns the network apply; this crate
//! owns the form, validation, atomic drop-in write and the visual shell.

#![forbid(unsafe_code)]

mod app;
mod key;
mod model;
mod render;

pub use app::{Action, Event, Focus, Options, Tui};
pub use model::{
    Field, FieldKind, Module, NETWORK_FILE, NetworkPlan, ValidationError, network_path,
};
pub use render::render;

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// What the TUI finished with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The validated plan was written to this path.
    Saved(PathBuf),
    /// The plan was rendered; nothing was written.
    DryRun(String),
    /// The user cancelled or input ended before a save.
    Cancelled,
    /// A write or internal error.
    Error(String),
}

/// Run the TUI until save, dry-run or cancel.
///
/// `input` can be a terminal or a scripted [`std::io::Cursor`]; the parser accepts lines and ANSI
/// arrows. The caller owns the root (the target passes `/`) and the output writer.
pub fn run<R: Read, W: Write>(
    root: &Path,
    options: Options,
    input: &mut R,
    output: &mut W,
) -> std::io::Result<Outcome> {
    let mut tui = Tui::new(root, options);
    writeln!(output, "raven config: TUI")?;
    write!(output, "{}", render(&tui))?;
    let mut buffer = [0u8; 64];
    let mut pending = Vec::new();
    loop {
        let read = input.read(&mut buffer)?;
        if read == 0 {
            writeln!(output, "raven config: cancelled")?;
            return Ok(Outcome::Cancelled);
        }
        pending.extend_from_slice(&buffer[..read]);
        let split = complete_prefix(&pending);
        let events = key::parse(&pending[..split]);
        pending.drain(..split);
        if events.is_empty() {
            continue;
        }
        for event in events {
            let action = tui.handle(event);
            if let Some(notice) = tui.take_notice() {
                writeln!(output, "{notice}")?;
            }
            match action {
                Action::Continue | Action::Validation(_) => {}
                Action::Saved(path) => {
                    writeln!(output, "raven config: saved {}", path.display())?;
                    return Ok(Outcome::Saved(path));
                }
                Action::DryRun { target, text } => {
                    let state = if Path::new(&target).exists() {
                        "present"
                    } else {
                        "absent"
                    };
                    writeln!(output, "raven config: dry-run plan={target} file={state}")?;
                    write!(output, "{text}")?;
                    return Ok(Outcome::DryRun(text));
                }
                Action::Cancelled => {
                    writeln!(output, "raven config: cancelled")?;
                    return Ok(Outcome::Cancelled);
                }
                Action::Error(error) => {
                    writeln!(output, "raven config: error: {error}")?;
                    return Ok(Outcome::Error(error));
                }
            }
        }
        write!(output, "{}", render(&tui))?;
    }
}

/// Split off a trailing incomplete ANSI escape (`ESC` or `ESC [`).
///
/// The serial console delivers an arrow key as separate bytes; parsing `ESC` alone would cancel
/// the TUI instead of moving the selection, so the incomplete tail waits for the next read.
fn complete_prefix(bytes: &[u8]) -> usize {
    let length = bytes.len();
    if length >= 1 && bytes[length - 1] == 0x1b {
        return length - 1;
    }
    if length >= 2 && bytes[length - 2] == 0x1b && bytes[length - 1] == b'[' {
        return length - 2;
    }
    length
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
