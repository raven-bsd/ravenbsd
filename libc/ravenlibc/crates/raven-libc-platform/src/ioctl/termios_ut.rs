// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Termios glue tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use crate::backend::FakeSys;
use crate::ioctl::tty::{
    B38400, ECHO, ICANON, VMIN, VTIME, cfgetispeed, cfgetospeed, cfmakeraw, cfsetispeed,
    cfsetospeed, default_termios, tcdrain, tcflush, tcgetattr, tcsetattr,
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
