// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Termios glue tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::ioctl::tty::{
    B38400, ECHO, ICANON, OP_GET_FG, OP_SET_FG, VMIN, VTIME, Winsize, cfgetispeed, cfgetospeed,
    cfmakeraw, cfsetispeed, cfsetospeed, custom_arg0, custom_body, default_termios, tcdrain,
    tcflush, tcgetattr, tcgetwinsize, tcsetattr, tcsetwinsize,
};
use crate::platform::Platform;

fn tty_platform() -> (Platform<FakeSys>, i32) {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    (platform, fd)
}

#[test]
fn tcgetattr_returns_canonical_echo() {
    let (platform, fd) = tty_platform();
    let termios = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_ne!(termios.c_lflag & ICANON, 0);
    assert_ne!(termios.c_lflag & ECHO, 0);
    assert_eq!(termios.c_cc[VMIN], 1);
}

#[test]
fn tcsetattr_switches_to_raw() {
    let (platform, fd) = tty_platform();
    let mut termios = default_termios();
    cfmakeraw(&mut termios);
    tcsetattr(&platform, fd, &termios).expect("tcsetattr");
    let read_back = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(read_back.c_lflag & ICANON, 0);
    assert_eq!(read_back.c_lflag & ECHO, 0);
}

#[test]
fn tcsetattr_clears_echo_only() {
    let (platform, fd) = tty_platform();
    let mut termios = default_termios();
    termios.c_lflag &= !ECHO;
    tcsetattr(&platform, fd, &termios).expect("tcsetattr");
    let read_back = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_ne!(read_back.c_lflag & ICANON, 0);
    assert_eq!(read_back.c_lflag & ECHO, 0);
}

