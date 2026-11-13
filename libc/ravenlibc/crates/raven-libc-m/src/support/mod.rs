// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Shared support for the math kernels: float and integer traits, rounding status, big integers.

#[macro_use]
pub mod macros;
mod big;
mod env;
mod float_traits;
pub mod hex_float;
mod int_traits;
mod modular;

#[allow(unused_imports)]
pub use big::{i256, u256};
pub use env::{FpResult, Round, Status};
#[allow(unused_imports)]
pub use float_traits::{DFloat, Float, HFloat, HalfRep, IntTy, f32_from_bits, f64_from_bits};
pub use hex_float::{DisplayHex, Hex, hf32, hf64};
pub use int_traits::{CastFrom, CastInto, DInt, HInt, Int, MinInt, NarrowingDiv};
pub use modular::linear_mul_reduction;

/// Hint to the compiler that the current path is cold.
#[inline]
pub fn cold_path() {}

/// Unsigned ceiling division, used by the hex-float parser.
#[inline]
pub fn div_ceil_u32(a: u32, b: u32) -> u32 {
    a.div_ceil(b)
}
