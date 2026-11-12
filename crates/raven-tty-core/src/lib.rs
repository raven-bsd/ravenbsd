// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Terminal line discipline for `consd` (K36).
//!
//! The discipline is a byte-oriented state machine: console input goes in, and the same call
//! returns the bytes to echo, the completed lines, raw-mode data and the asynchronous events the
//! console server has to act on (`Interrupt`, `EndOfFile`). It performs no I/O and has no clock,
//! so every rule in `architecture/16-first-shell.md` section 3 is host-testable.
//!
//! Canonical mode implements the line editing a serial console needs: `\r` and `\n` end a line
//! (the completed line is delivered with a `\n` terminator), backspace erases, `^U` kills,
//! `^C` discards the line and raises [`Event::Interrupt`], `^D` on an empty line raises
//! [`Event::EndOfFile`] and `^L` is inserted verbatim. Raw mode passes bytes through without echo
//! or editing.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::vec::Vec;

/// Longest accepted input line, in bytes (without the terminator).
pub const MAX_LINE: usize = 1024;

/// The bell byte a full line answers with.
pub const BELL: u8 = 0x07;

/// End-of-line byte delivered with every completed canonical line.
pub const NEWLINE: u8 = b'\n';

/// Erase byte the terminal echoes for a backspace (`"\b \b"`).
const ERASE: &[u8] = b"\x08 \x08";

const CTRL_C: u8 = 0x03;
const CTRL_D: u8 = 0x04;
const CTRL_L: u8 = 0x0c;
const CTRL_U: u8 = 0x15;
const DEL: u8 = 0x7f;
const BACKSPACE: u8 = 0x08;

/// Input mode of the discipline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// Line editing with echo; `READ` returns whole lines.
    #[default]
    Canonical,
    /// Byte stream without echo or editing.
    Raw,
}

/// Mode and echo switch of the terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    /// Canonical or raw input.
    pub mode: Mode,
    /// Echo input bytes back to the console in canonical mode.
    pub echo: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::Canonical,
            echo: true,
        }
    }
}

/// An asynchronous condition the console server must deliver to a waiting reader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `^C`: discard the current line and interrupt the pending read (`-EINTR`).
    Interrupt,
    /// `^D` on an empty line: the pending read returns zero bytes.
    EndOfFile,
}

/// Everything one [`Discipline::feed`] call produced.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Output {
    /// Bytes to write back to the console (canonical mode only).
    pub echo: Vec<u8>,
    /// Completed lines, each terminated with [`NEWLINE`].
    pub lines: Vec<Vec<u8>>,
    /// Bytes for the reader in raw mode.
    pub data: Vec<u8>,
    /// Events in the order they happened.
    pub events: Vec<Event>,
}

/// The line discipline state.
#[derive(Clone, Debug)]
pub struct Discipline {
    settings: Settings,
    line: [u8; MAX_LINE],
    len: usize,
}

impl Default for Discipline {
    fn default() -> Self {
        Self::new()
    }
}

impl Discipline {
    /// A canonical, echoing discipline with an empty line.
    pub const fn new() -> Self {
        Self {
            settings: Settings {
                mode: Mode::Canonical,
                echo: true,
            },
            line: [0u8; MAX_LINE],
            len: 0,
        }
    }

    /// Current mode and echo switch.
    pub fn settings(&self) -> Settings {
        self.settings
    }

    /// Apply new settings, returning the number of pending input bytes discarded.
    ///
    /// A partially edited line cannot be carried across a mode switch (in raw mode it would be
    /// delivered as a line the reader never saw), so a mode change drops it.
    pub fn set_settings(&mut self, settings: Settings) -> usize {
        let discarded = if settings.mode == self.settings.mode {
            0
        } else {
            let pending = self.len;
            self.len = 0;
            pending
        };
        self.settings = settings;
        discarded
    }

    /// Number of bytes in the line being edited.
    pub fn pending(&self) -> usize {
        self.len
    }

    /// Discard the pending line and report how many bytes were dropped.
    pub fn flush(&mut self) -> usize {
        let pending = self.len;
        self.len = 0;
        pending
    }

    /// Feed console input through the discipline.
    pub fn feed(&mut self, input: &[u8], out: &mut Output) {
        for &byte in input {
            if self.settings.mode == Mode::Raw {
                out.data.push(byte);
                continue;
            }
            match byte {
                b'\r' | NEWLINE => self.complete_line(out),
                BACKSPACE | DEL => self.erase(out),
                CTRL_U => self.kill_line(out),
                CTRL_C => self.interrupt(out),
                CTRL_D => self.eof_or_complete(out),
                CTRL_L => self.insert(byte, out),
                _ => self.insert(byte, out),
            }
        }
    }

    fn complete_line(&mut self, out: &mut Output) {
        let mut line = Vec::with_capacity(self.len + 1);
        line.extend_from_slice(&self.line[..self.len]);
        line.push(NEWLINE);
        out.lines.push(line);
        self.len = 0;
        self.echo_terminator(out);
    }

    fn erase(&mut self, out: &mut Output) {
        if self.len == 0 {
            return;
        }
        self.len -= 1;
        if self.settings.echo {
            out.echo.extend_from_slice(ERASE);
        }
    }

    fn kill_line(&mut self, out: &mut Output) {
        if self.len == 0 {
            return;
        }
        let erased = self.len;
        self.len = 0;
        if self.settings.echo {
            for _ in 0..erased {
                out.echo.extend_from_slice(ERASE);
            }
        }
    }

    fn interrupt(&mut self, out: &mut Output) {
        self.len = 0;
        if self.settings.echo {
            out.echo.extend_from_slice(b"^C");
            out.echo.extend_from_slice(b"\r\n");
        }
        out.events.push(Event::Interrupt);
    }

    fn eof_or_complete(&mut self, out: &mut Output) {
        if self.len == 0 {
            out.events.push(Event::EndOfFile);
            return;
        }
        self.complete_line(out);
    }

    fn insert(&mut self, byte: u8, out: &mut Output) {
        if self.len == MAX_LINE {
            if self.settings.echo {
                out.echo.push(BELL);
            }
            return;
        }
        self.line[self.len] = byte;
        self.len += 1;
        if self.settings.echo {
            out.echo.push(byte);
        }
    }

    fn echo_terminator(&mut self, out: &mut Output) {
        if self.settings.echo {
            out.echo.extend_from_slice(b"\r\n");
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
