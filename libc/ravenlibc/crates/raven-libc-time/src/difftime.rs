// SPDX-License-Identifier: LicenseRef-PublicDomain
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from netbsd:lib/libc/time/difftime.c @
// 36eddebcc36cb0775aa2962005b9c05419a6ff01. The donor file is in the public
// domain, so clarified as of 1996-06-05 by Arthur David Olson.

//! `difftime`: subtract in integers, convert once.
#![allow(clippy::arithmetic_side_effects)] // RavenBSD: the wrapping subtraction is the donor's exact path.

/// Return `t1 - t0` as a `double`.
///
/// The integer subtraction happens first so values near 2^53 keep the bits a
/// single conversion can represent. The function is soft-float clean; it uses
/// no SSE-only operation.
#[must_use]
pub fn difftime(t1: i64, t0: i64) -> f64 {
    let (magnitude, negative) = if t0 <= t1 {
        ((t1 as u64).wrapping_sub(t0 as u64), false)
    } else {
        ((t0 as u64).wrapping_sub(t1 as u64), true)
    };
    let value = magnitude as f64;
    if negative { -value } else { value }
}

#[cfg(test)]
#[path = "difftime_ut.rs"]
mod tests;
