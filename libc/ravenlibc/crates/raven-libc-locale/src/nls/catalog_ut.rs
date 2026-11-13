// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Catalog tests.

use super::*;
use alloc::collections::BTreeMap;

struct Fs(BTreeMap<Vec<u8>, Vec<u8>>);
impl FileSource for Fs {
    fn read_all(&self, path: &[u8], max: usize) -> Option<Vec<u8>> {
        self.0.get(path).filter(|data| data.len() <= max).cloned()
    }
}

struct Env(Vec<(Vec<u8>, Vec<u8>)>);
impl EnvSource for Env {
    fn get(&self, name: &[u8]) -> Option<&[u8]> {
        self.0
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_slice())
    }
}

fn catalog() -> Vec<u8> {
    let mut data = vec![0u8; 20 + 12 + 12];
    data.extend_from_slice(b"hello\0");
    let put = |data: &mut Vec<u8>, at: usize, value: u32| {
        if let Some(slot) = data.get_mut(at..at + 4) {
            slot.copy_from_slice(&value.to_be_bytes());
        }
    };
    let body_size = (data.len() - 20) as u32;
    put(&mut data, 0, 0xff88_ff89);
    put(&mut data, 4, 1);
    put(&mut data, 8, body_size);
    put(&mut data, 12, 12);
    put(&mut data, 16, 24);
    put(&mut data, 20, 1);
    put(&mut data, 24, 1);
    put(&mut data, 28, 0);
    put(&mut data, 32, 1);
    put(&mut data, 36, 5);
    put(&mut data, 40, 0);
    data
}

#[test]
fn valid_catalog_lookup() {
    let mut map = BTreeMap::new();
    map.insert(b"/tmp/test.cat".to_vec(), catalog());
    let fs = Fs(map);
    let catalog =
        Catalog::open(&fs, &Env(Vec::new()), b"/tmp/test.cat", 0, false, b"").expect("open");
    assert_eq!(catalog.gets(1, 1, b"default"), b"hello");
    assert_eq!(catalog.gets(1, 2, b"default"), b"default");
    assert_eq!(catalog.gets(2, 1, b"default"), b"default");
}

#[test]
fn wrong_magic_and_size_are_rejected() {
    let mut map = BTreeMap::new();
    let mut data = catalog();
    data[0] = 0;
    map.insert(b"bad".to_vec(), data);
    let fs = Fs(map);
    assert_eq!(
        Catalog::open(&fs, &Env(Vec::new()), b"bad", 0, false, b"").err(),
        Some(CatError::NotFound)
    );
    let mut map = BTreeMap::new();
    let mut data = catalog();
    data.pop();
    map.insert(b"short".to_vec(), data);
    let fs = Fs(map);
    assert_eq!(
        Catalog::open(&fs, &Env(Vec::new()), b"short", 0, false, b"").err(),
        Some(CatError::NotFound)
    );
}

#[test]
fn nlsxpath_expansion() {
    let mut map = BTreeMap::new();
    map.insert(b"/prefix/en/test.cat".to_vec(), catalog());
    let fs = Fs(map);
    let env = Env(vec![(b"NLSPATH".to_vec(), b"/prefix/%L/%N.cat".to_vec())]);
    let catalog = Catalog::open(&fs, &env, b"test", 1, false, b"en").expect("open");
    assert_eq!(catalog.gets(1, 1, b"default"), b"hello");
}
