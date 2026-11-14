// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C70 finding: `REG_ICASE | REG_NOSPEC` with an alphabetic byte must not recurse.
//!
//! The input is the minimised regression file
//! `tests/libc/fuzz/regressions/regex_compile_exec/2026-11-13-regex-icase-nospec.bin`.

use raven_libc_regex::{ByteLocale, regcomp};

#[test]
fn icase_nospec_with_an_alphabetic_byte_terminates() {
    let pattern = [
        0x35, 0xe8, 0x98, 0xa1, 0x9b, 0x7a, 0x3f, 0x9b, 0x08, 0xc3, 0xe2, 0x2c, 0xe0, 0xe4, 0x6a,
        0xa1, 0x2e, 0x7d, 0x01, 0x81, 0xa7, 0x55, 0x09, 0x41, 0x82, 0x6f, 0xfd, 0xbf, 0xed, 0x20,
        0x3c, 0x4b, 0xc3, 0xff, 0xe9,
    ];
    assert!(regcomp(&pattern, 0o46, &ByteLocale).is_ok());
}
