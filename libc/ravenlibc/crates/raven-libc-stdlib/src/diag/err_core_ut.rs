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

//! The `v*` writer order, the `x` forms and the explicit-code forms.

use super::*;

#[derive(Default)]
struct Fake {
    bytes: Vec<u8>,
}

impl DiagOut for Fake {
    fn write(&mut self, b: &[u8]) {
        self.bytes.extend_from_slice(b);
    }
    fn user_message(&mut self) {
        self.bytes.extend_from_slice(b"<fmt>");
    }
}

#[test]
fn common_order_with_message_and_strerror() {
    let mut fake = Fake::default();
    assert_eq!(warn_common(&mut fake, b"prog", 2, true, None), 0);
    assert_eq!(
        fake.bytes,
        b"prog: <fmt>: No such file or directory\n".to_vec()
    );
}

#[test]
fn common_without_message() {
    let mut fake = Fake::default();
    assert_eq!(warn_common(&mut fake, b"prog", 22, false, None), 0);
    assert_eq!(fake.bytes, b"prog: Invalid argument\n".to_vec());
}

#[test]
fn err_forms_return_eval() {
    let mut fake = Fake::default();
    assert_eq!(warn_common(&mut fake, b"p", 0, false, Some(7)), 7);
}

#[test]
fn explicit_forms_print_no_strerror() {
    let mut fake = Fake::default();
    assert_eq!(warn_explicit(&mut fake, b"prog", true, Some(3)), 3);
    assert_eq!(fake.bytes, b"prog: <fmt>\n".to_vec());
}

#[test]
fn unknown_errno_still_prints_a_text() {
    let mut fake = Fake::default();
    warn_common(&mut fake, b"p", 999, false, None);
    assert!(fake.bytes.starts_with(b"p: Unknown error: 999\n"));
}

#[test]
fn message_only_without_errno_text() {
    let mut fake = Fake::default();
    warn_explicit(&mut fake, b"p", true, None);
    assert_eq!(fake.bytes, b"p: <fmt>\n");
}

#[test]
fn no_message_with_strerror() {
    let mut fake = Fake::default();
    warn_common(&mut fake, b"p", 0, false, None);
    assert_eq!(fake.bytes, b"p: Success\n");
}

#[test]
fn explicit_code_uses_strerror() {
    let mut fake = Fake::default();
    warn_common(&mut fake, b"p", 22, false, None);
    assert_eq!(fake.bytes, b"p: Invalid argument\n");
}

#[test]
fn eval_is_returned_unchanged() {
    let mut fake = Fake::default();
    assert_eq!(warn_explicit(&mut fake, b"p", false, Some(42)), 42);
}
