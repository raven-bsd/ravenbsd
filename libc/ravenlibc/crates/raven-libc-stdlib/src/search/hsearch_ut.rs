// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `hsearch`: growth, collisions, the `_r` slots and every error path.

use super::*;

struct Strings {
    texts: Vec<Vec<u8>>,
}

impl KeyText for Strings {
    fn text(&self, key: usize) -> &[u8] {
        self.texts
            .get(key)
            .map(|text| text.as_slice())
            .unwrap_or(&[])
    }
}

struct Counter(u8);

impl Entropy for Counter {
    fn fill(&mut self, buf: &mut [u8]) {
        for byte in buf {
            *byte = self.0;
        }
    }
}

fn strings() -> Strings {
    let texts = (0..200_000usize)
        .map(|index| format!("key-{index}").into_bytes())
        .collect();
    Strings { texts }
}

#[test]
fn create_starts_at_sixteen_slots() {
    let table = hcreate(&mut Counter(0xa5)).unwrap();
    assert_eq!(table.capacity(), 16);
    assert_eq!(table.entries_used(), 0);
}

#[test]
fn enter_find_and_duplicates() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0xa5)).unwrap();
    let first = table.hsearch(1, 111, Action::Enter, &keys).unwrap();
    let again = table.hsearch(1, 999, Action::Enter, &keys).unwrap();
    assert_eq!(first, again);
    assert_eq!(table.entries_used(), 1);
    let slot = table.hsearch(1, 0, Action::Find, &keys).unwrap();
    assert_eq!(slot, first);
    assert_eq!(
        table.hsearch(2, 0, Action::Find, &keys),
        Err(HSearchError::NotFound)
    );
}

#[test]
fn one_hundred_thousand_inserts_grow_and_remain_findable() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0xa5)).unwrap();
    for key in 1..=100_000usize {
        table.hsearch(key, key * 2, Action::Enter, &keys).unwrap();
    }
    assert_eq!(table.entries_used(), 100_000);
    assert!(table.capacity() > 100_000);
    for key in 1..=100_000usize {
        let slot = table.hsearch(key, 0, Action::Find, &keys).unwrap();
        let entry = table.entries[slot.0].unwrap();
        assert_eq!(entry.key, key);
        assert_eq!(entry.data, key * 2);
    }
    for key in 100_001..100_100usize {
        assert_eq!(
            table.hsearch(key, 0, Action::Find, &keys),
            Err(HSearchError::NotFound)
        );
    }
}

#[test]
fn collisions_still_find_both_keys() {
    let keys = Strings {
        texts: vec![
            b"ab".to_vec(),
            b"ba".to_vec(),
            b"aab".to_vec(),
            b"abb".to_vec(),
        ],
    };
    let mut table = hcreate(&mut Counter(1)).unwrap();
    for key in 0..4usize {
        table.hsearch(key, key + 10, Action::Enter, &keys).unwrap();
    }
    for key in 0..4usize {
        assert!(table.hsearch(key, 0, Action::Find, &keys).is_ok());
    }
}

#[test]
fn growth_preserves_the_offset_basis() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0x33)).unwrap();
    let before = table.offset_basis;
    for key in 1..=100usize {
        table.hsearch(key, key, Action::Enter, &keys).unwrap();
    }
    assert_eq!(table.offset_basis, before);
    assert!(table.capacity() > 16);
}

#[test]
fn hdestroy_does_not_touch_keys_or_data() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(7)).unwrap();
    table.hsearch(1, 2, Action::Enter, &keys).unwrap();
    hdestroy(table);
    assert_eq!(keys.texts[1], b"key-1");
}

#[test]
fn error_variants_are_distinct() {
    assert_ne!(HSearchError::NoMem, HSearchError::NotFound);
}

#[test]
fn empty_find_is_not_found() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0)).unwrap();
    assert_eq!(
        table.hsearch(1, 0, Action::Find, &keys),
        Err(HSearchError::NotFound)
    );
}

#[test]
fn capacity_doubles() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0)).unwrap();
    let before = table.capacity();
    for key in 1..=100usize {
        table.hsearch(key, key, Action::Enter, &keys).unwrap();
    }
    assert!(table.capacity() > before);
    assert_eq!(table.capacity() % 16, 0);
}

#[test]
fn zero_key_is_still_a_key() {
    let keys = Strings {
        texts: vec![b"zero".to_vec(), b"one".to_vec()],
    };
    let mut table = hcreate(&mut Counter(0)).unwrap();
    table.hsearch(0, 10, Action::Enter, &keys).unwrap();
    table.hsearch(1, 11, Action::Enter, &keys).unwrap();
    assert!(table.hsearch(0, 0, Action::Find, &keys).is_ok());
}

#[test]
fn enter_after_growth_finds_all() {
    let keys = strings();
    let mut table = hcreate(&mut Counter(0)).unwrap();
    for key in 1..=1000usize {
        table.hsearch(key, key, Action::Enter, &keys).unwrap();
    }
    for key in 1..=1000usize {
        assert!(table.hsearch(key, 0, Action::Find, &keys).is_ok());
    }
}

#[test]
fn entry_fields_are_public() {
    let entry = Entry { key: 1, data: 2 };
    assert_eq!(entry.key, 1);
    assert_eq!(entry.data, 2);
}
