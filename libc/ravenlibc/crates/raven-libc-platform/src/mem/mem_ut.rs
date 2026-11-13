// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Memory module aggregate tests.

#[test]
fn bounce_limit_is_64_kib() {
    assert_eq!(crate::io::BOUNCE_LIMIT, 64 * 1024);
}

#[test]
fn page_size_matches_the_abi() {
    assert_eq!(crate::mem::getpagesize(), raven_libc_abi::limits::PAGE_SIZE);
}
