// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `uuid` module tests: the type layout and the re-exports.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
fn uuid_is_sixteen_bytes() {
    assert_eq!(core::mem::size_of::<Uuid>(), 16);
    assert_eq!(core::mem::align_of::<Uuid>(), 4);
}

#[test]

fn status_values_match_uuid_h() {
    assert_eq!(Status::Ok as u32, 0);
    assert_eq!(Status::BadVersion as u32, 1);
    assert_eq!(Status::InvalidString as u32, 2);
    assert_eq!(Status::NoMemory as u32, 3);
}

#[test]
fn re_exports_are_present() {
    let mut entropy = crate::testutil::FakeEntropy::new(1);
    let uuid = create(&mut entropy);
    assert!(!is_nil(&uuid));
    assert!(equal(&uuid, &uuid));
    assert_eq!(compare(&uuid, &uuid), 0);
    let _ = hash(&uuid);
    let mut text = [0u8; 37];
    to_string(&uuid, &mut text);
    assert_eq!(
        &text[..36],
        &from_string(&text[..36])
            .map(|parsed| {
                let mut again = [0u8; 37];
                to_string(&parsed, &mut again);
                again
            })
            .unwrap()[..36]
    );
    let mut bytes = [0u8; 16];
    enc_be(&uuid, &mut bytes);
    assert_eq!(dec_be(&bytes), uuid);
    enc_le(&uuid, &mut bytes);
    assert_eq!(dec_le(&bytes), uuid);
    assert_eq!(create_nil(), Uuid::default());
}
