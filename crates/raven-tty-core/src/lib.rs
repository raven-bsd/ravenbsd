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
//! `^C` discards the line and raises [`Event::Interrupt`], `^\` discards it and sets
//! [`Output::quit`] for the server's `SIGQUIT`, `^D` on an empty line raises
//! [`Event::EndOfFile`] and `^L` is inserted verbatim. Raw mode passes bytes through without echo
//! or editing.
//!
//! [`Termios`] is the complete terminal attribute record: the pty stores it, the protocol carries
//! it field by field and `tcgetattr` returns it unchanged (K444). The bit rules are the NetBSD
//! `ttyinput_wlock`/`ttyoutput` ones for the flags a scheme pty can honour (`ICRNL`, `INLCR`,
//! `IGNCR`, `ISTRIP`, `IXON`, `ISIG`, `ICANON`, `ECHO`, `OPOST`, `ONLCR`, `OCRNL`); bits with no
//! hardware behind them are preserved so the round-trip is exact.

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
const CTRL_U: u8 = 0x15;
const DEL: u8 = 0x7f;
const BACKSPACE: u8 = 0x08;

/// `IGNBRK`: ignore break.
pub const IGNBRK: u32 = 0x0000_0001;
/// `BRKINT`: break raises `SIGINT`.
pub const BRKINT: u32 = 0x0000_0002;
/// `IGNPAR`: ignore parity errors.
pub const IGNPAR: u32 = 0x0000_0004;
/// `PARMRK`: mark parity errors.
pub const PARMRK: u32 = 0x0000_0008;
/// `INPCK`: enable input parity checking.
pub const INPCK: u32 = 0x0000_0010;
/// `ISTRIP`: strip the eighth input bit.
pub const ISTRIP: u32 = 0x0000_0020;
/// `INLCR`: translate input `\n` to `\r`.
pub const INLCR: u32 = 0x0000_0040;
/// `IGNCR`: ignore input `\r`.
pub const IGNCR: u32 = 0x0000_0080;
/// `ICRNL`: translate input `\r` to `\n`.
pub const ICRNL: u32 = 0x0000_0100;
/// `IXON`: enable start/stop output flow control.
pub const IXON: u32 = 0x0000_0200;
/// `IXANY`: any character restarts output.
pub const IXANY: u32 = 0x0000_0800;
/// `IXOFF`: enable start/stop input flow control.
pub const IXOFF: u32 = 0x0000_0400;
/// `IMAXBEL`: ring the bell on input overflow.
pub const IMAXBEL: u32 = 0x0000_2000;
/// `IUTF8`: input is UTF-8.
pub const IUTF8: u32 = 0x0000_4000;

/// `OPOST`: enable output processing.
pub const OPOST: u32 = 0x0000_0001;
/// `ONLCR`: map output `\n` to `\r\n`.
pub const ONLCR: u32 = 0x0000_0002;
/// `OCRNL`: map output `\r` to `\n`.
pub const OCRNL: u32 = 0x0000_0010;
/// `ONOCR`: do not output `\r` at column zero.
pub const ONOCR: u32 = 0x0000_0020;
/// `ONLRET`: output `\n` also returns the carriage.
pub const ONLRET: u32 = 0x0000_0040;

/// `CS8`: eight-bit characters.
pub const CS8: u32 = 0x0000_0300;
/// `CREAD`: enable the receiver.
pub const CREAD: u32 = 0x0000_0800;
/// `B38400`, the pty default line speed (numeric donor value).
pub const B38400: u32 = 38_400;

/// `ISIG`: enable `VINTR`/`VQUIT`/`VSUSP` signals.
pub const ISIG: u32 = 0x0000_0080;
/// `ICANON`: enable canonical input processing.
pub const ICANON: u32 = 0x0000_0100;
/// `ECHO`: echo input characters.
pub const ECHO: u32 = 0x0000_0008;
/// `ECHOE`: echo erase as `"\b \b"`.
pub const ECHOE: u32 = 0x0000_0002;
/// `ECHOK`: echo `VKILL` as a newline.
pub const ECHOK: u32 = 0x0000_0004;
/// `ECHONL`: echo `\n` even when `ECHO` is off.
pub const ECHONL: u32 = 0x0000_0010;
/// `IEXTEN`: enable implementation-defined input processing.
pub const IEXTEN: u32 = 0x0000_0400;

