// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Broken `strlcpy` variants.

/// Copy at most `dsize - 1` bytes but omit the NUL when the source does not fit.
pub fn nonul(buf: &mut [u8], dsize: usize, src: &[u8]) -> usize {
    let length = src.len();
    if dsize > 0 {
        let copy = length.min(dsize - 1);
        if copy > 0 {
            buf[..copy].copy_from_slice(&src[..copy]);
        }
        if length < dsize {
            buf[copy] = 0;
        }
    }
    length
}

/// Correct copy but return the copied length instead of the source length.
pub fn ret(buf: &mut [u8], dsize: usize, src: &[u8]) -> usize {
    let length = src.len();
    if dsize > 0 {
        let copy = length.min(dsize - 1);
        if copy > 0 {
            buf[..copy].copy_from_slice(&src[..copy]);
        }
        buf[copy] = 0;
        copy
    } else {
        0
    }
}

/// Correct copy but write the terminator one byte past the allowed end on truncation.
pub fn overrun(buf: &mut [u8], dsize: usize, src: &[u8]) -> usize {
    let length = src.len();
    if dsize > 0 {
        let copy = length.min(dsize - 1);
        if copy > 0 {
            buf[..copy].copy_from_slice(&src[..copy]);
        }
        if length < dsize {
            buf[copy] = 0;
        } else if dsize < buf.len() {
            buf[dsize] = 0;
        }
    }
    length
}

/// Ignore the `dsize == 0` case and write a terminator anyway.
pub fn zero(buf: &mut [u8], dsize: usize, src: &[u8]) -> usize {
    let length = src.len();
    if dsize == 0 {
        if let Some(first) = buf.first_mut() {
            *first = 0;
        }
        return length;
    }
    let copy = length.min(dsize - 1);
    if copy > 0 {
        buf[..copy].copy_from_slice(&src[..copy]);
    }
    buf[copy] = 0;
    length
}

/// Calls one `strlcpy` variant.
type StrlcpyMutant = fn(&mut [u8], usize, &[u8]) -> usize;

/// Every broken variant with its API name.
pub const ALL: [(&str, StrlcpyMutant); 4] = [
    ("nonul", nonul),
    ("ret", ret),
    ("overrun", overrun),
    ("zero", zero),
];

#[cfg(test)]
#[path = "strlcpy_ut.rs"]
mod tests;
