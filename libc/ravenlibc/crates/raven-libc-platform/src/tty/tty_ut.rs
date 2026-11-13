// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TTY module tests.

#[test]
fn tty_name_max_is_32() {
    assert_eq!(crate::tty::TTY_NAME_MAX, 32);
}

#[test]
fn tty_name_max_matches_the_abi() {
    assert_eq!(
        crate::tty::TTY_NAME_MAX,
        raven_libc_abi::limits::TTY_NAME_MAX as usize
    );
}
