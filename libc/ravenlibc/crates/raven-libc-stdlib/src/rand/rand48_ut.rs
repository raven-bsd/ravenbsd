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

//! The `rand48` family: recurrence, seeding, the caller-seed forms and bit-exact doubles.

use super::*;

fn words(x: u64) -> [u16; 3] {
    [
        (x & 0xffff) as u16,
        ((x >> 16) & 0xffff) as u16,
        ((x >> 32) & 0xffff) as u16,
    ]
}

fn from_words(w: &[u16; 3]) -> u64 {
    u64::from(w[0]) | (u64::from(w[1]) << 16) | (u64::from(w[2]) << 32)
}

#[test]
fn caller_seed_words_round_trip() {
    let mut generator = Rand48::new();
    let mut seed = words(0x1234_abcd_330e);
    let first = generator.step_with(&mut seed);
    assert_eq!(from_words(&seed), first);
    let second = generator.step_with(&mut seed);
    assert_ne!(first, second);
}

#[test]
fn default_state_is_posix() {
    let generator = Rand48::new();
    assert_eq!(generator.x, 0x1234_abcd_330e);
    assert_eq!(generator.a, 0x0000_0005_deec_e66d);
    assert_eq!(generator.c, 0xb);
}

#[test]
fn recurrence_is_mod_2_48() {
    let mut generator = Rand48::new();
    let mut x = generator.x;
    for _ in 0..1000 {
        x = (generator
            .a
            .wrapping_mul(x)
            .wrapping_add(u64::from(generator.c)))
            & 0xffff_ffff_ffff;
        assert_eq!(generator.step(), x);
        assert!(generator.x < (1u64 << 48));
    }
}

#[test]
fn srand48_sets_the_low_word() {
    let mut generator = Rand48::new();
    generator.srand48(0x1234_5678);
    assert_eq!(generator.x, (0x1234_5678u64 << 16) | 0x330e);
    generator.srand48(0);
    assert_eq!(generator.x, 0x330e);
    generator.srand48(-1);
    assert_eq!(generator.x, 0xffff_ffff_330e);
}

#[test]
fn srand48_restores_default_a_and_c() {
    let mut generator = Rand48::new();
    generator.lcong48(&[1, 2, 3, 4, 5, 6, 7]);
    generator.srand48(9);
    assert_eq!(generator.a, 0x0000_0005_deec_e66d);
    assert_eq!(generator.c, 0xb);
}

#[test]
fn seed48_returns_the_previous_seed() {
    let mut generator = Rand48::new();
    let before = words(generator.x);
    let returned = generator.seed48(&[0x1111, 0x2222, 0x3333]);
    assert_eq!(returned, before);
    assert_eq!(generator.x, 0x3333_2222_1111);
}

#[test]
fn lcong48_takes_seven_words() {
    let mut generator = Rand48::new();
    generator.lcong48(&[0x1111, 0x2222, 0x3333, 0x4444, 0x5555, 0x6666, 0x7777]);
    assert_eq!(generator.x, 0x3333_2222_1111);
    assert_eq!(generator.a, 0x6666_5555_4444);
    assert_eq!(generator.c, 0x7777);
}

#[test]
fn caller_seed_forms_update_the_array() {
    let mut generator = Rand48::new();
    let mut seed = [0x330e, 0x1234, 0x5678];
    let x = generator.step_with(&mut seed);
    assert_eq!(from_words(&seed), x);
}

#[test]
fn lrand48_is_the_top_31_bits() {
    let mut generator = Rand48::new();
    let expected = (generator.step() >> 17) as i64;
    generator = Rand48::new();
    assert_eq!(lrand48(&mut generator), expected);
    assert!(lrand48(&mut generator) >= 0);
}

#[test]
fn mrand48_is_signed_32_bits() {
    let mut generator = Rand48::new();
    let x = generator.step();
    let expected = i64::from((x >> 16) as i32);
    generator = Rand48::new();
    assert_eq!(mrand48(&mut generator), expected);
}

#[test]
fn caller_seed_forms_agree_with_the_state_forms() {
    let mut a = Rand48::new();
    let mut b = Rand48::new();
    let mut seed = words(b.x);
    assert_eq!(nrand48(&mut a, &mut seed), lrand48(&mut b));
    let mut a = Rand48::new();
    let mut b = Rand48::new();
    let mut seed = words(b.x);
    assert_eq!(jrand48(&mut a, &mut seed), mrand48(&mut b));
}

#[test]
fn x_to_double_is_exact_and_has_no_float_ops() {
    assert_eq!(x_to_double(0), 0.0);
    assert_eq!(x_to_double(1), 2f64.powi(-48));
    assert_eq!(x_to_double(1u64 << 47), 0.5);
    assert_eq!(x_to_double((1u64 << 48) - 1), 1.0 - 2f64.powi(-48));
    let mut generator = Rand48::new();
    for _ in 0..1000 {
        let x = generator.step();
        let expected = (x as f64) / 2f64.powi(48);
        assert_eq!(x_to_double(x).to_bits(), expected.to_bits());
    }
}

#[test]
fn drand48_matches_x_to_double() {
    let mut generator = Rand48::new();
    let x = generator.step();
    generator = Rand48::new();
    assert_eq!(drand48(&mut generator).to_bits(), x_to_double(x).to_bits());
    let mut generator = Rand48::new();
    let mut seed = words(0x1234_abcd_330e);
    let x = generator.step_with(&mut seed);
    generator = Rand48::new();
    let mut seed = words(0x1234_abcd_330e);
    assert_eq!(
        erand48(&mut generator, &mut seed).to_bits(),
        x_to_double(x).to_bits()
    );
}
