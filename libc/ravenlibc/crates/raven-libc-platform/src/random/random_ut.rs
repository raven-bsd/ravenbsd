// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Random module aggregate tests.

#[test]
fn getentropy_max_is_256() {
    assert_eq!(crate::random::GETENTROPY_MAX, 256);
}
