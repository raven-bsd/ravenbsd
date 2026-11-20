// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The plain/ANSI two-pane renderer. It is deliberately independent of terminal size probing so
//! the host tests can assert an exact frame.

use crate::app::{Focus, Tui};

const LEFT_WIDTH: usize = 24;

/// Render one full frame.
#[must_use]
pub fn render(tui: &Tui) -> String {
    let width = tui.options.width.max(60);
    let right_width = width.saturating_sub(LEFT_WIDTH + 1);
    let mut output = String::new();
    if tui.options.color {
        output.push_str("\x1b[2J\x1b[H");
        output.push_str("\x1b[1mraven config\x1b[0m\n");
    } else {
        output.push_str("\n--- raven config ---\n");
    }

    let mut left = Vec::new();
    left.push(String::from("Modules"));
    if tui.search {
        left.push(format!("/{}", tui.filter));
    } else {
        left.push(String::from("/ or type: filter"));
    }
    left.push(String::new());
    let filtered = tui.filtered_modules();
    for (position, index) in filtered.iter().enumerate() {
        let selected = (tui.focus == Focus::List && position == tui.selected)
            || (tui.focus == Focus::Form && *index == tui.active);
        let marker = if selected { ">" } else { " " };
        let module = &tui.modules[*index];
        left.push(format!("{marker} {}", module.title));
    }

    let mut right = Vec::new();
    if let Some(module) = tui.selected_module() {
        right.push(format!("{}  ({})", module.title, module.summary));
        right.push(String::new());
        for (index, field) in module.fields.iter().enumerate() {
            let marker = if tui.focus == Focus::Form && index == tui.field {
                ">"
            } else {
                " "
            };
            right.push(format!("{marker} {}: {}", field.label, field.display()));
            if tui.focus == Focus::Form && index == tui.field {
                right.push(format!("    {}", field.help));
            }
            if let Some(error) = &field.error {
                right.push(format!("    ! {error}"));
            }
        }
        right.push(String::new());
        if module.writable {
            right.push(String::from("[ Save ]  [ Cancel ]"));
        } else {
            right.push(String::from("[ Back ]"));
        }
    } else {
        right.push(String::from("No module selected"));
    }
    right.push(String::new());
    right.push(format!("status: {}", tui.status));

    let rows = left.len().max(right.len());
    for row in 0..rows {
        let left_text = left.get(row).map_or("", String::as_str);
        let right_text = right.get(row).map_or("", String::as_str);
        output.push_str(&fit(left_text, LEFT_WIDTH));
        output.push(' ');
        output.push_str(&fit(right_text, right_width));
        output.push('\n');
    }
    output.push_str(&fit(
        "/ search  Up/Down select  Tab/Up/Down field  Left/Right choice  Enter save  Esc cancel",
        width,
    ));
    output.push('\n');
    output
}

fn fit(text: &str, width: usize) -> String {
    let mut output: String = text.chars().take(width).collect();
    let length = output.chars().count();
    if length < width {
        output.push_str(&" ".repeat(width - length));
    }
    output
}

#[cfg(test)]
#[path = "render_ut.rs"]
mod tests;
