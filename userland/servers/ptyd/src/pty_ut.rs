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
fn hangup_makes_empty_reads_return_eof() {
    let mut pty = pty();
    pty.hangup();
    assert!(pty.is_eof());
    assert_eq!(pty.read_master(&mut [0u8; 4]), Ok(0));
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
