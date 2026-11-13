// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{ERRMSG_BLOB, ERRMSG_COUNT, ERRMSG_OFF, text};

#[test]
fn every_slot_has_text() {
    for slot in 0usize..ERRMSG_COUNT {
        let value = text(slot).unwrap_or_else(|| panic!("slot {slot} has no text"));
        assert!(!value.is_empty(), "slot {slot} is empty");
    }
    assert_eq!(text(ERRMSG_COUNT), None);
    assert_eq!(text(usize::MAX), None);
}

#[test]
fn offsets_are_sorted_and_inside_the_blob() {
    for slot in 0usize..ERRMSG_COUNT {
        let offset = usize::from(ERRMSG_OFF[slot]);
        assert!(offset < ERRMSG_BLOB.len(), "slot {slot} offset {offset}");
        if slot != 0 {
            assert!(usize::from(ERRMSG_OFF[slot - 1]) < offset, "slot {slot}");
        }
    }
}

#[test]
fn blob_is_nul_separated_with_exactly_one_nul_per_slot() {
    let nuls = ERRMSG_BLOB.iter().filter(|&&byte| byte == 0).count();
    assert_eq!(nuls, ERRMSG_COUNT);
    assert_eq!(ERRMSG_BLOB.last(), Some(&0));
}

#[test]
fn known_texts() {
    assert_eq!(text(0), Some(&b"Success"[..]));
    assert_eq!(text(1), Some(&b"Operation not permitted"[..]));
    assert_eq!(text(11), Some(&b"Resource temporarily unavailable"[..]));
    assert_eq!(text(22), Some(&b"Invalid argument"[..]));
    assert_eq!(text(41), Some(&b"No error information"[..]));
    assert_eq!(text(58), Some(&b"No error information"[..]));
    assert_eq!(text(95), Some(&b"Not supported"[..]));
    assert_eq!(text(133), Some(&b"Memory page has hardware error"[..]));
}

#[test]
fn every_abi_errno_has_a_message() {
    let mut seen: Vec<(i32, &[u8])> = Vec::new();
    for &(name, value) in raven_libc_abi::errno::ALL {
        if !(0..=133).contains(&value) {
            continue;
        }
        let value = value as i32;
        let message =
            text(value as usize).unwrap_or_else(|| panic!("{name} ({value}) has no text"));
        assert!(!message.is_empty(), "{name}");
        if let Some((_, previous)) = seen.iter().find(|(number, _)| *number == value) {
            assert_eq!(*previous, message, "alias {name} disagrees");
        } else {
            for (other_number, other) in &seen {
                assert_ne!(
                    *other, message,
                    "numbers {other_number} and {value} share the text"
                );
            }
            seen.push((value, message));
        }
    }
    assert!(seen.len() >= 131, "only {} distinct numbers", seen.len());
}

#[test]
fn no_duplicate_text_between_distinct_numbers() {
    for left in 0usize..ERRMSG_COUNT {
        for right in left + 1..ERRMSG_COUNT {
            if left == 41 || left == 58 || right == 41 || right == 58 {
                continue; // the catch-all is deliberately shared
            }
            assert_ne!(text(left), text(right), "slots {left} and {right}");
        }
    }
}
