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

//! `vwarnc` delegates to the shared writer.

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
fn writes_in_the_donor_order() {
    let mut fake = Fake::default();
    let status = vwarnc(&mut fake, b"prog", 22, false);
    assert_eq!(status, 0);
    assert_eq!(fake.bytes, b"prog: Invalid argument\n".to_vec());
}
