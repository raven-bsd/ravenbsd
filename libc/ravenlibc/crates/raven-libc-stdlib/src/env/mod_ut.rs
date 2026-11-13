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

//! The environment group surface and `reload`.

use super::*;

fn leak(bytes: &[u8]) -> &'static [u8] {
    Box::leak(bytes.to_vec().into_boxed_slice())
}

#[test]
fn reload_replaces_the_list() {
    let mut env = EnvTable::new();
    env.adopt_initial(&[leak(b"A=1\0")]);
    env.reload(&[leak(b"B=2\0"), leak(b"C=3\0")]);
    assert_eq!(env.getenv(b"A"), None);
    assert_eq!(env.getenv(b"B"), Some(b"2".as_slice()));
    assert_eq!(env.len(), 2);
}

#[test]
fn as_slice_is_the_published_array() {
    let mut env = EnvTable::new();
    let a = leak(b"A=1\0");
    env.adopt_initial(&[a]);
    assert_eq!(env.as_slice(), &[a]);
}

#[test]
fn default_is_empty() {
    let env = EnvTable::default();
    assert!(env.is_empty());
    assert_eq!(env.generation(), 0);
}

#[test]
fn empty_table_has_zero_generation() {
    let table = EnvTable::new();
    assert_eq!(table.generation(), 0);
    assert_eq!(table.len(), 0);
}

#[test]
fn adopt_initial_bumps_generation() {
    let mut table = EnvTable::new();
    let entry = leak(b"A=1\0");
    table.adopt_initial(&[entry]);
    assert!(table.generation() > 0);
    assert_eq!(table.len(), 1);
}

#[test]
fn clearenv_on_empty_is_quiet() {
    let mut table = EnvTable::new();
    let generation = table.generation();
    clearenv(&mut table);
    assert_eq!(table.generation(), generation);
}

#[test]
fn putenv_without_equal_on_missing_name_is_ok() {
    let mut table = EnvTable::new();
    assert!(putenv(&mut table, leak(b"MISSING\0")).is_ok());
    assert!(table.is_empty());
}

#[test]
fn getenv_r_exact_buffer_fits_nul() {
    let mut table = EnvTable::new();
    let entry = leak(b"A=xy\0");
    table.adopt_initial(&[entry]);
    let mut buf = [0u8; 3];
    assert_eq!(getenv_r(&table, b"A", &mut buf), Ok(2));
    assert_eq!(&buf, b"xy\0");
}

#[test]
fn secure_getenv_at_secure_false() {
    let mut table = EnvTable::new();
    let entry = leak(b"A=1\0");
    table.adopt_initial(&[entry]);
    assert_eq!(secure_getenv(&table, b"A", false), Some(b"1".as_slice()));
}

#[test]
fn multiple_entries_order() {
    let mut table = EnvTable::new();
    let a = leak(b"A=1\0");
    let b = leak(b"B=2\0");
    table.adopt_initial(&[a, b]);
    assert_eq!(table.as_slice(), &[a, b]);
}

#[test]
fn reload_keeps_generation_monotonic() {
    let mut table = EnvTable::new();
    table.adopt_initial(&[leak(b"A=1\0")]);
    let first = table.generation();
    table.reload(&[leak(b"B=2\0")]);
    assert!(table.generation() > first);
}
