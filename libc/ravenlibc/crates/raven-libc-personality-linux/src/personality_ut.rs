// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `personality` query and accepted value.

use super::*;

#[test]
fn query_returns_per_linux() {
    assert_eq!(personality(PERSONALITY_QUERY), Ok(0));
}

#[test]
fn per_linux_is_accepted() {
    assert_eq!(personality(PER_LINUX), Ok(0));
}

#[test]
fn every_other_value_is_einval() {
    for value in [1u64, 2, 8, 0x1000, u64::MAX] {
        assert_eq!(personality(value), Err(Errno::EINVAL), "{value}");
    }
}

#[test]
fn query_word_is_the_unsigned_max() {
    assert_eq!(PERSONALITY_QUERY, u64::from(u32::MAX));
}
