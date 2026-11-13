// SPDX-License-Identifier: ISC
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 2000-2002, 2007, 2010 Todd C. Miller <millert@openbsd.org>
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/readpassphrase.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: readpassphrase.c,v 1.32 2026/10/15 16:40:50 millert Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// Sponsored in part by the Defense Advanced Research Projects Agency
// (DARPA) and Air Force Research Laboratory, Air Force Materiel Command,
// USAF, under agreement number F39502-99-1-0512.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `readpassphrase` (donor-map 15 T3, 02 B13).
//!
//! The tty is `/dev/tty` through [`TtyOps::open_tty`]. Echo is turned off before the first
//! read, the prompt goes to the output descriptor, and the old terminal attributes are restored
//! on every exit path. The donor installs signal handlers and re-raises the signal after the
//! restore; here the seam exposes [`TtyOps::pending_signal`], the read loop stops on a signal,
//! restores the terminal and returns `None`; the shell re-raises the signal. `bufsiz == 0`
//! returns `None` (the shell sets `EINVAL`), as do `RPP_REQUIRE_TTY` without a tty (`ENOTTY`).
//!
//! `VSTATUS` is a BSD `termios` field that the Linux-layout `Termios` of C36 does not have; the
//! donor's "disable VSTATUS" line is not translated (documented in the C65 report).

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use raven_libc_abi::generated::structs::Termios;

use crate::seams::TtyOps;

/// Do not echo the input.
pub const RPP_ECHO_OFF: i32 = 0x00;
/// Echo the input.
pub const RPP_ECHO_ON: i32 = 0x01;
/// Require a controlling terminal.
pub const RPP_REQUIRE_TTY: i32 = 0x02;
/// Fold alphabetic characters to lower case.
pub const RPP_FORCELOWER: i32 = 0x04;
/// Fold alphabetic characters to upper case.
pub const RPP_FORCEUPPER: i32 = 0x08;
/// Strip the eighth bit of every character.
pub const RPP_SEVENBIT: i32 = 0x10;
/// Read from standard input, never open `/dev/tty`.
pub const RPP_STDIN: i32 = 0x20;

const ECHO: u32 = 0o0000010;
const ECHONL: u32 = 0o0000100;
const STDIN_FILENO: i32 = 0;
const STDERR_FILENO: i32 = 2;

/// `readpassphrase`.
///
/// Returns the number of characters stored in `buf` (the shell adds the NUL), or `None` on
/// error, on a signal or when the terminal is required but absent.
pub fn readpassphrase<T: TtyOps>(
    tty: &mut T,
    prompt: &[u8],
    buf: &mut [u8],
    flags: i32,
) -> Option<usize> {
    if buf.is_empty() {
        return None;
    }
    let (input, output, opened) = if flags & RPP_STDIN != 0 {
        (STDIN_FILENO, STDERR_FILENO, false)
    } else {
        match tty.open_tty() {
            Ok(fd) => (fd, fd, true),
            Err(_) => {
                if flags & RPP_REQUIRE_TTY != 0 {
                    return None;
                }
                (STDIN_FILENO, STDERR_FILENO, false)
            }
        }
    };

    let mut original = Termios::ZERO;
    let mut current = Termios::ZERO;
    let mut echo_was_on = true;
    if input != STDIN_FILENO {
        if let Ok(attributes) = tty.tcgetattr(input) {
            original = attributes;
            current = attributes;
            if flags & RPP_ECHO_ON == 0 {
                current.c_lflag &= !(ECHO | ECHONL);
            }
            let _ = tty.tcsetattr(input, &current);
            echo_was_on = current.c_lflag & ECHO != 0;
        } else {
            current.c_lflag |= ECHO;
            original.c_lflag |= ECHO;
        }
    } else {
        current.c_lflag |= ECHO;
        original.c_lflag |= ECHO;
    }

    if flags & RPP_STDIN == 0 {
        let _ = tty.write(output, prompt);
    }

    let end = buf.len().wrapping_sub(1);
    let mut written = 0usize;
    let mut interrupted = false;
    loop {
        if tty.pending_signal().is_some() {
            interrupted = true;
            break;
        }
        match tty.read_byte(input) {
            Ok(Some(mut ch)) => {
                if ch == b'\n' || ch == b'\r' {
                    break;
                }
                if written < end {
                    if flags & RPP_SEVENBIT != 0 {
                        ch &= 0x7f;
                    }
                    if ch.is_ascii_alphabetic() {
                        if flags & RPP_FORCELOWER != 0 {
                            ch = ch.to_ascii_lowercase();
                        }
                        if flags & RPP_FORCEUPPER != 0 {
                            ch = ch.to_ascii_uppercase();
                        }
                    }
                    buf[written] = ch;
                    written = written.wrapping_add(1);
                }
            }
            Ok(None) => break,
            Err(_) => {
                interrupted = true;
                break;
            }
        }
    }
    buf[written] = 0;

    // The donor writes the newline when the *new* terminal flags have echo off.
    if !echo_was_on && !interrupted {
        let _ = tty.write(output, b"\n");
    }
    if current != original {
        let _ = tty.tcsetattr(input, &original);
    }
    if opened && input != STDIN_FILENO {
        tty.close(input);
    }
    if interrupted {
        return None;
    }
    Some(written)
}

#[cfg(test)]
#[path = "readpassphrase_ut.rs"]
mod tests;
