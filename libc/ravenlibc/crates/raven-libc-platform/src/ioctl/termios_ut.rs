// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Termios glue tests.

use raven_abi::errno::Errno;
use raven_libc_abi::fcntl;

use super::{CC_MAP, SPEED_MAP, cc_from_wire, cc_to_wire, termios_from_wire, wire_from_termios};
use crate::backend::FakeSys;
use crate::ioctl::tty::{
    B38400, ECHO, ICANON, OP_GET_FG, OP_SET_FG, VEOF, VINTR, VMIN, VTIME, Winsize, cfgetispeed,
    cfgetospeed, cfmakeraw, cfsetispeed, cfsetospeed, custom_arg0, custom_body, default_termios,
    tcdrain, tcflush, tcgetattr, tcgetwinsize, tcsetattr, tcsetwinsize,
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
fn public_bsd_indexes_read_the_wire_control_characters() {
    let termios = default_termios();
    assert_eq!(termios.c_cc[VINTR], 3, "public VINTR reads the wire ^C");
    assert_eq!(termios.c_cc[VEOF], 4, "public VEOF reads the wire ^D");
    assert_eq!(termios.c_cc[VMIN], 1, "public VMIN reads the wire VMIN");
    assert_eq!(termios.c_cc[VTIME], 0, "public VTIME reads the wire VTIME");
    assert_ne!(termios.c_cc[VINTR], termios.c_cc[VEOF]);
}

#[test]
fn every_control_character_round_trips_in_both_directions() {
    let (platform, fd) = tty_platform();
    let mut want = default_termios();
    for (index, byte) in want.c_cc.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(7).wrapping_add(11);
    }
    want.c_iflag = 0x100 | 0x200; // ICRNL | IXON
    want.c_oflag = 0x1 | 0x2; // OPOST | ONLCR
    want.c_cflag = 0x300 | 0x800 | 0x8000; // CS8 | CREAD | CLOCAL
    want.c_lflag = 0x8 | 0x80 | 0x100 | 0x400; // ECHO | ISIG | ICANON | IEXTEN
    want.c_ispeed = 9600;
    want.c_ospeed = 19_200;
    tcsetattr(&platform, fd, &want).expect("tcsetattr");
    let got = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(got.c_iflag, want.c_iflag);
    assert_eq!(got.c_oflag, want.c_oflag);
    assert_eq!(got.c_cflag, want.c_cflag);
    assert_eq!(got.c_lflag, want.c_lflag);
    assert_eq!(got.c_ispeed, want.c_ispeed);
    assert_eq!(got.c_ospeed, want.c_ospeed);
    assert_eq!(got.c_cc, want.c_cc);
}

/// The expected mapping, written independently of the production table.
const EXPECTED_CC_MAP: [(usize, usize); 20] = [
    (0, 4),   // VEOF
    (1, 11),  // VEOL
    (2, 16),  // VEOL2
    (3, 2),   // VERASE
    (4, 14),  // VWERASE
    (5, 3),   // VKILL
    (6, 12),  // VREPRINT
    (7, 7),   // VERASE2
    (8, 0),   // VINTR
    (9, 1),   // VQUIT
    (10, 10), // VSUSP
    (11, 17), // VDSUSP
    (12, 8),  // VSTART
    (13, 9),  // VSTOP
    (14, 15), // VLNEXT
    (15, 13), // VDISCARD
    (16, 6),  // VMIN
    (17, 5),  // VTIME
    (18, 18), // VSTATUS
    (19, 19), // spare 19
];

#[test]
fn every_public_slot_maps_to_its_exact_wire_slot() {
    assert_eq!(CC_MAP, EXPECTED_CC_MAP, "production table drifted");
    let mut public = [0u8; 20];
    for (index, byte) in public.iter_mut().enumerate() {
        *byte = 0x10 + index as u8;
    }
    let wire = cc_to_wire(&public);
    for &(public_index, wire_index) in &EXPECTED_CC_MAP {
        assert_eq!(
            wire[wire_index], public[public_index],
            "public slot {public_index} must land in wire slot {wire_index}"
        );
    }
    assert_eq!(wire[0], public[8], "VINTR");
    assert_eq!(wire[4], public[0], "VEOF");
    assert_eq!(wire[6], public[16], "VMIN");
    assert_eq!(wire[5], public[17], "VTIME");
    assert_eq!(wire[19], public[19], "spare 19");
    assert_eq!(cc_from_wire(&wire), public);
}

