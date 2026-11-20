// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The plain/ANSI two-pane renderer. It is deliberately independent of terminal size probing so
//! the host tests can assert an exact frame.
//!
//! Color is a presentation layer only: every selection also carries the `>` marker, so
//! `NO_COLOR`, `TERM=dumb` and `--color never` keep the same information.

use crate::app::{Focus, Tui};

const LEFT_WIDTH: usize = 26;
const RESET: &str = "\x1b[0m";
const TITLE: &str = "\x1b[1;36m";
const VIOLET: &str = "\x1b[38;5;183m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const SELECT: &str = "\x1b[7;38;5;183m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";
const SEPARATOR: &str = "\x1b[90m";

/// Render one full frame.
#[must_use]
pub fn render(tui: &Tui) -> String {
    let width = tui.options.width.max(60);
    let right_width = width.saturating_sub(LEFT_WIDTH + 3);
    let color = tui.options.color;
    let mut output = String::new();
    if color {
        output.push_str("\x1b[2J\x1b[H");
        output.push_str(&paint(color, TITLE, "raven config"));
        output.push('\n');
        output.push_str(&paint(color, DIM, "Users, network and system settings"));
        output.push('\n');
        output.push_str(&rule(width, color));
        output.push('\n');
    } else {
        output.push_str("\n--- raven config ---\n");
    }

    let mut left = Vec::new();
    left.push(paint(color, BOLD, "Modules"));
    if tui.search {
        left.push(paint(color, VIOLET, &format!("/{}", tui.filter)));
    } else {
        left.push(paint(color, DIM, "/ or type: filter"));
    }
    left.push(String::new());
    let filtered = tui.filtered_modules();
    for (position, index) in filtered.iter().enumerate() {
        let selected = (tui.focus == Focus::List && position == tui.selected)
            || (tui.focus == Focus::Form && *index == tui.active);
        let marker = if selected { ">" } else { " " };
        let module = &tui.modules[*index];
        let line = format!("{marker} {}", module.title);
        left.push(if selected {
            paint(color, SELECT, &line)
        } else {
            line
        });
    }

    let mut right = Vec::new();
    if let Some(module) = tui.selected_module() {
        right.push(format!(
            "{}  {}",
            paint(color, TITLE, &module.title),
            paint(color, DIM, &format!("({})", module.summary))
        ));
        right.push(String::new());
        for (index, field) in module.fields.iter().enumerate() {
            let selected = tui.focus == Focus::Form && index == tui.field;
            let marker = if selected { ">" } else { " " };
            let label = if selected {
                paint(color, VIOLET, &format!("{}:", field.label))
            } else {
                paint(color, BOLD, &format!("{}:", field.label))
            };
            right.push(format!("{marker} {label} {}", field.display()));
            if selected {
                right.push(format!("    {}", paint(color, DIM, field.help)));
            }
            if let Some(error) = &field.error {
                right.push(format!("    {}", paint(color, RED, &format!("! {error}"))));
            }
        }
        right.push(String::new());
        if module.writable {
            right.push(format!(
                "{}  {}",
                paint(color, GREEN, "[ Save ]"),
                paint(color, DIM, "[ Cancel ]")
            ));
        } else {
            right.push(paint(color, DIM, "[ Back ]"));
        }
    } else {
        right.push(String::from("No module selected"));
    }
    right.push(String::new());
    let status_color = if tui.status.starts_with("Validation") {
        RED
    } else if tui.status.starts_with("Module") || tui.status.starts_with("Dry run") {
        GREEN
    } else {
        YELLOW
    };
    right.push(format!(
        "{} {}",
        paint(color, BOLD, "status:"),
        paint(color, status_color, &tui.status)
    ));

    let rows = left.len().max(right.len());
    for row in 0..rows {
        let left_text = left.get(row).map_or("", String::as_str);
        let right_text = right.get(row).map_or("", String::as_str);
        output.push_str(&fit(left_text, LEFT_WIDTH));
        output.push_str(&paint(color, SEPARATOR, " | "));
        output.push_str(&fit(right_text, right_width));
        output.push('\n');
    }
    if color {
        output.push_str(&rule(width, color));
        output.push('\n');
    }
    output.push_str(&fit(
        &paint(
            color,
            DIM,
            "Up/Down select  Tab field  Left/Right choice  Enter save  Esc back  Ctrl-S save  q cancel",
        ),
        width,
    ));
    output.push('\n');
    output
}

fn paint(color: bool, style: &str, text: &str) -> String {
    if color {
        format!("{style}{text}{RESET}")
    } else {
        text.to_string()
    }
}

fn rule(width: usize, color: bool) -> String {
    let line = format!("+{}+", "-".repeat(width.saturating_sub(2)));
    paint(color, BOLD, &line)
}

fn fit(text: &str, width: usize) -> String {
    let mut output = truncate_visible(text, width);
    let length = visible_len(&output);
    if length < width {
        output.push_str(&" ".repeat(width - length));
    }
    if text.contains('\x1b') && visible_len(text) > width && !output.ends_with(RESET) {
        output.push_str(RESET);
    }
    output
}

/// Count visible characters, skipping SGR escape sequences.
fn visible_len(text: &str) -> usize {
    let mut visible = 0;
    let mut escape = false;
    for character in text.chars() {
        if escape {
            if character.is_ascii_alphabetic() {
                escape = false;
            }
        } else if character == '\x1b' {
            escape = true;
        } else {
            visible += 1;
        }
    }
    visible
}

/// Truncate to `width` visible characters while preserving complete escape sequences.
fn truncate_visible(text: &str, width: usize) -> String {
    let mut output = String::new();
    let mut visible = 0;
    let mut escape = false;
    for character in text.chars() {
        if escape {
            output.push(character);
            if character.is_ascii_alphabetic() {
                escape = false;
            }
            continue;
        }
        if character == '\x1b' {
            escape = true;
            output.push(character);
            continue;
        }
        if visible >= width {
            break;
        }
        output.push(character);
        visible += 1;
    }
    output
}

#[cfg(test)]
#[path = "render_ut.rs"]
mod tests;
