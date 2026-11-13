// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/env/__init_tls.c, src/env/__reset_tls.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! Copy placement for the TLS block (`__copy_tls`) and re-initialisation after `fork`.
//!
//! The plan is pure arithmetic and fully testable without a mapping; [`crate::raw`] executes it.
//! Variant I places the DTV at the top of the area and the TCB just above `mem` after an
//! alignment adjustment; variant II places the DTV at `mem` and the TCB at an aligned address
//! near the end. The initial image is copied to the module block and the rest of `.tbss` is
//! zero; a reused block is zeroed first (A-tls-4).

use crate::consts::{Abi, MAX_TLS_MODULES, TARGET_ABI};
use crate::layout::{LayoutError, StaticLayout, TlsModuleIn, module_offsets_for};

/// Where `__copy_tls` will write each object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CopyPlan {
    /// Address of the `Pthread` structure.
    pub tcb: usize,
    /// Address of DTV word 0 (the module count).
    pub dtv: usize,
    /// Address of each module block.
    pub blocks: [usize; MAX_TLS_MODULES],
    /// Number of valid entries in [`Self::blocks`].
    pub len: usize,
    /// Number of DTV words (`module count + 1`).
    pub words: usize,
}

/// Compute the copy placement without touching memory.
///
/// # Errors
///
/// [`LayoutError`] when a module or the total size is invalid.
pub fn plan_copy(
    layout: &StaticLayout,
    mem: usize,
    tcb_size: usize,
    mods: &[TlsModuleIn],
) -> Result<CopyPlan, LayoutError> {
    plan_copy_for(TARGET_ABI, layout, mem, tcb_size, mods)
}

/// [`plan_copy`] with an explicit ABI.
///
/// # Errors
///
/// See [`plan_copy`].
pub fn plan_copy_for(
    abi: Abi,
    layout: &StaticLayout,
    mem: usize,
    tcb_size: usize,
    mods: &[TlsModuleIn],
) -> Result<CopyPlan, LayoutError> {
    let offsets = module_offsets_for(abi, mods)?;
    let count = offsets.len;
    let words = count.checked_add(1).ok_or(LayoutError::Overflow)?;
    let mut blocks = [0usize; MAX_TLS_MODULES];
    let (tcb, dtv) = if abi.is_above_tp() {
        let tail = words
            .checked_mul(core::mem::size_of::<usize>())
            .ok_or(LayoutError::Overflow)?;
        let dtv = mem
            .checked_add(layout.total)
            .ok_or(LayoutError::Overflow)?
            .checked_sub(tail)
            .ok_or(LayoutError::Overflow)?;
        let mask = layout.align.wrapping_sub(1);
        let bias = mem.checked_add(tcb_size).ok_or(LayoutError::Overflow)?;
        let tcb = mem
            .checked_add(bias.wrapping_neg() & mask)
            .ok_or(LayoutError::Overflow)?;
        let block_mem = tcb.checked_add(tcb_size).ok_or(LayoutError::Overflow)?;
        for index in 0..count {
            let offset = offsets.get(index).ok_or(LayoutError::Overflow)?;
            let block = block_mem.checked_add(offset).ok_or(LayoutError::Overflow)?;
            if let Some(slot) = blocks.get_mut(index) {
                *slot = block;
            }
        }
        (tcb, dtv)
    } else {
        let dtv = mem;
        let mut end = mem
            .checked_add(layout.total)
            .ok_or(LayoutError::Overflow)?
            .checked_sub(tcb_size)
            .ok_or(LayoutError::Overflow)?;
        end = end
            .checked_sub(end & layout.align.wrapping_sub(1))
            .ok_or(LayoutError::Overflow)?;
        let tcb = end;
        for index in 0..count {
            let offset = offsets.get(index).ok_or(LayoutError::Overflow)?;
            let block = tcb.checked_sub(offset).ok_or(LayoutError::Overflow)?;
            if let Some(slot) = blocks.get_mut(index) {
                *slot = block;
            }
        }
        (tcb, dtv)
    };
    Ok(CopyPlan {
        tcb,
        dtv,
        blocks,
        len: count,
        words,
    })
}

/// Bytes of the initialised image that `__reset_tls` copies again after `fork`.
#[must_use]
pub fn reset_bytes(module: &TlsModuleIn) -> usize {
    module.filesz
}

/// The zeroed tail of one module (`.tbss`).
#[must_use]
pub fn zero_bytes(module: &TlsModuleIn) -> usize {
    module.memsz.saturating_sub(module.filesz)
}

/// Capacity of [`BUILTIN_TLS`].
pub use crate::consts::BUILTIN_TLS_SIZE;
/// The static `builtin_tls` buffer (small TLS needs no mapping).
pub use crate::raw::BUILTIN_TLS;

#[cfg(test)]
#[path = "init_ut.rs"]
mod tests;
