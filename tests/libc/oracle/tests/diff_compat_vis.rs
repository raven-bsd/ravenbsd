// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential tests for `compat-vis`: the Rust `vis`/`unvis` encoders against the NetBSD
//! donor C.

#![cfg(feature = "oracle-compat-vis")]
#![allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::panic,
    clippy::arithmetic_side_effects,
    clippy::undocumented_unsafe_blocks
)]

use core::ffi::c_char;
use std::ffi::CString;

use raven_libc_compat::vis::{VIS_CSTYLE, VIS_NL, VIS_OCTAL, VIS_TAB, strunvis, strvis, vis};
use raven_libc_oracle::decls::compat_vis::{oracle_strunvis, oracle_strvis, oracle_vis};

#[test]
fn vis_matches_the_donor() {
    let cases: &[(u8, u32)] = &[
        (b'A', 0),
        (b'0', 0),
        (b' ', 0),
        (b'~', 0),
        (b'\n', VIS_CSTYLE),
        (b'\n', VIS_CSTYLE | VIS_NL),
        (b'\t', VIS_CSTYLE | VIS_TAB),
        (b'\r', VIS_CSTYLE),
        (b'\x08', VIS_CSTYLE),
        (b'\x07', VIS_CSTYLE),
        (b'\x0b', VIS_CSTYLE),
        (b'\x0c', VIS_CSTYLE),
    ];
    for &(character, flags) in cases {
        let mut subject = [0u8; 16];
        let subject_length = vis(&mut subject, character, flags, 0).expect("subject vis");
        let mut oracle = [0u8; 16];
        let end = unsafe {
            oracle_vis(
                oracle.as_mut_ptr().cast::<c_char>(),
                i32::from(character),
                flags as i32,
                0,
            )
        };
        assert!(!end.is_null());
        let oracle_length = end as usize - oracle.as_ptr() as usize;
        assert_eq!(
            &subject[..subject_length],
            &oracle[..oracle_length],
            "char {character} flags {flags}"
        );
    }
}

#[test]
fn strvis_and_strunvis_match_the_donor() {
    let texts: &[(&[u8], u32)] = &[
        (b"abc", 0),
        (b"a\tb", VIS_CSTYLE | VIS_TAB),
        (b"\x01\x02", VIS_OCTAL),
    ];
    for &(text, flags) in texts {
        let source = CString::new(text).expect("source");
        let mut subject = [0u8; 64];
        let subject_length = strvis(&mut subject, text, flags).expect("subject strvis");
        let mut oracle = [0u8; 64];
        let oracle_length = unsafe {
            oracle_strvis(
                oracle.as_mut_ptr().cast::<c_char>(),
                source.as_ptr(),
                flags as i32,
            )
        };
        assert!(oracle_length >= 0);
        assert_eq!(
            &subject[..subject_length],
            &oracle[..oracle_length as usize],
            "strvis {text:?}"
        );
    }

    let escaped: &[&[u8]] = &[b"abc", b"a\\tb", b"\\101\\102"];
    for text in escaped {
        let source = CString::new(*text).expect("source");
        let mut subject = [0u8; 64];
        let subject_length = strunvis(&mut subject, text, 0).expect("subject strunvis");
        let mut oracle = [0u8; 64];
        let oracle_length =
            unsafe { oracle_strunvis(oracle.as_mut_ptr().cast::<c_char>(), source.as_ptr()) };
        assert!(oracle_length >= 0);
        assert_eq!(
            &subject[..subject_length],
            &oracle[..oracle_length as usize],
            "strunvis {text:?}"
        );
    }
}
