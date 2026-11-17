// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn ftok_uses_the_posix_bit_layout() {
    assert_eq!(ftok_key(0x1234, 0xbeef, 0x42), 0x4234_beef);
    assert_eq!(ftok_key(0, 0, 0), 0);
    // Only the documented low bits take part.
    assert_eq!(ftok_key(0xabcd, 0x1_2345, 0x7f), 0x7fcd_2345);
}

#[test]
fn ftok_preserves_the_signed_key_pattern() {
    assert_eq!(ftok_key(0, 0, 0x80), -2_147_483_648);
    assert_eq!(ftok_key(0, 0, 0xff), -16_777_216);
}

#[test]
fn the_private_key_is_zero_only() {
    assert!(key_is_private(IPC_PRIVATE));
    assert!(!key_is_private(1));
    assert!(!key_is_private(-1));
}

#[test]
fn sysv_object_service_is_not_faked() {
    assert_eq!(unsupported(), crate::consts::ENOSYS);
}
