// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the K491 fixture package.
//!
//! The kernel rules themselves are proven by the guest qtest; this test keeps the package
//! buildable and pins the resource indices the fixture uses.

use super::*;

#[test]
fn fixture_uses_the_abi_resource_indices() {
    assert_eq!(RESOURCE_CPU, 0);
    assert_eq!(RESOURCE_NOFILE, 7);
    assert_eq!(RESOURCE_COUNT, 16);
}
