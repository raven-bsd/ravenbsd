// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Clean-room `strlcpy` stand-in.

/// Copy `src` (already without its terminator) into `buf`.
///
/// At most `dsize - 1` bytes are copied and a NUL is written at their end unless `dsize == 0`.
/// Returns `src.len()`. Bytes at and after `dsize` are guard bytes and stay untouched. The caller
/// guarantees `buf.len() >= dsize`.
pub fn strlcpy(buf: &mut [u8], dsize: usize, src: &[u8]) -> usize {
    let length = src.len();
    if dsize > 0 {
        let copy = length.min(dsize - 1);
        if copy > 0 {
            buf[..copy].copy_from_slice(&src[..copy]);
        }
        buf[copy] = 0;
    }
    length
}

#[cfg(test)]
#[path = "strlcpy_ut.rs"]
mod tests;
