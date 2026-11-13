// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn zero_length_is_ok() {
    assert_eq!(getentropy_check(0), Ok(()));
}

#[test]
fn exact_maximum_is_ok() {
    assert_eq!(getentropy_check(256), Ok(()));
}

#[test]
fn above_maximum_is_eio() {
    assert_eq!(getentropy_check(257), Err(RandError::Io));
    assert_eq!(getentropy_check(usize::MAX), Err(RandError::Io));
}

#[test]
fn flags_zero_decodes_to_false() {
    assert_eq!(getrandom_flags(0), Ok(GrndFlags::default()));
}

#[test]
fn every_known_flag_decodes() {
    assert_eq!(
        getrandom_flags(GRND_NONBLOCK),
        Ok(GrndFlags {
            nonblock: true,
            ..GrndFlags::default()
        })
    );
    assert_eq!(
        getrandom_flags(GRND_RANDOM),
        Ok(GrndFlags {
            random: true,
            ..GrndFlags::default()
        })
    );
    assert_eq!(
        getrandom_flags(GRND_INSECURE),
        Ok(GrndFlags {
            insecure: true,
            ..GrndFlags::default()
        })
    );
}

#[test]
fn unknown_bits_are_invalid() {
    assert_eq!(getrandom_flags(8), Err(RandError::Inval));
    assert_eq!(getrandom_flags(0x8000_0000), Err(RandError::Inval));
    assert_eq!(getrandom_flags(GRND_NONBLOCK | 8), Err(RandError::Inval));
}

#[test]
fn random_and_insecure_conflict() {
    assert_eq!(
        getrandom_flags(GRND_RANDOM | GRND_INSECURE),
        Err(RandError::Inval)
    );
}

#[test]
fn getrandom_length_is_clamped() {
    assert_eq!(getrandom_len(0), 0);
    assert_eq!(getrandom_len(1), 1);
    assert_eq!(getrandom_len(256), 256);
    assert_eq!(getrandom_len(257), 256);
    assert_eq!(getrandom_len(usize::MAX), 256);
}

#[test]
fn nonblock_and_insecure_are_valid() {
    assert!(getrandom_flags(GRND_NONBLOCK | GRND_INSECURE).is_ok());
}

#[test]
fn nonblock_and_random_are_valid() {
    assert!(getrandom_flags(GRND_NONBLOCK | GRND_RANDOM).is_ok());
}

#[test]
fn boundaries_255_and_256() {
    assert_eq!(getentropy_check(255), Ok(()));
    assert_eq!(getentropy_check(256), Ok(()));
}
