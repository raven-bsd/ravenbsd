// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn constants_match_the_header() {
    assert_eq!(AIO_CANCELED, 0);
    assert_eq!(AIO_NOTCANCELED, 1);
    assert_eq!(AIO_ALLDONE, 2);
    assert_eq!(LIO_READ, 0);
    assert_eq!(LIO_WRITE, 1);
    assert_eq!(LIO_NOP, 2);
    assert_eq!(LIO_WAIT, 0);
    assert_eq!(LIO_NOWAIT, 1);
}

#[test]
fn ai_o_is_never_reported_as_queued() {
    assert_eq!(unsupported(), crate::consts::ENOSYS);
}

#[test]
fn counts_modes_and_opcodes_are_validated() {
    assert!(validate_count(0).is_ok());
    assert!(validate_count(4).is_ok());
    assert_eq!(validate_count(-1), Err(crate::consts::EINVAL));

    assert!(validate_opcode(LIO_READ).is_ok());
    assert!(validate_opcode(LIO_WRITE).is_ok());
    assert!(validate_opcode(LIO_NOP).is_ok());
    assert_eq!(validate_opcode(9), Err(crate::consts::EINVAL));

    assert!(validate_mode(LIO_WAIT).is_ok());
    assert!(validate_mode(LIO_NOWAIT).is_ok());
    assert_eq!(validate_mode(7), Err(crate::consts::EINVAL));
}
