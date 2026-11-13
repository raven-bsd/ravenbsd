// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{BUF_LEN, Xsi, message, strerror_into, strerror_r_xsi, strsignal_into};

#[test]
fn buffer_is_64_bytes() {
    assert_eq!(BUF_LEN, 64);
}

#[test]
fn xsi_values_are_distinct() {
    assert_ne!(Xsi::Ok, Xsi::Einval);
    assert_ne!(Xsi::Ok, Xsi::Erange);
    assert_ne!(Xsi::Einval, Xsi::Erange);
}

#[test]
fn engine_entry_points_agree_on_known_text() {
    let mut error_buffer = [0u8; BUF_LEN];
    let (error_text, known) = strerror_into(2, &mut error_buffer);
    assert!(known);
    assert_eq!(Some(error_text), message(2));

    let mut xsi_buffer = [0u8; 32];
    assert_eq!(strerror_r_xsi(2, &mut xsi_buffer), Xsi::Ok);
    assert_eq!(&xsi_buffer[..error_text.len()], error_text);

    let mut signal_buffer = [0u8; BUF_LEN];
    let (signal_text, known) = strsignal_into(2, &mut signal_buffer);
    assert!(known);
    assert_eq!(signal_text, b"Interrupt");
}
