// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `compat-humanize`: Rust `humanize_number`/`expand_number` against the
//! FreeBSD donor C.

#![cfg(feature = "oracle-compat-humanize")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks
)]

use core::ffi::c_char;
use std::ffi::CString;

use raven_libc_compat::humanize::expand::expand_number;
use raven_libc_compat::humanize::number::humanize_number;
use raven_libc_compat::humanize::{HN_AUTOSCALE, HN_DIVISOR_1000};
use raven_libc_oracle::decls::compat_humanize::{oracle_expand_number, oracle_humanize_number};

#[test]
fn humanize_number_matches_the_donor() {
    let cases: &[(i64, i32, i32, usize)] = &[
        (0, HN_AUTOSCALE, HN_DIVISOR_1000, 4),
        (500, HN_AUTOSCALE, HN_DIVISOR_1000, 4),
        (1_500_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4),
        (500_000_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4),
        (500_000_000_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4),
    ];
    let empty = CString::new("").expect("empty suffix");
    for &(quotient, scale, flags, len) in cases {
        let mut subject = vec![b'X'; len];
        let subject_result = humanize_number(&mut subject, quotient, b"", scale, flags);
        let mut oracle = vec![b'X'; len];
        let oracle_result = unsafe {
            oracle_humanize_number(
                oracle.as_mut_ptr().cast::<c_char>(),
                len,
                quotient,
                empty.as_ptr(),
                scale,
                flags,
            )
        };
        assert_eq!(subject_result, oracle_result, "quotient {quotient}");
        let written = usize::try_from(subject_result.max(0)).unwrap_or(0).min(len);
        assert_eq!(
            &subject[..written],
            &oracle[..written],
            "quotient {quotient}"
        );
    }
}

#[test]
fn expand_number_matches_the_donor() {
    for text in ["0", "1K", "2M", "1G", "-1", "0x10", "1.5K", "bad"] {
        let subject = expand_number(text.as_bytes());
        let ctext = CString::new(text).expect("text");
        let mut value = 0i64;
        let oracle = unsafe { oracle_expand_number(ctext.as_ptr(), &mut value) };
        match subject {
            Ok(number) => {
                assert_eq!(oracle, 0, "text {text}");
                assert_eq!(number, value, "text {text}");
            }
            Err(_) => assert_ne!(oracle, 0, "text {text}"),
        }
    }
}
