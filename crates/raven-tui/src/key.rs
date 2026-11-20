// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-to-event parser for the serial/terminal input.
//!
//! The parser accepts both a canonical line (characters followed by `\n`) and a raw stream with
//! ANSI arrows, so the same TUI works with the shell's line discipline and with the qtest's
//! serial input.

use crate::app::Event;

/// Parse one input chunk into key events.
#[must_use]
pub fn parse(bytes: &[u8]) -> Vec<Event> {
    let mut events = Vec::new();
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' | b'\n' => events.push(Event::Enter),
            b'\t' => events.push(Event::Tab),
            0x7f | 0x08 => events.push(Event::Backspace),
            0x13 => events.push(Event::CtrlSave),
            0x03 => events.push(Event::CtrlCancel),
            0x1b => {
                if index + 2 < bytes.len() && bytes[index + 1] == b'[' {
                    let event = match bytes[index + 2] {
                        b'A' => Some(Event::Up),
                        b'B' => Some(Event::Down),
                        b'C' => Some(Event::Right),
                        b'D' => Some(Event::Left),
                        _ => None,
                    };
                    if let Some(event) = event {
                        events.push(event);
                        index += 3;
                        continue;
                    }
                }
                events.push(Event::Escape);
            }
            byte if byte.is_ascii_graphic() || byte == b' ' => {
                events.push(Event::Char(byte as char));
            }
            _ => {}
        }
        index += 1;
    }
    events
}

#[cfg(test)]
#[path = "key_ut.rs"]
mod tests;
