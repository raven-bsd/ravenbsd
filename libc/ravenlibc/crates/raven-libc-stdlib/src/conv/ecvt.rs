// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ecvt_r`/`fcvt_r` digit layout.
//!
//! The caller converts the `double` to digits (the shell uses `raven-libc-dtoa`, the donor of
//! the formatting engine) and this module applies the two C layouts: `ecvt_r` writes exactly
//! `ndigit` significant digits with trailing zeros, `fcvt_r` writes the `ndecimal` digits after
//! the point. Both return the decimal-point position and the sign through [`Converted`]; the
//! shell writes the caller's `int *` slots.
//!
//! A non-finite value has no digits; the caller passes the substitute text (`inf`/`nan`) and the
//! layout truncates it to `ndigit` characters, with `*decpt` zero, as C libraries do.

use raven_libc_abi::errno::{EINVAL, Errno};

/// The result of one layout: the string length, decimal point and sign.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Converted {
    /// Bytes written before the terminating NUL.
    pub len: usize,
    /// Position of the decimal point relative to the first digit (C `ecvt_r`/`fcvt_r` rule).
    pub decpt: i32,
    /// Whether the value was negative (including `-0.0`).
    pub negative: bool,
}

/// `ecvt_r` layout: exactly `ndigit` digits, padded with zeros, NUL-terminated.
///
/// # Errors
///
/// [`EINVAL`] when the output cannot hold `ndigit` digits plus the terminating NUL.
pub fn ecvt_digits(
    ndigit: i32,
    negative: bool,
    digits: &[u8],
    decpt: i32,
    special: Option<&[u8]>,
    out: &mut [u8],
) -> Result<Converted, Errno> {
    let count = usize::try_from(ndigit.max(0)).map_err(|_| errno(EINVAL))?;
    if count >= out.len() {
        return Err(errno(EINVAL));
    }
    let (source, padded, length) = match special {
        // `inf`/`nan`: C libraries emit the text itself, not a zero-padded field.
        Some(text) => (text, false, text.len().min(count)),
        None => (digits, true, count),
    };
    let mut written = 0usize;
    while written < length {
        if let Some(slot) = out.get_mut(written) {
            *slot = source.get(written).copied().unwrap_or(b'0');
        }
        written = written.saturating_add(1);
    }
    if padded {
        while written < count {
            if let Some(slot) = out.get_mut(written) {
                *slot = b'0';
            }
            written = written.saturating_add(1);
        }
    }
    if let Some(slot) = out.get_mut(count) {
        *slot = 0;
    }
    Ok(Converted {
        len: written,
        decpt: if special.is_some() { 0 } else { decpt },
        negative,
    })
}

/// `fcvt_r` layout: the digits up to `ndecimal` places after the point, NUL-terminated.
///
/// A value that rounds to zero inside the requested precision produces the empty string and
/// `decpt = -ndecimal`, which is the C library convention.
///
/// # Errors
///
/// [`EINVAL`] when the output cannot hold the result plus the terminating NUL.
pub fn fcvt_digits(
    ndecimal: i32,
    negative: bool,
    digits: &[u8],
    decpt: i32,
    special: Option<&[u8]>,
    out: &mut [u8],
) -> Result<Converted, Errno> {
    if let Some(text) = special {
        let length = text.len();
        if length >= out.len() {
            return Err(errno(EINVAL));
        }
        for index in 0..length {
            if let (Some(slot), Some(byte)) = (out.get_mut(index), text.get(index)) {
                *slot = *byte;
            }
        }
        if let Some(slot) = out.get_mut(length) {
            *slot = 0;
        }
        return Ok(Converted {
            len: length,
            decpt: 0,
            negative,
        });
    }
    let places = i64::from(ndecimal.max(0));
    let total = i64::from(decpt).saturating_add(places);
    if total < 0 {
        if out.is_empty() {
            return Err(errno(EINVAL));
        }
        if let Some(slot) = out.first_mut() {
            *slot = 0;
        }
        return Ok(Converted {
            len: 0,
            decpt: 0i32.saturating_sub(i32::try_from(places).map_err(|_| errno(EINVAL))?),
            negative,
        });
    }
    let length = usize::try_from(total).map_err(|_| errno(EINVAL))?;
    if length >= out.len() {
        return Err(errno(EINVAL));
    }
    let mut written = 0usize;
    while written < length {
        let byte = digits.get(written).copied().unwrap_or(b'0');
        if let Some(slot) = out.get_mut(written) {
            *slot = byte;
        }
        written = written.saturating_add(1);
    }
    if let Some(slot) = out.get_mut(length) {
        *slot = 0;
    }
    Ok(Converted {
        len: length,
        decpt,
        negative,
    })
}

fn errno(value: i32) -> Errno {
    Errno(value as u16)
}

#[cfg(test)]
#[path = "ecvt_ut.rs"]
mod tests;
