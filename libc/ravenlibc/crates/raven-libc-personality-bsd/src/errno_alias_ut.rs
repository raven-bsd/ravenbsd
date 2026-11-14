// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `__error`/`__errno` contract.

use super::*;

#[test]
fn both_spellings_are_named() {
    assert_eq!(ERRNO_ALIASES, &["__error", "__errno"]);
}
