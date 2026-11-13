// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The per-call arena (`DtoaCtx`).
//!
//! Replaces the donor's global `freelist[]`, `private_mem` bump pool and `p5s` power-of-five list
//! (`misc.c:36-42,392`) and both `__gdtoa_locks` with a value the CALLER places on its stack. A
//! `Bigint` is a block of `5 + 2^k` words: header `[link, k, maxwds, sign, wds]` followed by
//! `x[0 .. 2^k]`. A [`BigRef`] is the word index of the header in a unified address space: indices
//! below [`POOL_WORDS`] address `pool`, larger indices address the spill vector (feature `alloc`).
//! No pointers, no references stored, no globals.

use crate::config::KMAX;

/// Words in the 8 KiB pool (`POOL_BYTES / 4`, spelled out to keep the lint surface small).
pub const POOL_WORDS: usize = 2048;

/// Sentinel "no block".
const NIL: u32 = u32::MAX;

/// Words of a `Bigint` header: link, k, maxwds, sign, wds.
const HDR_WORDS: usize = 5;

/// Number of size-class heads in `free` (0..=`KBIG`).
const FREE_CLASSES: usize = 15;

/// A `Bigint` reference: the word index of its header in the arena address space.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct BigRef(pub(crate) u32);

impl BigRef {
    /// The "no block" sentinel (C's `NULL`).
    pub(crate) const NIL: Self = Self(NIL);

    /// Whether this is the sentinel.
    #[must_use]
    pub(crate) const fn is_nil(self) -> bool {
        self.0 == NIL
    }
}

/// The per-call arena, free lists and lazy power-of-five chain.
///
/// Create one per conversion on the caller's stack: `let mut ctx = DtoaCtx::new();`. The value
/// holds no pointers and is [`Send`]; a reused context gives byte-identical results to a fresh one.
pub struct DtoaCtx {
    /// 8 KiB word pool (bump-allocated, two-word aligned like the donor's `double`).
    pool: [u32; POOL_WORDS],
    /// Bump pointer in words.
    top: usize,
    /// High-water mark of `top` (test/diagnostic use).
    peak: usize,
    /// Head index per size class; `NIL` means empty.
    free: [u32; FREE_CLASSES],
    /// Head of the lazily built `625, 5^8, 5^16, ...` chain.
    p5: u32,
    /// Set after a failed allocation; every later conversion should use a fresh context.
    failed: bool,
    /// Spill words (feature `alloc` only).
    #[cfg(feature = "alloc")]
    spill: alloc::vec::Vec<u32>,
    /// Bump pointer of the spill vector, in words.
    #[cfg(feature = "alloc")]
    spill_top: usize,
}