/// Index of `VINTR` in `c_cc`.
pub const VINTR: usize = 0;
/// Index of `VQUIT` in `c_cc`.
pub const VQUIT: usize = 1;
/// Index of `VERASE` in `c_cc`.
pub const VERASE: usize = 2;
/// Index of `VKILL` in `c_cc`.
pub const VKILL: usize = 3;
/// Index of `VEOF` in `c_cc`.
pub const VEOF: usize = 4;
/// Index of `VTIME` in `c_cc`.
pub const VTIME: usize = 5;
/// Index of `VMIN` in `c_cc`.
pub const VMIN: usize = 6;
/// Index of `VSTART` in `c_cc`.
pub const VSTART: usize = 8;
/// Index of `VSTOP` in `c_cc`.
pub const VSTOP: usize = 9;
/// Index of `VSUSP` in `c_cc`.
pub const VSUSP: usize = 10;
/// Index of `VEOL` in `c_cc`.
pub const VEOL: usize = 11;
/// Index of `VWERASE` in `c_cc`.
pub const VWERASE: usize = 14;

/// The disabled special-character value (`_POSIX_VDISABLE`, DR-71/C20 is `0xff`).
pub const VDISABLE: u8 = 0xff;

/// The complete `struct termios` attribute record.
///
/// The field layout and bit values are the Raven ABI (`libc/ravenlibc/abi/structs.toml`), which
/// follows the BSD termios layout. The wire protocol encodes every field explicitly; this type is
/// never sent as raw memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Termios {
    /// Input flags (`c_iflag`).
    pub iflag: u32,
    /// Output flags (`c_oflag`).
    pub oflag: u32,
    /// Control flags (`c_cflag`).
    pub cflag: u32,
    /// Local flags (`c_lflag`).
    pub lflag: u32,
    /// Line discipline (`c_line`).
    pub line: u8,
    /// Special characters (`c_cc`).
    pub cc: [u8; 32],
    /// Input speed (`__c_ispeed`).
    pub ispeed: u32,
    /// Output speed (`__c_ospeed`).
    pub ospeed: u32,
}

impl Termios {
    /// The sane pty default: an 8-bit, 38400-baud, canonical, echoing RFC 4254 terminal with
    /// `ICRNL`, `IXON`, `ISIG`, `IEXTEN`, `OPOST` and `ONLCR`.
    #[must_use]
    pub const fn sane() -> Self {
        // `_POSIX_VDISABLE` is 0xff, so every slot without a default is explicitly disabled.
        let mut cc = [VDISABLE; 32];
        cc[VINTR] = CTRL_C;
        cc[VQUIT] = 0x1c;
        cc[VERASE] = DEL;
        cc[VKILL] = CTRL_U;
        cc[VEOF] = CTRL_D;
        cc[VSTART] = 0x11;
        cc[VSTOP] = 0x13;
        cc[VSUSP] = 0x1a;
        cc[VWERASE] = 0x17;
        cc[VMIN] = 1;
        cc[VTIME] = 0;
        Self {
            iflag: ICRNL | IXON,
            oflag: OPOST | ONLCR,
            cflag: CS8 | CREAD | B38400,
            lflag: ISIG | ICANON | ECHO | IEXTEN,
            line: 0,
            cc,
            ispeed: B38400,
            ospeed: B38400,
        }
    }

    /// Canonical/raw view of the record.
    #[must_use]
    pub const fn settings(&self) -> Settings {
        Settings {
            mode: if self.lflag & ICANON != 0 {
                Mode::Canonical
            } else {
                Mode::Raw
            },
            echo: self.lflag & ECHO != 0,
        }
    }