#[test]
fn a_legacy_console_falls_back_to_get_mode_and_set_mode() {
    let fake = FakeSys::new();
    fake.tty_legacy(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    let before = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_ne!(before.c_lflag & ICANON, 0);
    assert_ne!(before.c_lflag & ECHO, 0);

    let mut raw = default_termios();
    cfmakeraw(&mut raw);
    tcsetattr(&platform, fd, &raw).expect("tcsetattr");
    let after = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(after.c_lflag & ICANON, 0);
    assert_eq!(after.c_lflag & ECHO, 0);
}

#[test]
fn custom_fg_body_round_trips_the_pgrp() {
    let body = custom_body(OP_GET_FG, 0);
    assert_eq!(
        u16::from_le_bytes(body[0..2].try_into().unwrap()),
        raven_proto_tty::VERSION
    );
    assert_eq!(
        u16::from_le_bytes(body[2..4].try_into().unwrap()),
        OP_GET_FG
    );
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    response[0..2].copy_from_slice(&raven_proto_tty::VERSION.to_le_bytes());
    response[2..4].copy_from_slice(&OP_GET_FG.to_le_bytes());
    response[4..8].copy_from_slice(&4242u32.to_le_bytes());
    assert_eq!(custom_arg0(&response, OP_GET_FG), Ok(4242));
}

#[test]
fn custom_fg_rejects_the_wrong_op() {
    let mut response = [0u8; raven_proto_tty::CALL_LEN];
    response[0..2].copy_from_slice(&raven_proto_tty::VERSION.to_le_bytes());
    response[2..4].copy_from_slice(&OP_SET_FG.to_le_bytes());
    assert_eq!(custom_arg0(&response, OP_GET_FG), Err(Errno::EPROTO));
}

#[test]
fn custom_fg_rejects_a_short_response() {
    assert_eq!(custom_arg0(&[], OP_GET_FG), Err(Errno::EPROTO));
}

#[test]
fn winsize_round_trips_all_four_fields() {
    let (platform, fd) = tty_platform();
    let first = Winsize {
        row: 43,
        col: 120,
        xpixel: 0,
        ypixel: 0,
    };
    tcsetwinsize(&platform, fd, &first).expect("tcsetwinsize");
    assert_eq!(tcgetwinsize(&platform, fd), Ok(first));

    let second = Winsize {
        row: 24,
        col: 80,
        xpixel: 640,
        ypixel: 480,
    };
    tcsetwinsize(&platform, fd, &second).expect("tcsetwinsize");
    assert_eq!(tcgetwinsize(&platform, fd), Ok(second));

    let zeros = Winsize::default();
    tcsetwinsize(&platform, fd, &zeros).expect("tcsetwinsize");
    assert_eq!(tcgetwinsize(&platform, fd), Ok(zeros));
}

#[test]
fn a_fresh_tty_starts_at_zero_winsize() {
    let (platform, fd) = tty_platform();
    assert_eq!(tcgetwinsize(&platform, fd), Ok(Winsize::default()));
}

#[test]
fn winsize_on_a_non_tty_is_enotty() {
    let fake = FakeSys::new();
    fake.file(b"/plain", b"x");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/plain", fcntl::O_RDONLY as u32, 0)
        .expect("open");
    assert_eq!(tcgetwinsize(&platform, fd), Err(Errno::ENOTTY));
    assert_eq!(
        tcsetwinsize(&platform, fd, &Winsize::default()),
        Err(Errno::ENOTTY)
    );
}

#[test]
fn a_legacy_console_winsize_is_enotty_not_enosys() {
    let fake = FakeSys::new();
    fake.tty_legacy(b"/dev/tty");
    let platform = Platform::new(fake);
    let fd = platform
        .open(b"/dev/tty", fcntl::O_RDWR as u32, 0)
        .expect("open");
    assert_eq!(tcgetwinsize(&platform, fd), Err(Errno::ENOTTY));
    assert_eq!(
        tcsetwinsize(&platform, fd, &Winsize::default()),
        Err(Errno::ENOTTY)
    );
}

#[test]
fn tcflush_succeeds_on_a_tty() {
    let (platform, fd) = tty_platform();
    assert_eq!(tcflush(&platform, fd, 0), Ok(()));
}

#[test]
fn tcdrain_is_enosys_until_pg_54() {
    let (platform, fd) = tty_platform();
    assert_eq!(tcdrain(&platform, fd), Err(Errno::ENOSYS));
}

#[test]
fn cfmakeraw_clears_the_input_flags() {
    let mut termios = default_termios();
    termios.c_iflag = 0xffff;
    termios.c_oflag = 0xffff;
    cfmakeraw(&mut termios);
    assert_eq!(termios.c_iflag, 0);
    assert_eq!(termios.c_oflag, 0);
    assert_eq!(termios.c_lflag, 0);
    assert_eq!(termios.c_cc[VMIN], 1);
    assert_eq!(termios.c_cc[VTIME], 0);
}

#[test]
fn speed_accessors_round_trip() {
    let mut termios = default_termios();
    assert_eq!(cfgetispeed(&termios), B38400);
    assert_eq!(cfgetospeed(&termios), B38400);
    assert_eq!(cfsetispeed(&mut termios, 9600), 0);
    assert_eq!(cfsetospeed(&mut termios, 4800), 0);
    assert_eq!(cfgetispeed(&termios), 9600);
    assert_eq!(cfgetospeed(&termios), 4800);
}

#[test]
fn default_termios_has_cread_and_cs8() {
    let termios = default_termios();
    use crate::ioctl::tty::{CREAD, CS8};
    assert_ne!(termios.c_cflag & CS8, 0);
    assert_ne!(termios.c_cflag & CREAD, 0);
}

/// The exact OpenSSH `pty_req` mode word shape: ICRNL, IXON, ISIG, ICANON, ECHO, IEXTEN,
/// OPOST/ONLCR, CS8, VMIN/VTIME and both speeds. `tcsetattr`/`tcgetattr` must preserve every
/// field; K444 is red when any one of them is dropped.
#[test]
fn tcsetattr_round_trips_every_termios_field() {
    const IGNBRK: u32 = 0o0000001;
    const ICRNL: u32 = 0o0000400;
    const IXON: u32 = 0o0002000;
    const OPOST: u32 = 0o0000001;
    const ONLCR: u32 = 0o0000004;
    const ISIG: u32 = 0o0000001;
    const IEXTEN: u32 = 0o100000;
    const VINTR: usize = 0;
    const VERASE: usize = 2;
    const VKILL: usize = 3;
    const VEOF: usize = 4;
    const VSTART: usize = 8;
    const VSTOP: usize = 9;
    const B9600: u32 = 0o15;
    const B19200: u32 = 0o16;

    let (platform, fd) = tty_platform();
    let mut want = default_termios();
    want.c_iflag = IGNBRK | ICRNL | IXON;
    want.c_oflag = OPOST | ONLCR;
    want.c_cflag = crate::ioctl::tty::CS8 | crate::ioctl::tty::CREAD | B38400;
    want.c_lflag = ISIG | ICANON | ECHO | IEXTEN;
    want.c_line = 0;
    want.c_cc[VINTR] = 3;
    want.c_cc[VERASE] = 0x7f;
    want.c_cc[VKILL] = 0x15;
    want.c_cc[VEOF] = 0x04;
    want.c_cc[VSTART] = 0x11;
    want.c_cc[VSTOP] = 0x13;
    want.c_cc[VMIN] = 7;
    want.c_cc[VTIME] = 3;
    want.__c_ispeed = B9600;
    want.__c_ospeed = B19200;

    tcsetattr(&platform, fd, &want).expect("tcsetattr");
    let got = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(got, want);
}
