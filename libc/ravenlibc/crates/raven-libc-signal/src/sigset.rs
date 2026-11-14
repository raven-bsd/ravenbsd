// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/sigaddset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigdelset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigemptyset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigfillset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigismember.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigisemptyset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigandset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/sigorset.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The 128-byte `sigset_t` operations.
//!
//! musl stores `_NSIG = 65` signals in `unsigned long __bits[128/sizeof(long)]`; bit `n-1` is
//! signal `n`, words 1..15 stay zero, and the internal signals 32..34 are refused by
//! `sigaddset`/`sigdelset` but are ordinary members for `sigismember`. `sigfillset` stores
//! `0xfffffffc7fffffff`, which leaves the internal bits clear so an application full mask can
//! never block `SIGCANCEL` (A-sig-2).

use crate::errors::EINVAL_ERR;
use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::signal;

/// `_NSIG`: one past the highest signal number.
pub const NSIG: usize = 65;
/// Signal 32: the POSIX timer helper (libc-internal).
pub const SIGTIMER: i32 = 32;
/// Signal 33: cancellation (libc-internal).
pub const SIGCANCEL: i32 = 33;
/// Signal 34: `synccall` (libc-internal).
pub const SIGSYNCCALL: i32 = 34;
/// The first application-visible real-time signal.
pub const SIGRTMIN: i32 = 35;
/// The last real-time signal.
pub const SIGRTMAX: i32 = 64;
/// Application mask: signals 32-34 clear (`musl src/signal/block.c`).
pub const APP_MASK: u64 = 0xffff_fffc_7fff_ffff;
/// Kernel mask word with every bit set.
pub const ALL_MASK: u64 = !0;

/// A 128-byte signal set, layout-identical to `sigset_t`.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct SigSet(pub [u64; 16]);

impl SigSet {
    /// The empty set.
    #[must_use]
    pub const fn empty() -> Self {
        Self([0; 16])
    }

    /// The empty set (const alias used by statics).
    #[must_use]
    pub const fn new() -> Self {
        Self::empty()
    }

    /// Set every application signal, leaving 32..34 clear.
    pub fn fill(&mut self) {
        if let Some(word) = self.0.first_mut() {
            *word = APP_MASK;
        }
    }

    /// Add `sig`. `EINVAL` for `sig < 1`, `sig > 64` and 32..=34.
    pub fn add(&mut self, sig: i32) -> Result<(), Errno> {
        let shift = signal_shift(sig)?;
        if let Some(word) = self.0.first_mut() {
            *word |= 1u64.wrapping_shl(shift);
        }
        Ok(())
    }

    /// Delete `sig`. Same rejected numbers as [`SigSet::add`].
    pub fn del(&mut self, sig: i32) -> Result<(), Errno> {
        let shift = signal_shift(sig)?;
        if let Some(word) = self.0.first_mut() {
            *word &= !1u64.wrapping_shl(shift);
        }
        Ok(())
    }

    /// Whether `sig` is a member. Numbers outside 1..=64 are simply not members (musl
    /// `sigismember` does not set `errno`).
    pub fn is_member(&self, sig: i32) -> Result<bool, Errno> {
        if !(1..=64).contains(&sig) {
            return Ok(false);
        }
        let shift = (sig.wrapping_sub(1) & 63) as u32;
        Ok(self
            .0
            .first()
            .is_some_and(|word| word & 1u64.wrapping_shl(shift) != 0))
    }

    /// Whether every word is zero.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.iter().all(|word| *word == 0)
    }

    /// Word-wise AND (`sigandset`).
    #[must_use]
    pub fn and(&self, other: &Self) -> Self {
        Self(core::array::from_fn(|index| {
            self.0.get(index).copied().unwrap_or(0) & other.0.get(index).copied().unwrap_or(0)
        }))
    }

    /// Word-wise OR (`sigorset`).
    #[must_use]
    pub fn or(&self, other: &Self) -> Self {
        Self(core::array::from_fn(|index| {
            self.0.get(index).copied().unwrap_or(0) | other.0.get(index).copied().unwrap_or(0)
        }))
    }

    /// The application word (only signal numbers 1..64 live here).
    #[must_use]
    pub fn app_word(&self) -> u64 {
        self.0.first().copied().unwrap_or(0)
    }

    /// A copy with only `sig` set, or an empty set for a rejected number.
    #[must_use]
    pub fn singleton(sig: i32) -> Self {
        let mut set = Self::empty();
        let _ = set.add(sig);
        set
    }

    /// The ABI-generated view of the same bytes.
    #[must_use]
    pub const fn to_abi(self) -> raven_libc_abi::structs::SigsetT {
        raven_libc_abi::structs::SigsetT { bytes: self.0 }
    }

    /// Read the ABI view back.
    #[must_use]
    pub const fn from_abi(value: raven_libc_abi::structs::SigsetT) -> Self {
        Self(value.bytes)
    }

    /// The bit mask of this set's application word.
    #[must_use]
    pub fn bits(&self) -> u64 {
        self.0.first().copied().unwrap_or(0)
    }
}

impl Default for SigSet {
    fn default() -> Self {
        Self::empty()
    }
}

/// The musl `s-1`/`sig-32U < 3` gate, returning the bit position.
fn signal_shift(sig: i32) -> Result<u32, Errno> {
    if !(1..=64).contains(&sig) || (32..=34).contains(&sig) {
        return Err(EINVAL_ERR);
    }
    Ok((sig.wrapping_sub(1) & 63) as u32)
}

/// The `NSIG` value of the generated ABI must agree with this module.
const _: () = assert!(NSIG == signal::_NSIG as usize);
const _: () = assert!(core::mem::size_of::<SigSet>() == 128);

#[cfg(test)]
#[path = "sigset_ut.rs"]
mod sigset_ut;
