// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn the_defined_speed_range_is_accepted() {
    for speed in [B0, 1, 0o16, B38400, B57600, 0o10005, B4000000] {
        assert!(is_supported_speed(speed), "{speed:#o}");
    }
    for speed in [0o20, 0o10000, 0o10020, u32::MAX] {
        assert!(!is_supported_speed(speed), "{speed:#o}");
    }
}

#[test]
fn speeds_round_trip_through_the_record() {
    let mut termios = Termios::ZERO;
    assert_eq!(cfgetispeed(&termios), B0);
    assert_eq!(cfgetospeed(&termios), B0);

    assert_eq!(cfsetispeed(&mut termios, 0o15), Ok(()));
    assert_eq!(cfgetispeed(&termios), 0o15);
    assert_eq!(cfgetospeed(&termios), B0);

    assert_eq!(cfsetospeed(&mut termios, B57600), Ok(()));
    assert_eq!(cfgetospeed(&termios), B57600);
    assert_eq!(cfgetispeed(&termios), 0o15);

    assert_eq!(cfsetspeed(&mut termios, B38400), Ok(()));
    assert_eq!(cfgetispeed(&termios), B38400);
    assert_eq!(cfgetospeed(&termios), B38400);
}

#[test]
fn an_invalid_speed_leaves_the_record_alone() {
    let mut termios = Termios::ZERO;
    termios.__c_ispeed = 0o14;
    termios.__c_ospeed = 0o13;

    assert_eq!(cfsetispeed(&mut termios, 0o20), Err(EINVAL));
    assert_eq!(cfsetospeed(&mut termios, 0o20), Err(EINVAL));
    assert_eq!(cfsetspeed(&mut termios, 0o20), Err(EINVAL));
    assert_eq!(termios.__c_ispeed, 0o14);
    assert_eq!(termios.__c_ospeed, 0o13);
}

#[test]
fn queue_and_flow_selectors_are_bounded() {
    assert!(validate_queue(TCIFLUSH).is_ok());
    assert!(validate_queue(TCOFLUSH).is_ok());
    assert!(validate_queue(TCIOFLUSH).is_ok());
    assert_eq!(validate_queue(3), Err(EINVAL));

    assert!(validate_flow(TCOOFF).is_ok());
    assert!(validate_flow(TCOON).is_ok());
    assert!(validate_flow(TCIOFF).is_ok());
    assert!(validate_flow(TCION).is_ok());
    assert_eq!(validate_flow(4), Err(EINVAL));
}

#[test]
fn unimplemented_tty_operations_are_typed() {
    assert_eq!(unsupported(), crate::consts::ENOSYS);
}
