// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Safe replacements for the donor's `force_eval!` and `i!` macros.
//!
//! `force_eval!` existed to defeat constant folding (and, in the C original, to touch memory so
//! the x87 excess precision would round). Rust evaluates in the target's precision and never
//! contracts, so a `black_box` is the faithful replacement for the folding part; the exception
//! side is a separate `fp::raise` call (A-num-4) and is not hidden in a macro.
//!
//! `i!` performed unchecked indexing in release builds. The index is bounded by construction in
//! every caller (tables indexed by a computed digit), so plain indexing is used; a stray index is
//! a trap, not undefined behaviour.

macro_rules! force_eval {
    ($e:expr) => {
        ::core::hint::black_box($e)
    };
}

macro_rules! i {
    ($array:expr, $index:expr) => {
        $array[$index]
    };
    ($array:expr, $index:expr, = , $rhs:expr) => {
        $array[$index] = $rhs
    };
    ($array:expr, $index:expr, += , $rhs:expr) => {
        $array[$index] += $rhs
    };
    ($array:expr, $index:expr, -= , $rhs:expr) => {
        $array[$index] -= $rhs
    };
    ($array:expr, $index:expr, &= , $rhs:expr) => {
        $array[$index] &= $rhs
    };
    ($array:expr, $index:expr, == , $rhs:expr) => {
        $array[$index] == $rhs
    };
}
