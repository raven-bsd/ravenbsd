// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the raw-termios flag port and the non-TTY rejection.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;

use super::{CS8, CSIZE, Error, PARENB, RawMode, Termios};

#[test]
fn termios_is_the_c_60_byte_shape() {
    assert_eq!(core::mem::size_of::<Termios>(), 60);
    assert_eq!(core::mem::align_of::<Termios>(), 4);
}

#[test]
fn make_raw_matches_the_netbsd_cfmakeraw_flags() {
    let mut termios = Termios {
        c_iflag: u32::MAX,
        c_oflag: u32::MAX,
        c_cflag: u32::MAX,
        c_lflag: u32::MAX,
        c_line: 0,
        c_cc: [0; 32],
        __c_ispeed: 0,
        __c_ospeed: 0,
    };
    termios.make_raw();
    // IMAXBEL|IGNBRK|BRKINT|PARMRK|ISTRIP|INLCR|IGNCR|ICRNL|IXON are clear.
    assert_eq!(termios.c_iflag & 0o022_753, 0);
    // OPOST is clear.
    assert_eq!(termios.c_oflag & 0o000_001, 0);
    // ECHO|ECHONL|ICANON|ISIG|IEXTEN are clear.
    assert_eq!(termios.c_lflag & 0o100_113, 0);
    // CSIZE/PARENB are clear; CS8 is selected.
    assert_eq!(termios.c_cflag & CSIZE, CS8);
    assert_eq!(termios.c_cflag & PARENB, 0);
    // The 8-bit data path keeps the unrelated control bits (for example CREAD).
    assert_ne!(termios.c_cflag & 0o000_200, 0);
}

#[test]
fn a_non_tty_descriptor_is_rejected_without_claiming_the_guard() {
    let (left, _right) = UnixStream::pair().expect("socketpair");
    let error = RawMode::enable(left.as_raw_fd()).expect_err("socketpair is not a terminal");
    assert_eq!(error, Error::GetAttr);
    // A second attempt reaches tcgetattr again, not the busy path.
    let error = RawMode::enable(left.as_raw_fd()).expect_err("socketpair is not a terminal");
    assert_eq!(error, Error::GetAttr);
}