impl DtoaCtx {
    /// A fresh, empty context.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool: [0u32; POOL_WORDS],
            top: 0,
            peak: 0,
            free: [NIL; FREE_CLASSES],
            p5: NIL,
            failed: false,
            #[cfg(feature = "alloc")]
            spill: alloc::vec::Vec::new(),
            #[cfg(feature = "alloc")]
            spill_top: 0,
        }
    }

    /// The header word at `offset` (0 = link, 1 = k, 2 = maxwds, 3 = sign, 4 = wds).
    #[inline(always)]
    pub(crate) fn w(&self, r: BigRef, offset: u32) -> u32 {
        let index = (r.0 as usize).wrapping_add(offset as usize);
        self.word(index)
    }

    /// Write one header word.
    #[inline(always)]
    pub(crate) fn set_w(&mut self, r: BigRef, offset: u32, value: u32) {
        let index = (r.0 as usize).wrapping_add(offset as usize);
        self.set_word(index, value);
    }

    /// `x[i]`: the `i`-th significand word.
    #[inline(always)]
    pub(crate) fn x(&self, r: BigRef, i: u32) -> u32 {
        let index = (r.0 as usize)
            .wrapping_add(HDR_WORDS)
            .wrapping_add(i as usize);
        self.word(index)
    }

    /// Write `x[i]`.
    #[inline(always)]
    pub(crate) fn set_x(&mut self, r: BigRef, i: u32, value: u32) {
        let index = (r.0 as usize)
            .wrapping_add(HDR_WORDS)
            .wrapping_add(i as usize);
        self.set_word(index, value);
    }

    /// The `next` link of a `Bigint` (free list or the donor's `p5s` chain).
    #[inline(always)]
    pub(crate) fn link(&self, r: BigRef) -> u32 {
        self.w(r, 0)
    }

    /// Set the `next` link.
    #[inline(always)]
    pub(crate) fn set_link(&mut self, r: BigRef, value: u32) {
        self.set_w(r, 0, value);
    }

    /// The donor's `b->k` (size-class exponent).
    pub(crate) fn k(&self, r: BigRef) -> i32 {
        self.w(r, 1) as i32
    }

    /// The donor's `b->maxwds`.
    pub(crate) fn maxwds(&self, r: BigRef) -> i32 {
        self.w(r, 2) as i32
    }

    /// The donor's `b->sign`.
    pub(crate) fn sign(&self, r: BigRef) -> i32 {
        self.w(r, 3) as i32
    }

    /// The donor's `b->wds`.
    pub(crate) fn wds(&self, r: BigRef) -> i32 {
        self.w(r, 4) as i32
    }

    /// Set `b->k`.
    pub(crate) fn set_k(&mut self, r: BigRef, value: i32) {
        self.set_w(r, 1, value as u32);
    }

    /// Set `b->maxwds`.
    pub(crate) fn set_maxwds(&mut self, r: BigRef, value: i32) {
        self.set_w(r, 2, value as u32);
    }

    /// Set `b->sign`.
    pub(crate) fn set_sign(&mut self, r: BigRef, value: i32) {
        self.set_w(r, 3, value as u32);
    }

    /// Set `b->wds`.
    pub(crate) fn set_wds(&mut self, r: BigRef, value: i32) {
        self.set_w(r, 4, value as u32);
    }

    /// The head of the context's power-of-five chain.
    pub(crate) fn p5_head(&self) -> u32 {
        self.p5
    }

    /// Replace the head of the power-of-five chain.
    pub(crate) fn set_p5_head(&mut self, value: u32) {
        self.p5 = value;
    }

    /// Whether an allocation has failed in this context (a failed context should be dropped).
    #[doc(hidden)]
    #[must_use]
    pub fn failed(&self) -> bool {
        self.failed
    }

    /// Pool high-water mark in words (never above [`POOL_WORDS`]); for the arena tests and the
    /// size report.
    #[doc(hidden)]
    #[must_use]
    pub fn peak_words(&self) -> usize {
        self.peak
    }

    /// Allocate a `Bigint` of size class `k` (`2^k` significand words), or `None` (C's `NULL`).
    pub(crate) fn balloc(&mut self, k: i32) -> Option<BigRef> {
        if k < 0 {
            self.failed = true;
            return None;
        }
        let class = k as usize;
        if class < FREE_CLASSES {
            let head = self.free.get(class).copied().unwrap_or(NIL);
            if head != NIL {
                let r = BigRef(head);
                let next = self.link(r);
                if let Some(slot) = self.free.get_mut(class) {
                    *slot = next;
                }
                self.set_link(r, NIL);
                self.set_sign(r, 0);
                self.set_wds(r, 0);
                return Some(r);
            }
        }
        let shift = u32::try_from(k).ok()?;
        let maxwds = 1usize.checked_shl(shift)?;
        let need = HDR_WORDS.checked_add(maxwds)?;
        let aligned = align_two(self.top);
        if let Some(end) = aligned.checked_add(need)
            && end <= POOL_WORDS
        {
            self.top = end;
            if end > self.peak {
                self.peak = end;
            }
            let r = BigRef(aligned as u32);
            self.init_header(r, k, maxwds);
            return Some(r);
        }
        #[cfg(feature = "alloc")]
        if let Some(r) = self.balloc_spill(k, maxwds, need) {
            return Some(r);
        }
        self.failed = true;
        None
    }

    /// Release a block. Size classes above [`KMAX`] really release: the last bump allocation is
    /// rolled back, others go on the extended per-class list.
    pub(crate) fn bfree(&mut self, r: BigRef) {
        if r.is_nil() {
            return;
        }
        let k = self.k(r);
        if k < 0 {
            return;
        }
        let size = HDR_WORDS.wrapping_add(self.maxwds(r) as usize);
        let base = r.0 as usize;
        if k > KMAX {
            if base >= POOL_WORDS {
                #[cfg(feature = "alloc")]
                {
                    let spill_base = base.wrapping_sub(POOL_WORDS);
                    if spill_base.wrapping_add(size) == self.spill_top {
                        self.spill_top = spill_base;
                        return;
                    }
                }
            } else if base.wrapping_add(size) == self.top {
                self.top = base;
                return;
            }
        }
        let class = k as usize;
        if class < FREE_CLASSES {
            let head = self.free.get(class).copied().unwrap_or(NIL);
            self.set_link(r, head);
            if let Some(slot) = self.free.get_mut(class) {
                *slot = r.0;
            }
        }
    }

    /// Word read across pool and spill.
    #[inline(always)]
    fn word(&self, index: usize) -> u32 {
        if index < POOL_WORDS {
            return self.pool.get(index).copied().unwrap_or(0);
        }
        #[cfg(feature = "alloc")]
        {
            let spill_index = index.wrapping_sub(POOL_WORDS);
            self.spill.get(spill_index).copied().unwrap_or(0)
        }
        #[cfg(not(feature = "alloc"))]
        {
            0
        }
    }

    /// Word write across pool and spill.
    #[inline(always)]
    fn set_word(&mut self, index: usize, value: u32) {
        if index < POOL_WORDS {
            if let Some(slot) = self.pool.get_mut(index) {
                *slot = value;
            }
        } else {
            #[cfg(feature = "alloc")]
            {
                let spill_index = index.wrapping_sub(POOL_WORDS);
                if let Some(slot) = self.spill.get_mut(spill_index) {
                    *slot = value;
                }
            }
        }
    }

    /// Fill a freshly bumped block header.
    fn init_header(&mut self, r: BigRef, k: i32, maxwds: usize) {
        self.set_link(r, NIL);
        self.set_k(r, k);
        let maxwds_i32 = i32::try_from(maxwds).unwrap_or(i32::MAX);
        self.set_maxwds(r, maxwds_i32);
        self.set_sign(r, 0);
        self.set_wds(r, 0);
    }

    /// Bump-allocate from the spill vector (feature `alloc`).
    #[cfg(feature = "alloc")]
    fn balloc_spill(&mut self, k: i32, maxwds: usize, need: usize) -> Option<BigRef> {
        let aligned = align_two(self.spill_top);
        let end = aligned.checked_add(need)?;
        if end > self.spill.len() {
            let extra = end.wrapping_sub(self.spill.len());
            if self.spill.try_reserve(extra).is_err() {
                self.failed = true;
                return None;
            }
            self.spill.resize(end, 0);
        }
        self.spill_top = end;
        let index = POOL_WORDS.checked_add(aligned)?;
        let r = BigRef(u32::try_from(index).ok()?);
        self.init_header(r, k, maxwds);
        Some(r)
    }
}

impl Default for DtoaCtx {
    fn default() -> Self {
        Self::new()
    }
}

/// The donor aligns `Bigint` blocks to `sizeof(double)`.
fn align_two(value: usize) -> usize {
    value.wrapping_add(1) & !1usize
}

#[cfg(test)]
#[path = "ctx_ut.rs"]
mod tests;
