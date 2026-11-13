// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C23 bit-level additions: total ordering, canonicalisation, NaN payloads and `nextafter`
//! neighbours.
//!
//! The ordering is the IEEE 754-2019 `totalOrder` relation: negative NaNs sort below `-inf`,
//! `-0` below `+0`, and positive NaNs above `+inf`; within the NaNs the payload orders the value.
//! `totalordermag` compares magnitudes and uses the sign only to break a tie.
//!
//! Written from the C23 7.12.11/7.12.12 definitions for RavenBSD. No donor code was copied.

const F64_SIGN: u64 = 1 << 63;
const F64_EXP: u64 = 0x7ff0_0000_0000_0000;
const F64_MANTISSA: u64 = 0x000f_ffff_ffff_ffff;
const F64_QUIET: u64 = 0x0008_0000_0000_0000;
const F64_PAYLOAD_MAX: u64 = 1 << 51;

const F32_SIGN: u32 = 1 << 31;
const F32_EXP: u32 = 0x7f80_0000;
const F32_MANTISSA: u32 = 0x007f_ffff;
const F32_QUIET: u32 = 0x0040_0000;
const F32_PAYLOAD_MAX: u32 = 1 << 22;

fn order_key64(x: f64) -> u64 {
    let bits = x.to_bits();
    if bits & F64_SIGN != 0 {
        !bits
    } else {
        bits ^ F64_SIGN
    }
}

fn order_key32(x: f32) -> u32 {
    let bits = x.to_bits();
    if bits & F32_SIGN != 0 {
        !bits
    } else {
        bits ^ F32_SIGN
    }
}

/// C23 `totalorder(x, y)`.
pub fn totalorder(x: f64, y: f64) -> bool {
    order_key64(x) <= order_key64(y)
}

/// C23 `totalorderf(x, y)`.
pub fn totalorderf(x: f32, y: f32) -> bool {
    order_key32(x) <= order_key32(y)
}

/// C23 `totalordermag(x, y)`.
pub fn totalordermag(x: f64, y: f64) -> bool {
    let ax = x.to_bits() & !F64_SIGN;
    let ay = y.to_bits() & !F64_SIGN;
    if ax != ay {
        return ax < ay;
    }
    x.to_bits() & F64_SIGN != 0 && y.to_bits() & F64_SIGN == 0
}

/// C23 `totalordermagf(x, y)`.
pub fn totalordermagf(x: f32, y: f32) -> bool {
    let ax = x.to_bits() & !F32_SIGN;
    let ay = y.to_bits() & !F32_SIGN;
    if ax != ay {
        return ax < ay;
    }
    x.to_bits() & F32_SIGN != 0 && y.to_bits() & F32_SIGN == 0
}

/// C23 `canonicalize(x)`: the canonical quiet NaN for a NaN, the value otherwise.
pub fn canonicalize(x: f64) -> f64 {
    if x.is_nan() { f64::NAN } else { x }
}

/// C23 `canonicalizef(x)`.
pub fn canonicalizef(x: f32) -> f32 {
    if x.is_nan() { f32::NAN } else { x }
}

/// C23 `issignaling(x)`.
pub fn issignaling(x: f64) -> bool {
    x.is_nan() && (x.to_bits() & F64_QUIET) == 0
}

/// C23 `issignalingf(x)`.
pub fn issignalingf(x: f32) -> bool {
    x.is_nan() && (x.to_bits() & F32_QUIET) == 0
}

/// C23 `getpayload(x)`: the NaN mantissa bits, quiet bit included. Zero for a non-NaN.
pub fn getpayload(x: f64) -> u64 {
    if x.is_nan() {
        x.to_bits() & F64_MANTISSA
    } else {
        0
    }
}

/// C23 `getpayloadf(x)`.
pub fn getpayloadf(x: f32) -> u32 {
    if x.is_nan() {
        x.to_bits() & F32_MANTISSA
    } else {
        0
    }
}

/// C23 `setpayload(x, payload)`: a positive quiet NaN with `payload`.
///
/// A zero payload is the C rule for "no payload" and yields `+0.0`; a payload that does not fit is
/// the canonical NaN.
pub fn setpayload(x: f64, payload: u64) -> f64 {
    if payload == 0 {
        return 0.0;
    }
    if payload >= F64_PAYLOAD_MAX {
        return f64::NAN;
    }
    let _ = x;
    f64::from_bits(F64_EXP | F64_QUIET | payload)
}

/// C23 `setpayloadf(x, payload)`.
pub fn setpayloadf(x: f32, payload: u32) -> f32 {
    if payload == 0 {
        return 0.0;
    }
    if payload >= F32_PAYLOAD_MAX {
        return f32::NAN;
    }
    let _ = x;
    f32::from_bits(F32_EXP | F32_QUIET | payload)
}

/// C23 `setpayloadsig(x, payload)`: a positive signalling NaN with `payload`.
pub fn setpayloadsig(x: f64, payload: u64) -> f64 {
    if payload == 0 {
        return 0.0;
    }
    if payload >= F64_PAYLOAD_MAX {
        return f64::from_bits(F64_EXP | 1);
    }
    let _ = x;
    f64::from_bits(F64_EXP | payload)
}

/// C23 `setpayloadsigf(x, payload)`.
pub fn setpayloadsigf(x: f32, payload: u32) -> f32 {
    if payload == 0 {
        return 0.0;
    }
    if payload >= F32_PAYLOAD_MAX {
        return f32::from_bits(F32_EXP | 1);
    }
    let _ = x;
    f32::from_bits(F32_EXP | payload)
}

/// C23 `nextup(x)`.
pub fn nextup(x: f64) -> f64 {
    if x.is_nan() || x == f64::INFINITY {
        return x;
    }
    if x == 0.0 {
        return f64::from_bits(1);
    }
    if x > 0.0 {
        f64::from_bits(x.to_bits() + 1)
    } else {
        f64::from_bits(x.to_bits() - 1)
    }
}

/// C23 `nextdown(x)`.
pub fn nextdown(x: f64) -> f64 {
    if x.is_nan() || x == f64::NEG_INFINITY {
        return x;
    }
    if x == 0.0 {
        return -f64::from_bits(1);
    }
    if x > 0.0 {
        f64::from_bits(x.to_bits() - 1)
    } else {
        f64::from_bits(x.to_bits() + 1)
    }
}

/// C23 `roundeven(x)`: round to the nearest integer, ties to even.
pub fn roundeven(x: f64) -> f64 {
    let whole = crate::trunc(x);
    let fraction = x - whole;
    if fraction == 0.5 {
        if (whole % 2.0) == 0.0 {
            whole
        } else {
            whole + 1.0
        }
    } else if fraction == -0.5 {
        if (whole % 2.0) == 0.0 {
            whole
        } else {
            whole - 1.0
        }
    } else {
        crate::round(x)
    }
}

#[cfg(test)]
#[path = "c23_ut.rs"]
mod tests;