    /// Apply a canonical/raw view without touching the other fields.
    pub fn set_settings(&mut self, settings: Settings) {
        if settings.mode == Mode::Canonical {
            self.lflag |= ICANON;
        } else {
            self.lflag &= !ICANON;
        }
        if settings.echo {
            self.lflag |= ECHO;
        } else {
            self.lflag &= !ECHO;
        }
    }

    /// `c_cc[index]`, or [`VDISABLE`] when the index is outside the record.
    #[must_use]
    pub fn cc(&self, index: usize) -> u8 {
        self.cc.get(index).copied().unwrap_or(VDISABLE)
    }
}

impl Default for Termios {
    fn default() -> Self {
        Self::sane()
    }
}

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
    /// True when `^\` discarded the line and the server must deliver `SIGQUIT`.
    pub quit: bool,
}

/// The terminal window size (`struct winsize`).
///
/// The four fields are the BSD layout: `ws_row`, `ws_col`, `ws_xpixel`, `ws_ypixel`, all
/// `unsigned short`. A new pty starts at `0x0`, as in NetBSD; the pty stores one record per pair
/// and returns it unchanged on both the master and the slave.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Winsize {
    /// Rows, in characters.
    pub row: u16,
    /// Columns, in characters.
    pub col: u16,
    /// Horizontal size, in pixels.
    pub xpixel: u16,
    /// Vertical size, in pixels.
    pub ypixel: u16,
}