#[test]
fn every_donor_speed_round_trips_through_both_seams() {
    let (platform, fd) = tty_platform();
    for &(public, wire_value) in SPEED_MAP {
        let mut termios = default_termios();
        assert_eq!(cfsetispeed(&mut termios, public), 0, "ispeed {public}");
        assert_eq!(cfsetospeed(&mut termios, public), 0, "ospeed {public}");
        assert_eq!(cfgetispeed(&termios), public);
        assert_eq!(cfgetospeed(&termios), public);
        let wire = wire_from_termios(&termios);
        assert_eq!(wire.ispeed, wire_value);
        assert_eq!(wire.ospeed, wire_value);
        let back = termios_from_wire(wire);
        assert_eq!(back.c_ispeed, public);
        assert_eq!(back.c_ospeed, public);
        tcsetattr(&platform, fd, &termios).expect("tcsetattr");
        let got = tcgetattr(&platform, fd).expect("tcgetattr");
        assert_eq!(got.c_ispeed, public);
        assert_eq!(got.c_ospeed, public);
    }
}

#[test]
fn every_donor_flag_bit_round_trips_through_tcsetattr() {
    const IFLAGS: u32 = 0x0000_6fff;
    const OFLAGS: u32 = 0x0000_007f;
    const CFLAGS: u32 = 0x003f_ff01;
    const LFLAGS: u32 = 0xa2c0_0fff;
    let (platform, fd) = tty_platform();
    let mut termios = default_termios();
    termios.c_iflag = IFLAGS;
    termios.c_oflag = OFLAGS;
    termios.c_cflag = CFLAGS;
    termios.c_lflag = LFLAGS;
    tcsetattr(&platform, fd, &termios).expect("tcsetattr");
    let got = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(got.c_iflag, IFLAGS, "input flags");
    assert_eq!(got.c_oflag, OFLAGS, "output flags");
    assert_eq!(got.c_cflag, CFLAGS, "control flags");
    assert_eq!(got.c_lflag, LFLAGS, "local flags");
}

#[test]
fn posix_vdisable_is_ff_on_both_sides() {
    let public = [0xff; 20];
    let wire = cc_to_wire(&public);
    for &(_, wire_index) in CC_MAP {
        assert_eq!(wire[wire_index], 0xff, "disabled slot keeps 0xff");
    }
    assert_eq!(cc_from_wire(&wire), public);
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
    const IGNBRK: u32 = 0x1;
    const ICRNL: u32 = 0x100;
    const IXON: u32 = 0x200;
    const OPOST: u32 = 0x1;
    const ONLCR: u32 = 0x2;
    const ISIG: u32 = 0x80;
    const IEXTEN: u32 = 0x400;
    const CLOCAL: u32 = 0x8000;
    const VERASE: usize = 3;
    const VKILL: usize = 5;
    const VSTART: usize = 12;
    const VSTOP: usize = 13;

    let (platform, fd) = tty_platform();
    let mut want = default_termios();
    want.c_iflag = IGNBRK | ICRNL | IXON;
    want.c_oflag = OPOST | ONLCR;
    want.c_cflag = crate::ioctl::tty::CS8 | crate::ioctl::tty::CREAD | CLOCAL;
    want.c_lflag = ISIG | ICANON | ECHO | IEXTEN;
    want.c_cc[VINTR] = 3;
    want.c_cc[VERASE] = 0x7f;
    want.c_cc[VKILL] = 0x15;
    want.c_cc[VEOF] = 0x04;
    want.c_cc[VSTART] = 0x11;
    want.c_cc[VSTOP] = 0x13;
    want.c_cc[VMIN] = 7;
    want.c_cc[VTIME] = 3;
    want.c_ispeed = 9600;
    want.c_ospeed = 19_200;

    tcsetattr(&platform, fd, &want).expect("tcsetattr");
    let got = tcgetattr(&platform, fd).expect("tcgetattr");
    assert_eq!(got, want);
}
