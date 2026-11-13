// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn the_static_entry_returns_word_one() {
    let mut descriptor = [0usize, 0x1234];
    assert_eq!(static_entry(&mut descriptor), 0x1234);
}

#[test]
fn the_resolver_reads_module_and_offset() {
    let kernel = crate::test_support::MockKernel::new();
    let descriptor = [1usize, 0x40];
    // No DTV installed yet: the resolver returns 0 rather than dereferencing.
    assert_eq!(resolver(&kernel, &descriptor), 0);
}
