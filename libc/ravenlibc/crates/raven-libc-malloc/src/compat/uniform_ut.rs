// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use core::cell::Cell;

struct Bytes {
    value: Cell<u8>,
}

impl Entropy for Bytes {
    fn fill(&self, buf: &mut [u8]) -> bool {
        let mut index = 0usize;
        while index < buf.len() {
            buf[index] = self.value.get();
            index += 1;
        }
        true
    }
}

struct Failing;

impl Entropy for Failing {
    fn fill(&self, _buf: &mut [u8]) -> bool {
        false
    }
}

#[test]
fn zero_has_no_uniform_value() {
    assert_eq!(
        uniform(
            &Bytes {
                value: Cell::new(1)
            },
            0
        ),
        None
    );
}

#[test]
fn uniform_stays_below_n() {
    for n in 1u32..64 {
        let value = uniform(
            &Bytes {
                value: Cell::new(0xaa),
            },
            n,
        )
        .expect("value");
        assert!(value < n);
    }
}

#[test]
fn entropy_failure_is_reported() {
    assert_eq!(uniform(&Failing, 10), None);
}

#[test]
fn uniform_one_is_always_zero() {
    for _ in 0..10 {
        assert_eq!(
            uniform(
                &Bytes {
                    value: Cell::new(0xff)
                },
                1
            ),
            Some(0)
        );
    }
}

#[test]
fn uniform_rejects_a_zero_modulus_before_entropy() {
    assert_eq!(uniform(&Failing, 0), None);
}

#[test]
fn uniform_large_modulus_uses_the_rejection_path() {
    let value = uniform(
        &Bytes {
            value: Cell::new(0x12),
        },
        u32::MAX,
    )
    .expect("value");
    assert!(value < u32::MAX);
}
