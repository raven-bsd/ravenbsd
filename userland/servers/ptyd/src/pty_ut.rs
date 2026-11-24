// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pty queues and line discipline.

use alloc::vec::Vec;

use super::*;
use raven_tty_core::Mode;

#[derive(Default)]
struct Sink {
    signals: Vec<(i32, u32)>,
}

impl SignalSink for Sink {
    fn signal(&mut self, pgrp: i32, signo: u32) -> Result<(), Errno> {
        self.signals.push((pgrp, signo));
        Ok(())
    }
}

fn pty() -> Pty<Sink> {
    Pty::new(Sink::default())
}

fn read_master(pty: &mut Pty<Sink>) -> Vec<u8> {
    let mut buffer = [0u8; 64];
    let count = pty.read_master(&mut buffer).expect("master read");
    buffer[..count].to_vec()
}

fn read_slave(pty: &mut Pty<Sink>) -> Vec<u8> {
    let mut buffer = [0u8; 64];
    let count = pty.read_slave(&mut buffer).expect("slave read");
    buffer[..count].to_vec()
}

#[test]
fn canonical_input_is_line_buffered_and_echoed() {
    let mut pty = pty();
    pty.write_master(b"hello\r");
    assert_eq!(read_master(&mut pty), b"hello\r\n");
    assert_eq!(read_slave(&mut pty), b"hello\n");
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Err(Errno::EAGAIN));
}

#[test]
fn raw_mode_passes_bytes_without_echo() {
    let mut pty = pty();
    pty.set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    pty.write_master(b"raw");
    assert_eq!(pty.read_master(&mut [0u8; 4]), Err(Errno::EAGAIN));
    assert_eq!(read_slave(&mut pty), b"raw");
}

#[test]
fn ctrl_c_signals_the_foreground_group_and_echoes() {
    let mut pty = pty();
    pty.set_fg(42);
    pty.write_master(b"\x03");
    assert_eq!(pty.sys().signals, vec![(-42, SIGINT)]);
    assert_eq!(read_master(&mut pty), b"^C\r\n");
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Err(Errno::EAGAIN));
}

#[test]
fn ctrl_backslash_signals_sigquit_to_the_foreground_group() {
    let mut pty = pty();
    pty.set_fg(42);
    pty.write_master(b"\x1c");
    assert_eq!(pty.sys().signals, vec![(-42, SIGQUIT)]);
    assert_eq!(read_master(&mut pty), b"^\\\r\n");
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Err(Errno::EAGAIN));
}

#[test]
fn echo_off_and_flush_drop_the_expected_bytes() {
    let mut pty = pty();
    pty.set_settings(Settings {
        mode: Mode::Canonical,
        echo: false,
    });
    pty.write_master(b"quiet\r");
    assert_eq!(pty.read_master(&mut [0u8; 4]), Err(Errno::EAGAIN));
    assert_eq!(read_slave(&mut pty), b"quiet\n");

    pty.write_master(b"pending");
    assert_eq!(pty.flush(), 7);
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Err(Errno::EAGAIN));
}

#[test]
fn hangup_makes_master_reads_eio_and_slave_reads_eof() {
    // FreeBSD sys/kern/tty_pts.c:223 returns EIO for the master once the slave side is
    // finished; the slave keeps the EOF (0) contract.
    let mut pty = pty();
    pty.hangup();
    assert!(pty.is_eof());
    assert_eq!(pty.read_master(&mut [0u8; 4]), Err(Errno::EIO));
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Ok(0));
}

#[test]
fn slave_output_is_readable_on_the_master() {
    let mut pty = pty();
    pty.write_slave(b"output");
    assert_eq!(read_master(&mut pty), b"output");
}

impl Pty<Sink> {
    fn sys(&self) -> &Sink {
        &self.sink
    }
}

#[test]
fn the_complete_termios_round_trips() {
    let mut pty = pty();
    let mut want = raven_tty_core::Termios::sane();
    want.iflag = raven_tty_core::ICRNL | raven_tty_core::IXON;
    want.oflag = raven_tty_core::OPOST | raven_tty_core::ONLCR;
    want.cc[raven_tty_core::VMIN] = 7;
    want.cc[raven_tty_core::VTIME] = 3;
    want.ispeed = 0o15;
    want.ospeed = 0o16;
    assert_eq!(pty.set_termios(want), 0);
    assert_eq!(pty.termios(), want);
    assert_eq!(pty.settings(), want.settings());
}

