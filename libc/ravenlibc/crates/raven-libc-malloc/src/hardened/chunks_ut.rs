// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn junk_words_are_the_donor_pattern() {
    assert_eq!(JUNK_FREE_WORD, 0xdfdfdfdfdfdfdfdf_u64);
    assert_eq!(JUNK_ALLOC, 0xdb);
    assert_eq!(JUNK_FREE, 0xdf);
}

#[test]
fn bucket_bytes_cover_the_small_range() {
    assert_eq!(bucket_bytes(0), 16);
    assert_eq!(bucket_bytes(1), 16);
    assert_eq!(bucket_bytes(128), 2048);
    assert_eq!(find_bucket(2048), 128);
    assert_eq!(find_bucket(2049), 129);
}
