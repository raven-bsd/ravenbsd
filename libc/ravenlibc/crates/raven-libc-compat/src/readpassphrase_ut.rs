// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `readpassphrase` tests with the scripted `FakeTty`: echo restore on every exit path,
//! `RPP_REQUIRE_TTY`, the flag folding, the 1-byte buffer and EOF.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;
use crate::testutil::FakeTty;
use raven_libc_abi::errno::Errno;

fn buffer() -> [u8; 32] {
    [0u8; 32]
}

#[test]
fn simple_password_with_prompt() {
    let mut tty = FakeTty::default();
    tty.input = b"secret\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"Password: ", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(&buf[..n], b"secret");
    assert_eq!(buf[n], 0);
    assert_eq!(tty.writes[0], (7, b"Password: ".to_vec()));
    // Echo was cleared and restored.
    assert_eq!(tty.lflag_log.len(), 2);
    assert_eq!(tty.lflag_log[0] & ECHO, 0);
    assert_eq!(tty.lflag_log[1] & ECHO, ECHO);
}

#[test]
fn echo_stays_on_with_rpp_echo_on() {
    let mut tty = FakeTty::default();
    tty.input = b"abc\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_ON).unwrap();
    assert_eq!(&buf[..n], b"abc");
    assert_eq!(tty.lflag_log[0] & ECHO, ECHO);
}

#[test]
fn line_ends_at_carriage_return_too() {
    let mut tty = FakeTty::default();
    tty.input = b"abc\rdef\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(&buf[..n], b"abc");
}

#[test]
fn sevenbit_and_case_folding() {
    let mut tty = FakeTty::default();
    tty.input = b"A\xe9Z\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(
        &mut tty,
        b"",
        &mut buf,
        RPP_ECHO_OFF | RPP_SEVENBIT | RPP_FORCELOWER,
    )
    .unwrap();
    assert_eq!(&buf[..n], b"aiz");
    let mut tty = FakeTty::default();
    tty.input = b"abz\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF | RPP_FORCEUPPER).unwrap();
    assert_eq!(&buf[..n], b"ABZ");
}

#[test]
fn one_byte_buffer_holds_only_the_terminator() {
    let mut tty = FakeTty::default();
    tty.input = b"abc\n".to_vec();
    let mut buf = [0u8; 1];
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(n, 0);
    assert_eq!(buf[0], 0);
}

#[test]
fn empty_buffer_is_none() {
    let mut tty = FakeTty::default();
    let mut buf: [u8; 0] = [];
    assert_eq!(readpassphrase(&mut tty, b"", &mut buf, 0), None);
}

#[test]
fn buffer_overflow_is_consumed_and_dropped() {
    let mut tty = FakeTty::default();
    tty.input = b"abcdef\n".to_vec();
    let mut buf = [0u8; 4];
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(&buf[..n], b"abc");
    assert_eq!(buf[n], 0);
}

#[test]
fn eof_returns_the_partial_buffer() {
    let mut tty = FakeTty::default();
    tty.input = b"ab".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(&buf[..n], b"ab");
}

#[test]
fn eof_with_no_data_returns_the_empty_buffer() {
    let mut tty = FakeTty::default();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(n, 0);
}

#[test]
fn eof_restores_echo() {
    let mut tty = FakeTty::default();
    tty.input = Vec::new();
    let mut buf = buffer();
    readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF);
    assert_eq!(tty.lflag_log.last().copied().unwrap_or(0) & ECHO, ECHO);
}

#[test]
fn require_tty_without_a_tty_is_none() {
    let mut tty = FakeTty::default();
    tty.open_error = Some(Errno(raven_libc_abi::errno::ENOENT as u16));
    let mut buf = buffer();
    assert_eq!(
        readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF | RPP_REQUIRE_TTY),
        None
    );
}

#[test]
fn without_require_tty_it_falls_back_to_stdin() {
    let mut tty = FakeTty::default();
    tty.open_error = Some(Errno(raven_libc_abi::errno::ENOENT as u16));
    tty.input = b"x\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"p", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(&buf[..n], b"x");
    // The donor still writes the prompt, to stderr, before reading stdin.
    assert_eq!(tty.writes, alloc::vec![(2, b"p".to_vec())]);
}

#[test]
fn rpp_stdin_never_opens_the_tty() {
    let mut tty = FakeTty::default();
    tty.input = b"x\n".to_vec();
    let mut buf = buffer();
    let n = readpassphrase(&mut tty, b"p", &mut buf, RPP_STDIN).unwrap();
    assert_eq!(&buf[..n], b"x");
    assert!(tty.writes.is_empty());
}

#[test]
fn a_signal_restores_the_terminal_and_returns_none() {
    let mut tty = FakeTty::default();
    tty.input = b"abc".to_vec();
    tty.signals = alloc::vec![2];
    let mut buf = buffer();
    assert_eq!(readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF), None);
    assert_eq!(tty.lflag_log.last().copied().unwrap_or(0) & ECHO, ECHO);
    // The prompt is written; a signal exit does not print the newline.
    assert_eq!(tty.writes.len(), 1);
    assert_eq!(tty.writes[0].1, b"");
}

#[test]
fn signal_after_some_input_still_restores_and_returns_none() {
    let mut tty = FakeTty::default();
    tty.input = b"abc".to_vec();
    // The first read returns a byte; the signal is observed before the second read.
    let mut buf = buffer();
    let result = {
        struct Once<'a> {
            tty: &'a mut FakeTty,
            fired: bool,
        }
        impl TtyOps for Once<'_> {
            fn open_tty(&mut self) -> Result<i32, Errno> {
                self.tty.open_tty()
            }
            fn tcgetattr(&mut self, fd: i32) -> Result<Termios, Errno> {
                self.tty.tcgetattr(fd)
            }
            fn tcsetattr(&mut self, fd: i32, t: &Termios) -> Result<(), Errno> {
                self.tty.tcsetattr(fd, t)
            }
            fn write(&mut self, fd: i32, b: &[u8]) -> Result<usize, Errno> {
                self.tty.write(fd, b)
            }
            fn read_byte(&mut self, fd: i32) -> Result<Option<u8>, Errno> {
                self.tty.read_byte(fd)
            }
            fn close(&mut self, fd: i32) {
                self.tty.close(fd);
            }
            fn pending_signal(&mut self) -> Option<i32> {
                if self.fired {
                    Some(15)
                } else {
                    self.fired = true;
                    None
                }
            }
        }
        let mut wrapper = Once {
            tty: &mut tty,
            fired: false,
        };
        readpassphrase(&mut wrapper, b"", &mut buf, RPP_ECHO_OFF)
    };
    assert_eq!(result, None);
    assert_eq!(&buf[..1], b"a");
    assert_eq!(tty.lflag_log.last().copied().unwrap_or(0) & ECHO, ECHO);
}

#[test]
fn close_happens_only_when_the_tty_was_opened() {
    let mut tty = FakeTty::default();
    tty.input = b"x\n".to_vec();
    let mut buf = buffer();
    readpassphrase(&mut tty, b"", &mut buf, RPP_ECHO_OFF).unwrap();
    assert_eq!(tty.closes, 1);
    let mut tty = FakeTty::default();
    tty.input = b"x\n".to_vec();
    readpassphrase(&mut tty, b"", &mut buf, RPP_STDIN).unwrap();
    assert_eq!(tty.closes, 0);
}
