// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `getexecname` and `getisax`: both are reads of the saved auxiliary vector.
//!
//! CLEANROOM: `06-solaris-design.md` A-sol-4 says the executable name and the CPU-capability
//! bits are values the loader placed in the auxiliary vector; the functions never open a file,
//! never read `/proc` and never copy the loader's string (`PG-50`, `PG-51`).
//!
//! Auxiliary-vector keys are the Linux numeric ABI of `PG-50` (16, 26, 31), not a Solaris header
//! fact. No `AV_*` capability name is defined or promised: the bit meaning is the architecture's
//! own `AT_HWCAP` meaning (C67 5.A).

use crate::seams::AuxSrc;

/// `AT_HWCAP`: the first CPU-capability word.
pub const AT_HWCAP: u64 = 16;
/// `AT_HWCAP2`: the second CPU-capability word.
pub const AT_HWCAP2: u64 = 26;
/// `AT_EXECFN`: the executable path name pointer.
pub const AT_EXECFN: u64 = 31;

/// `getexecname()`: the saved `AT_EXECFN` pointer, or `NULL` when the key is absent.
///
/// The returned address points into the loader's data; the string is not copied. A caller that
/// gets `None` treats it as the C `NULL` return.
#[must_use]
pub fn getexecname<A: AuxSrc>(aux: &A) -> Option<usize> {
    aux.get(AT_EXECFN)
        .and_then(|value| usize::try_from(value).ok())
}

/// `getisax(uint32_t *array, uint_t n)`: fill at most three words and return how many.
///
/// PROPOSED layout (C67 5.A): word 0 is the low 32 bits of `AT_HWCAP`, word 1 the high 32 bits
/// of `AT_HWCAP`, word 2 the low 32 bits of `AT_HWCAP2`. The number of words written is
/// `min(n, 3)`; when `AT_HWCAP` is absent nothing is written and the result is 0 (`PG-51`).
/// An absent `AT_HWCAP2` writes a zero third word.
#[must_use]
pub fn getisax<A: AuxSrc>(aux: &A, out: &mut [u32]) -> usize {
    let Some(hwcap) = aux.get(AT_HWCAP) else {
        return 0;
    };
    let high = hwcap.wrapping_shr(32);
    let words = [
        u32::try_from(hwcap & 0xffff_ffff).unwrap_or(u32::MAX),
        u32::try_from(high).unwrap_or(u32::MAX),
        u32::try_from(aux.get(AT_HWCAP2).unwrap_or(0) & 0xffff_ffff).unwrap_or(u32::MAX),
    ];
    let count = out.len().min(words.len());
    for (slot, value) in out.iter_mut().zip(words.iter()).take(count) {
        *slot = *value;
    }
    count
}

#[cfg(test)]
#[path = "identity_ut.rs"]
mod tests;
