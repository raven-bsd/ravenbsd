// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: `inet_pton` must expand a leading `::` correctly.

use raven_libc_resolv::inet_pton;

#[test]
fn double_colon_expands_to_the_documented_address() {
    let mut address = [0u8; 16];
    assert_eq!(inet_pton(10, b"::a:4e:3:e:a:a", &mut address), 1);
    assert_eq!(
        address,
        [
            0, 0, 0, 0, 0, 0x0a, 0, 0x4e, 0, 0x03, 0, 0x0e, 0, 0x0a, 0, 0x0a
        ]
    );
}
