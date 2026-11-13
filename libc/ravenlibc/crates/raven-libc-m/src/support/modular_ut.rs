// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/support/modular.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (tests moved out of modular.rs)

use crate::support::linear_mul_reduction;
use crate::support::modular::Reducer;

#[test]
fn reducer_ops() {
    for n in 33..=63_u8 {
        for x in 0..2 * n {
            let temp = Reducer::new(x, n);
            let n = n as u32;
            let x0 = temp.partial_remainder() as u32;
            assert_eq!(x as u32, x0);
            for k in 0..=7 {
                let mut red = temp.clone();
                let u = red.shift_reduce(k) as u32;
                let x1 = red.partial_remainder() as u32;
                assert_eq!(x1, (x0 << k) - u * n);
                assert!(x1 < 2 * n);
                assert!((red._2xq as u32).is_multiple_of(2 * x1));

                // `word_reduce` is equivalent to `shift_reduce(U::BITS - 1)`
                if k == 7 {
                    let mut alt = temp.clone();
                    let w = alt.word_reduce();
                    assert_eq!(u, w as u32);
                    assert_eq!(alt, red);
                }
            }
        }
    }
}

#[test]
fn reduction_u8() {
    for y in 1..64u8 {
        for x in 0..2 * y {
            let mut r = x % y;
            for e in 0..100 {
                assert_eq!(r, linear_mul_reduction(x, e, y));
                r <<= 1;
                if r >= y {
                    r -= y;
                }
            }
        }
    }
}

#[test]
fn reduction_u128() {
    assert_eq!(
        linear_mul_reduction::<u128>(17, 100, 123456789),
        (17 << 100) % 123456789
    );
    assert_eq!(
        linear_mul_reduction(0xdead_beef, 100, 1_u128 << 116),
        0xbeef << 100
    );
    let x = 10_u128.pow(37);
    let y = 11_u128.pow(36);
    assert!(x < y);
    let mut r = x;
    for e in 0..1000 {
        assert_eq!(r, linear_mul_reduction(x, e, y));
        r <<= 1;
        if r >= y {
            r -= y;
        }
        assert!(r != 0);
    }
}
