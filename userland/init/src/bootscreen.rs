// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Styled boot screen renderer.
//!
//! Pure formatting so the host golden tests can byte-compare the plain and ANSI transcripts. The
//! target glue in `boot.rs` supplies the timed rows; `getty` prints the `login: ` prompt after
//! this screen.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

/// Terminal width fallback when the console size is unknown.
pub const WIDTH: usize = 80;
/// Fixed step-name column.
pub const NAME_WIDTH: usize = 18;

static PLAIN: AtomicBool = AtomicBool::new(false);

/// Select the plain transcript for `NO_COLOR` or `TERM=dumb`.
pub fn set_plain(plain: bool) {
    PLAIN.store(plain, Ordering::Release);
}

/// Whether the plain transcript is selected.
#[must_use]
pub fn plain_requested() -> bool {
    PLAIN.load(Ordering::Acquire)
}

/// One boot-step row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Elapsed time from the boot start in milliseconds.
    pub elapsed_ms: u64,
    /// Step name.
    pub name: String,
    /// Short status: `ok`, `warn` or `fail`.
    pub status: Status,
}

/// Status shown in the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Ready.
    Ok,
    /// Non-required service not ready.
    Warn,
    /// Required service failed.
    Fail,
}

impl Status {
    fn text(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Warn => "warn",
            Self::Fail => "fail",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Ok => "\x1b[32m",
            Self::Warn => "\x1b[33m",
            Self::Fail => "\x1b[31m",
        }
    }
}

/// Render the screen for `arch`, sorted by completion time.
#[must_use]
pub fn render(arch: &str, rows: &[Row], color: bool) -> String {
    let mut rows: Vec<Row> = rows.to_vec();
    rows.sort_by_key(|row| row.elapsed_ms);
    let mut text = String::new();
    let title = "R A V E N   B S D";
    let rule = "-".repeat(WIDTH);
    if color {
        text.push_str("\x1b[1;36m");
        text.push_str(title);
        text.push_str("\x1b[0m\n");
        text.push_str("\x1b[2m");
        text.push_str(&rule);
        text.push_str("\x1b[0m\n");
    } else {
        text.push_str(title);
        text.push('\n');
        text.push_str(&rule);
        text.push('\n');
    }
    for row in &rows {
        let elapsed = format!("{:06.2}", row.elapsed_ms as f64 / 1000.0);
        let name = pad_name(&row.name);
        text.push_str(&elapsed);
        text.push_str("  ");
        text.push_str(&name);
        text.push_str("  ");
        if color {
            text.push_str(row.status.color());
            text.push_str(row.status.text());
            text.push_str("\x1b[0m\n");
        } else {
            text.push_str(row.status.text());
            text.push('\n');
        }
    }
    if color {
        text.push_str("\x1b[2m");
        text.push_str(&rule);
        text.push_str("\x1b[0m\n");
    } else {
        text.push_str(&rule);
        text.push('\n');
    }
    text.push('\n');
    text.push_str(&format!("Welcome to RavenBSD 0.1 {arch}\n"));
    text
}

fn pad_name(name: &str) -> String {
    let mut text = name.to_string();
    while text.len() < NAME_WIDTH {
        text.push(' ');
    }
    if text.len() > NAME_WIDTH {
        text.truncate(NAME_WIDTH);
    }
    text
}

/// Remove ANSI SGR sequences so a qtest or golden can compare the plain transcript.
#[must_use]
pub fn strip_ansi(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == 0x1b && bytes.get(index + 1) == Some(&b'[') {
            index = index.saturating_add(2);
            while index < bytes.len() {
                let byte = bytes[index];
                index = index.saturating_add(1);
                if byte.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(char::from(bytes[index]));
            index = index.saturating_add(1);
        }
    }
    out
}

#[cfg(test)]
#[path = "boot_ut.rs"]
mod tests;
