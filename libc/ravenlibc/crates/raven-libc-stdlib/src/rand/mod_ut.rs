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

//! The random group surface: the control block, errors and the thirty-one-bit contract.

use super::*;

#[test]
fn control_block_defaults() {
    let ctl = RandomCtl::new();
    assert_eq!(ctl.r#type, random::TYPE_3);
    assert_eq!(ctl.deg, 31);
    assert_eq!(ctl.sep, 3);
    assert_eq!(ctl.words(), 32);
    assert_eq!(RandomCtl::default(), ctl);
}

#[test]
fn rand_err_is_comparable() {
    assert_eq!(RandErr::Invalid, RandErr::Invalid);
}

#[test]
fn max_types_covers_the_five_states() {
    assert_eq!(MAX_TYPES, 5);
    assert_eq!(random::TYPE_0, 0);
    assert_eq!(random::TYPE_4, 4);
}

#[test]
fn default_generator_has_a_type_word() {
    let generator = DefaultRandom::new();
    assert_eq!(generator.state[0], random::TYPE_3);
    assert_eq!(generator.state.len(), 32);
}

#[test]
fn x_to_double_is_reexported() {
    assert_eq!(x_to_double(0).to_bits(), 0.0f64.to_bits());
}

#[test]
fn default_random_is_deterministic() {
    let mut a = DefaultRandom::new();
    let mut b = DefaultRandom::new();
    assert_eq!(a.rand(), b.rand());
    assert_eq!(a.state, b.state);
}

#[test]
fn rng_constants_and_words() {
    assert_eq!(random::TYPE_0, 0);
    assert_eq!(random::TYPE_1, 1);
    assert_eq!(random::TYPE_2, 2);
    assert_eq!(random::TYPE_3, 3);
    assert_eq!(random::TYPE_4, 4);
}

#[test]
fn x_to_double_zero_and_one_bit() {
    assert_eq!(x_to_double(0), 0.0);
    assert_eq!(x_to_double(1), 2f64.powi(-48));
}

#[test]
fn rand48_default_multiplier() {
    assert_eq!(Rand48::new().a, 0x5dee_ce66d);
    assert_eq!(Rand48::new().c, 0xb);
}
