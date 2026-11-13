// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `posix_spawnp` prechecks.

use super::*;

#[test]
fn an_empty_name_is_enoent() {
    assert_eq!(spawnp_prechecks(b""), Err(ENOENT));
}

#[test]
fn a_plain_name_needs_a_search() {
    assert_eq!(spawnp_prechecks(b"sh"), Ok(true));
}

#[test]
fn a_name_with_a_slash_is_direct() {
    assert_eq!(spawnp_prechecks(b"/bin/sh"), Ok(false));
    assert_eq!(spawnp_prechecks(b"./sh"), Ok(false));
}

#[test]
fn a_long_plain_name_still_needs_a_search() {
    let name = alloc::vec![b'x'; 300];
    assert_eq!(spawnp_prechecks(&name), Ok(true));
}
