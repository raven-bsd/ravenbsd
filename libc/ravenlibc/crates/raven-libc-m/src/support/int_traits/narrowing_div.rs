// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/support/int_traits/narrowing_div.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
// (chosen option: MIT; upstream SPDX line kept above; the unsafe precondition became a safe check)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of this software
// and associated documentation files (the "Software"), to deal in the Software without
// restriction, including without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all copies or
// substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED.

use crate::support::{CastInto, DInt, HInt, Int, MinInt, u256};

/// Trait for unsigned division of a double-wide integer when the quotient does not overflow.
///
/// This is the inverse of widening multiplication:
///  - for any `x` and nonzero `y`: `x.widen_mul(y).checked_narrowing_div_rem(y) == Some((x, 0))`,
///  - and for any `r in 0..y`: `x.carrying_mul(y, r).checked_narrowing_div_rem(y) == Some((x, r))`.
pub trait NarrowingDiv: DInt + MinInt<Unsigned = Self> {
    /// Returns `Some((self / n, self % n))` when `self.hi() < n`.
    fn checked_narrowing_div_rem(self, n: Self::H) -> Option<(Self::H, Self::H)>;
}

// For primitive types the double-wide type is a native integer, so the standard operators do the
// work after the overflow precondition is checked.
macro_rules! impl_narrowing_div_primitive {
    ($D:ident) => {
        impl NarrowingDiv for $D {
            fn checked_narrowing_div_rem(self, n: Self::H) -> Option<(Self::H, Self::H)> {
                if self.hi() >= n || n == 0 {
                    return None;
                }
                Some(((self / n.widen()).cast(), (self % n.widen()).cast()))
            }
        }
    };
}

// Extend division from `u2N / uN` to `u4N / u2N`. Not the most efficient algorithm, but simple.
macro_rules! impl_narrowing_div_recurse {
    ($D:ident) => {
        impl NarrowingDiv for $D {
            fn checked_narrowing_div_rem(self, n: Self::H) -> Option<(Self::H, Self::H)> {
                if self.hi() >= n || n == 0 {
                    return None;
                }

                // Normalize the divisor by shifting its most significant one to the leading
                // position; `n != 0` is implied by `self.hi() < n`.
                let lz = n.leading_zeros();
                let a = self << lz;
                let b = n << lz;

                let ah = a.hi();
                let (a0, a1) = a.lo().lo_hi();
                let (q1, r) = div_three_digits_by_two(a1, ah, b);
                let (q0, r) = div_three_digits_by_two(a0, r, b);

                // Undo the earlier normalization for the remainder.
                Some((Self::H::from_lo_hi(q0, q1), r >> lz))
            }
        }
    };
}

impl_narrowing_div_primitive!(u16);
impl_narrowing_div_primitive!(u32);
impl_narrowing_div_primitive!(u64);
impl_narrowing_div_primitive!(u128);
impl_narrowing_div_recurse!(u256);

/// Implement `u3N / u2N`-division on top of `u2N / uN`-division.
///
/// Returns the quotient and remainder of `(a * R + a0) / n`, where `R = (1 << U::BITS)` is the
/// digit size. The caller guarantees `n.leading_zeros() == 0` and `a < n`; a violation is a
/// debug assertion (the original donor used unchecked undefined behaviour).
fn div_three_digits_by_two<U>(a0: U, a: U::D, n: U::D) -> (U, U::D)
where
    U: HInt,
    U::D: Int + NarrowingDiv,
{
    debug_assert!(n.leading_zeros() == 0 && a < n);

    // n = n1R + n0
    let (n0, n1) = n.lo_hi();
    // a = a2R + a1
    let (a1, a2) = a.lo_hi();

    let mut q;
    let mut r;
    let mut wrap;
    // `a < n` is guaranteed by the caller, but `a2 == n1 && a1 < n0` is possible.
    if let Some((q0, r1)) = a.checked_narrowing_div_rem(n1) {
        q = q0;
        // a = qn1 + r1, where 0 <= r1 < n1

        // Include the remainder with the low bits:
        // r = a0 + r1R
        r = U::D::from_lo_hi(a0, r1);

        // Subtract the contribution of the divisor low bits with the estimated quotient.
        let d = q.widen_mul(n0);
        (r, wrap) = r.overflowing_sub(d);

        // Since `q` is the quotient of dividing with a slightly smaller divisor, it may be an
        // overapproximation, but is never too small; `r` is either the correct remainder ...
        if !wrap {
            return (q, r);
        }
        // ... or the remainder went "negative" (by as much as `d = qn0 < RR`) and needs adjusting.
        q -= U::ONE;
    } else {
        debug_assert!(a2 == n1 && a1 < n0);
        // Otherwise `a2 == n1`, and the estimated quotient would be `R + (a1 % n1)`, but the
        // correct quotient cannot overflow. Start from `q = R = (1 << U::BITS)`, so
        // `r = aR + a0 - qn = (a - n)R + a0`.
        r = U::D::from_lo_hi(a0, a1.wrapping_sub(n0));
        // Since `a < n`, the first decrement is always needed.
        q = U::MAX; /* R - 1 */
    }

    (r, wrap) = r.overflowing_add(n);
    if wrap {
        return (q, r);
    }

    // If the remainder still did not wrap, one more step is needed.
    q -= U::ONE;
    (r, wrap) = r.overflowing_add(n);
    // Since `n >= RR/2`, at least one of the two `r += n` must have wrapped.
    debug_assert!(wrap, "estimated quotient should be off by at most two");
    (q, r)
}