#[test]
fn output_processing_obeys_onlcr() {
    let mut pty = pty();
    pty.write_slave(b"line\n");
    assert_eq!(read_master(&mut pty), b"line\r\n");

    let mut raw = raven_tty_core::Termios::sane();
    raw.oflag &= !raven_tty_core::OPOST;
    pty.set_termios(raw);
    pty.write_slave(b"line\n");
    assert_eq!(read_master(&mut pty), b"line\n");
}

#[test]
fn a_drain_set_does_not_discard_queued_output() {
    let mut pty = pty();
    pty.write_slave(b"pending\n");
    let mut next = raven_tty_core::Termios::sane();
    next.cc[raven_tty_core::VMIN] = 9;
    next.lflag &= !raven_tty_core::ECHO;
    pty.set_termios(next);
    assert_eq!(read_master(&mut pty), b"pending\r\n");
}

#[test]
fn a_fresh_pty_starts_at_zero_winsize() {
    let pty = pty();
    assert_eq!(pty.winsize(), raven_tty_core::Winsize::default());
}

#[test]
fn the_window_size_is_shared_by_master_and_slave() {
    let mut pty = pty();
    let first = raven_tty_core::Winsize {
        row: 43,
        col: 120,
        xpixel: 0,
        ypixel: 0,
    };
    pty.set_winsize(first);
    assert_eq!(pty.winsize(), first);
    let second = raven_tty_core::Winsize {
        row: 24,
        col: 80,
        xpixel: 640,
        ypixel: 480,
    };
    pty.set_winsize(second);
    assert_eq!(pty.winsize(), second);
}

#[test]
fn a_winsize_change_signals_the_foreground_group_once() {
    let mut pty = pty();
    pty.set_fg(42);
    let first = raven_tty_core::Winsize {
        row: 43,
        col: 120,
        xpixel: 0,
        ypixel: 0,
    };
    pty.set_winsize(first);
    assert_eq!(pty.sys().signals, vec![(-42, SIGWINCH)]);
    pty.set_winsize(first);
    assert_eq!(pty.sys().signals, vec![(-42, SIGWINCH)]);
    let second = raven_tty_core::Winsize {
        row: 24,
        col: 80,
        xpixel: 640,
        ypixel: 480,
    };
    pty.set_winsize(second);
    assert_eq!(pty.sys().signals, vec![(-42, SIGWINCH), (-42, SIGWINCH)]);
}

#[test]
fn a_winsize_change_without_a_foreground_group_signals_nothing() {
    let mut pty = pty();
    pty.set_winsize(raven_tty_core::Winsize {
        row: 43,
        col: 120,
        xpixel: 0,
        ypixel: 0,
    });
    assert!(pty.sys().signals.is_empty());
}

#[test]
fn a_winsize_change_after_hangup_signals_nothing() {
    let mut pty = pty();
    pty.set_fg(42);
    pty.hangup();
    pty.set_winsize(raven_tty_core::Winsize {
        row: 43,
        col: 120,
        xpixel: 0,
        ypixel: 0,
    });
    assert!(pty.sys().signals.is_empty());
}

#[test]
fn a_custom_verase_byte_edits_the_line() {
    let mut pty = pty();
    let mut want = raven_tty_core::Termios::sane();
    want.cc[raven_tty_core::VERASE] = 0x18;
    want.lflag &= !raven_tty_core::ECHO;
    pty.set_termios(want);
    pty.write_master(b"ab\x18c\n");
    assert_eq!(read_slave(&mut pty), b"ac\n");
}

#[test]
fn master_read_drains_then_returns_eio_after_hangup() {
    // FreeBSD sys/kern/tty_pts.c:110 ptsdev_read returns EIO at :223 once the slave side is
    // finished; queued master data drains first. The slave reader keeps the EOF (0) contract.
    let mut pty = pty();
    pty.write_slave(b"last");
    pty.hangup();
    assert_eq!(read_master(&mut pty), b"last");
    assert_eq!(pty.read_master(&mut [0u8; 4]), Err(Errno::EIO));
    assert_eq!(pty.read_slave(&mut [0u8; 4]), Ok(0));
}
