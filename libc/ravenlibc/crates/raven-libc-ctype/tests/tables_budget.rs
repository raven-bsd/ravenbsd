// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Table budget and version assertions.

use raven_libc_ctype::{TABLE_BYTES, UNICODE_VERSION};

#[test]
fn table_budget_is_48_kib_and_the_version_is_15_1_0() {
    assert_eq!(UNICODE_VERSION, (15, 1, 0));
    const {
        assert!(TABLE_BYTES <= 48 * 1024);
        assert!(TABLE_BYTES >= 32 * 1024);
    }
    println!("TABLE_BYTES = {TABLE_BYTES}");
}
