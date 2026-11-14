// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn utf8_matches_the_standard_library() {
    assert_eq!(utf8(b"abc"), Some("abc"));
    assert_eq!(utf8(&[0xc0, 0x80]), None);
    assert_eq!(utf8(&[0xed, 0xa0, 0x80]), None);
    assert_eq!(utf8(&[0xf4, 0x90, 0x80, 0x80]), None);
}

#[test]
fn a_stride_of_four_byte_values_agrees_with_the_standard_library() {
    let mut buffer = [0u8; 4];
    let mut value = 0u32;
    while value < 0x11_0000 {
        buffer.copy_from_slice(&value.to_le_bytes());
        assert_eq!(
            utf8(&buffer).is_some(),
            core::str::from_utf8(&buffer).is_ok()
        );
        value = value.wrapping_add(4099);
    }
}

#[test]
fn decimal_and_signed_forms_model_the_scanner() {
    let model = int(b"  -42xyz", 10);
    assert_eq!(model.signed, -42);
    assert_eq!(model.unsigned, (-42i64) as u64);
    assert_eq!(model.consumed, 5);
    assert!(!model.range);
}

#[test]
fn hex_and_octal_prefixes_are_detected() {
    assert_eq!(int(b"0x2a", 0).signed, 42);
    assert_eq!(int(b"052", 0).signed, 42);
    assert_eq!(int(b"0b101", 0).signed, 5);
    assert_eq!(int(b"0x2a", 16).signed, 42);
}

#[test]
fn invalid_bases_and_no_digits_are_reported() {
    for base in [-1, 1, 37] {
        assert!(int(b"42", base).invalid_base);
        assert_eq!(int(b"42", base).consumed, 0);
    }
    assert!(int(b"xyz", 10).no_digits);
    assert_eq!(int(b"xyz", 10).consumed, 0);
}

#[test]
fn overflow_saturates() {
    let model = int(b"99999999999999999999999999", 10);
    assert!(model.range);
    assert_eq!(model.signed, i64::MAX);
    let negative = int(b"-99999999999999999999999999", 10);
    assert!(negative.range);
    assert_eq!(negative.signed, i64::MIN);
}

#[test]
fn base_thirty_six_digits_work() {
    assert_eq!(int(b"zz", 36).signed, 35 * 36 + 35);
}

#[test]
fn elf_and_gnu_hashes_match_the_documented_vectors() {
    assert_eq!(elf_hash(b""), 0);
    assert_eq!(elf_hash(b"printf"), 0x0779_05a6);
    assert_eq!(gnu_hash(b""), 5381);
    assert_eq!(gnu_hash(b"printf"), 0x156b_2bb8);
}

#[test]
fn binary_and_unsigned_saturation() {
    assert_eq!(int(b"0b111", 0).signed, 7);
    assert_eq!(int(b"-0b111", 0).signed, -7);
    assert_eq!(int(b"18446744073709551616", 10).unsigned, u64::MAX);
}

#[test]
fn dn_lowercases_and_drops_the_trailing_dot() {
    assert_eq!(dn(b"Example.COM."), b"example.com");
    assert_eq!(dn(b""), b"");
    assert_eq!(dn(b"."), b"");
}

#[test]
fn init_order_accepts_a_dependency_first_order() {
    let deps = |id: u32| match id {
        3 => vec![1, 2],
        2 => vec![1],
        _ => Vec::new(),
    };
    let order = [1, 2, 3];
    let fini = [3, 2, 1];
    assert!(init_order_ok(&order, &fini, &deps));
}

#[test]
fn init_order_rejects_a_dependency_last_order() {
    let deps = |id: u32| match id {
        1 => vec![2],
        _ => Vec::new(),
    };
    assert!(!init_order_ok(&[1, 2], &[2, 1], &deps));
}

#[test]
fn init_order_rejects_duplicates_and_a_wrong_fini() {
    let deps = |_id: u32| Vec::new();
    assert!(!init_order_ok(&[1, 1], &[1, 1], &deps));
    assert!(!init_order_ok(&[1, 2], &[1, 2], &deps));
}

#[test]
fn init_order_ignores_a_dependency_outside_the_order() {
    let deps = |id: u32| match id {
        1 => vec![2],
        _ => Vec::new(),
    };
    assert!(init_order_ok(&[1], &[1], &deps));
}