/// The line discipline state.
#[derive(Clone, Debug)]
pub struct Discipline {
    termios: Termios,
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
            termios: Termios::sane(),
            line: [0u8; MAX_LINE],
            len: 0,
        }
    }

    /// Current mode and echo switch.
    pub fn settings(&self) -> Settings {
        self.termios.settings()
    }

    /// The complete terminal attribute record.
    #[must_use]
    pub const fn termios(&self) -> Termios {
        self.termios
    }

    /// Replace the complete terminal attribute record, returning the number of pending input
    /// bytes discarded.
    ///
    /// A partially edited line cannot be carried across a canonical/raw switch (in raw mode it
    /// would be delivered as a line the reader never saw), so a mode change drops it. Every other
    /// field is stored verbatim, including bits this pty has no hardware to act on.
    pub fn set_termios(&mut self, termios: Termios) -> usize {
        let discarded = if termios.settings().mode == self.settings().mode {
            0
        } else {
            let pending = self.len;
            self.len = 0;
            pending
        };
        self.termios = termios;
        discarded
    }

    /// Apply new mode/echo settings, returning the number of pending input bytes discarded.
    pub fn set_settings(&mut self, settings: Settings) -> usize {
        let mut termios = self.termios;
        termios.set_settings(settings);
        self.set_termios(termios)
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

    /// Translate slave output through `OPOST`/`ONLCR`/`OCRNL`.
    ///
    /// This is the NetBSD `ttyoutput` rule set for the flags a scheme pty can honour. With
    /// `OPOST` clear the bytes are passed through unchanged.
    pub fn process_output(&self, data: &[u8], out: &mut Vec<u8>) {
        if self.termios.oflag & OPOST == 0 {
            out.extend_from_slice(data);
            return;
        }
        for &byte in data {
            if byte == b'\n' && self.termios.oflag & ONLCR != 0 {
                out.extend_from_slice(b"\r\n");
            } else if byte == b'\r' && self.termios.oflag & OCRNL != 0 {
                out.push(b'\n');
            } else {
                out.push(byte);
            }
        }
    }

    /// Feed console input through the discipline.
    pub fn feed(&mut self, input: &[u8], out: &mut Output) {
        for &raw in input {
            if self.termios.cflag & CREAD == 0 {
                continue;
            }
            let mut byte = raw;
            if self.termios.iflag & ISTRIP != 0 {
                byte &= 0x7f;
            }
            if self.termios.iflag & IXON != 0 {
                if self.special(VSTOP, byte) {
                    continue;
                }
                if self.special(VSTART, byte) {
                    continue;
                }
            }
            if byte == b'\r' {
                if self.termios.iflag & IGNCR != 0 {
                    continue;
                }
                if self.termios.iflag & ICRNL != 0 {
                    byte = NEWLINE;
                }
            } else if byte == NEWLINE && self.termios.iflag & INLCR != 0 {
                byte = b'\r';
            }

            // ISIG is independent of canonical mode: FreeBSD `ttyinput` tests the signal
            // characters before the canonical/raw branch, so ^C still interrupts in a raw
            // terminal unless cfmakeraw cleared ISIG.
            if self.termios.lflag & ISIG != 0 && self.special(VINTR, byte) {
                self.interrupt(out);
                continue;
            }
            if self.termios.lflag & ISIG != 0 && self.special(VQUIT, byte) {
                self.quit(out);
                continue;
            }

            // Raw mode skips editing and echo, but the input translation flags above still apply,
            // as in the BSD line discipline (cfmakeraw clears those flags for a true byte pipe).
            if self.termios.settings().mode == Mode::Raw {
                out.data.push(byte);
                continue;
            }
            if byte == NEWLINE || (self.special(VEOL, byte) && self.termios.lflag & IEXTEN != 0) {
                self.complete_line(out);
                continue;
            }
            if self.special(VWERASE, byte) {
                self.werase(out);
                continue;
            }
            if self.special(VERASE, byte) || byte == BACKSPACE || byte == DEL {
                self.erase(out);
                continue;
            }
            if self.special(VKILL, byte) {
                self.kill_line(out);
                continue;
            }
            if self.special(VEOF, byte) {
                self.eof_or_complete(out);
                continue;
            }
            if self.termios.iflag & IMAXBEL != 0 && self.len == MAX_LINE {
                if self.termios.lflag & ECHO != 0 {
                    out.echo.push(BELL);
                }
                continue;
            }
            self.insert(byte, out);
        }
    }

    fn special(&self, index: usize, byte: u8) -> bool {
        let value = self.termios.cc(index);
        value != VDISABLE && value == byte
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
        if self.termios.lflag & ECHO != 0 {
            out.echo.extend_from_slice(ERASE);
        }
    }

    fn werase(&mut self, out: &mut Output) {
        let mut erased = 0usize;
        while self.len > 0 && matches!(self.line[self.len - 1], b' ' | b'\t') {
            self.len -= 1;
            erased += 1;
        }
        while self.len > 0 && !matches!(self.line[self.len - 1], b' ' | b'\t') {
            self.len -= 1;
            erased += 1;
        }
        if self.termios.lflag & ECHO != 0 {
            for _ in 0..erased {
                out.echo.extend_from_slice(ERASE);
            }
        }
    }

    fn kill_line(&mut self, out: &mut Output) {
        if self.len == 0 {
            return;
        }
        let erased = self.len;
        self.len = 0;
        if self.termios.lflag & ECHO != 0 {
            for _ in 0..erased {
                out.echo.extend_from_slice(ERASE);
            }
        }
    }

    fn interrupt(&mut self, out: &mut Output) {
        self.len = 0;
        if self.termios.lflag & ECHO != 0 {
            out.echo.extend_from_slice(b"^C");
            out.echo.extend_from_slice(b"\r\n");
        }
        out.events.push(Event::Interrupt);
    }

    fn quit(&mut self, out: &mut Output) {
        self.len = 0;
        if self.termios.lflag & ECHO != 0 {
            out.echo.extend_from_slice(b"^\\");
            out.echo.extend_from_slice(b"\r\n");
        }
        out.quit = true;
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
            if self.termios.lflag & ECHO != 0 {
                out.echo.push(BELL);
            }
            return;
        }
        self.line[self.len] = byte;
        self.len += 1;
        if self.termios.lflag & ECHO != 0 {
            out.echo.push(byte);
        }
    }

    fn echo_terminator(&mut self, out: &mut Output) {
        if self.termios.lflag & ECHO != 0 {
            out.echo.extend_from_slice(b"\r\n");
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
