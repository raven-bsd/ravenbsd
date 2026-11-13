// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn twelve_names_in_order() {
    let names: [&[u8]; 12] = [
        b"alnum", b"alpha", b"blank", b"cntrl", b"digit", b"graph", b"lower", b"print", b"punct",
        b"space", b"upper", b"xdigit",
    ];
    for (index, name) in names.iter().enumerate() {
        assert_eq!(wctype(name), index as u64 + 1);
    }
    assert_eq!(wctype(b"x"), 0);
    assert_eq!(wctype(b""), 0);
}

#[test]
fn iswctype_matches_the_functions() {
    assert!(iswctype(u32::from(b'a'), WCTYPE_ALPHA));
    assert!(iswctype(u32::from(b'a'), WCTYPE_ALNUM));
    assert!(!iswctype(u32::from(b' '), WCTYPE_ALNUM));
    assert!(iswctype(u32::from(b' '), WCTYPE_SPACE));
    assert!(!iswctype(u32::from(b'a'), 0));
    assert!(!iswctype(u32::from(b'a'), 99));
}
