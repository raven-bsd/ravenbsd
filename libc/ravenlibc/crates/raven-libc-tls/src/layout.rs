// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/env/__init_tls.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
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

//! The `PT_TLS` size/offset arithmetic (musl `__init_tls.c:107-131`).
//!
//! The congruence term uses the runtime image address, not the offset: a module block must
//! satisfy `block_start % align == image_addr % align`. Variant II puts the block below `tp`
//! and uses `offset = size` for a single module; variant I puts it above `tp` and uses
//! `offset = GAP_ABOVE_TP + ((-GAP_ABOVE_TP + image) & (align-1))`.
//!
//! All arithmetic is wrapping or checked; a module list near `usize::MAX` returns
//! [`LayoutError::Overflow`] and never wraps silently.

use crate::consts::{Abi, DTV_HEADROOM, MAX_TLS_ALIGN, MAX_TLS_MODULES, MIN_TLS_ALIGN, TARGET_ABI};

/// One `PT_TLS` program header, resolved to runtime addresses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TlsModuleIn {
    /// Runtime address of the first TLS byte (the module's `p_vaddr` plus the load bias).
    pub image_addr: usize,
    /// Initialised bytes (`p_filesz`).
    pub filesz: usize,
    /// Total bytes including `.tbss` (`p_memsz`).
    pub memsz: usize,
    /// `p_align`; zero means one.
    pub align: usize,
}

impl TlsModuleIn {
    /// Build a module description.
    #[must_use]
    pub const fn new(image_addr: usize, filesz: usize, memsz: usize, align: usize) -> Self {
        Self {
            image_addr,
            filesz,
            memsz,
            align,
        }
    }

    fn normalized_align(&self) -> Result<usize, LayoutError> {
        let align = if self.align == 0 { 1 } else { self.align };
        if !align.is_power_of_two() {
            return Err(LayoutError::BadAlign);
        }
        // `p_align` above 4096 is capped, exactly as the loader does for a hostile header.
        Ok(align.min(MAX_TLS_ALIGN))
    }

    fn aligned_size(&self) -> Result<usize, LayoutError> {
        if self.filesz > self.memsz {
            return Err(LayoutError::FileszAboveMemsz);
        }
        let align = self.normalized_align()?;
        let mask = align.wrapping_sub(1);
        let pad = self.memsz.wrapping_neg().wrapping_sub(self.image_addr) & mask;
        let size = self.memsz.checked_add(pad).ok_or(LayoutError::Overflow)?;
        if size > isize::MAX as usize {
            return Err(LayoutError::TooLarge);
        }
        // The block must end in the address space; a wrapped image+size is not a layout.
        self.image_addr
            .checked_add(size)
            .ok_or(LayoutError::Overflow)?;
        Ok(size)
    }
}

/// Why a module list has no valid layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// `p_align` is neither zero nor a power of two.
    BadAlign,
    /// `p_filesz > p_memsz`.
    FileszAboveMemsz,
    /// The size computation or the total wrapped.
    Overflow,
    /// The result does not fit the address space, or the list is too long.
    TooLarge,
}

impl LayoutError {
    /// The ravenlibc error this failure maps to: bad headers are `ENOEXEC`, size failures are
    /// `ENOMEM` (documented in the loader).
    #[must_use]
    pub const fn errno(self) -> u16 {
        match self {
            Self::BadAlign | Self::FileszAboveMemsz => 8, // ENOEXEC
            Self::Overflow | Self::TooLarge => 12,        // ENOMEM
        }
    }
}

/// The frozen static layout of one `PT_TLS` set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StaticLayout {
    /// The used TLS block size (variant II: the module span; variant I: the span above `offset`).
    pub size: usize,
    /// The TPOFF base of the first module (variant II `size`; variant I the gap congruence).
    pub offset: usize,
    /// Effective alignment (at least [`MIN_TLS_ALIGN`], at most 4096).
    pub align: usize,
    /// Bytes to map for the TCB, DTV, TLS block and alignment slop.
    pub total: usize,
    /// Number of modules (zero when there is no `PT_TLS`).
    pub tls_cnt: usize,
}

/// Per-module TPOFF bases for the loader's `dtv`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModuleOffsets {
    /// `offsets[i]` is the TPOFF base of module `i`.
    pub offsets: [usize; MAX_TLS_MODULES],
    /// Number of valid entries in [`Self::offsets`].
    pub len: usize,
    /// End of the module span measured from the TLS base.
    pub span: usize,
}

impl ModuleOffsets {
    /// The offset of module `i`, if it exists.
    #[must_use]
    pub fn get(&self, index: usize) -> Option<usize> {
        if index < self.len {
            self.offsets.get(index).copied()
        } else {
            None
        }
    }
}

/// The `tls_size` formula: headroom, TCB, variant-I offset, module span, alignment slop.
fn total_size(
    abi: Abi,
    size: usize,
    offset: usize,
    align: usize,
    tcb_size: usize,
) -> Result<usize, LayoutError> {
    let mut sum = DTV_HEADROOM
        .checked_add(tcb_size)
        .ok_or(LayoutError::Overflow)?;
    if abi.is_above_tp() {
        sum = sum.checked_add(offset).ok_or(LayoutError::Overflow)?;
    }
    sum = sum.checked_add(size).ok_or(LayoutError::Overflow)?;
    sum = sum.checked_add(align).ok_or(LayoutError::Overflow)?;
    sum = sum
        .checked_add(MIN_TLS_ALIGN.wrapping_sub(1))
        .ok_or(LayoutError::Overflow)?;
    let total = sum & !MIN_TLS_ALIGN.wrapping_sub(1);
    if total > isize::MAX as usize {
        return Err(LayoutError::TooLarge);
    }
    Ok(total)
}

/// The single-module (or any-number-of-modules) static layout.
///
/// `tcb_size` is `size_of::<Pthread>()`. With no module the count is zero and `size` is zero;
/// variant I still reports `offset = GAP_ABOVE_TP`, exactly as the donor's `main_tls` does.
///
/// # Errors
///
/// [`LayoutError`] for a bad alignment, `filesz > memsz`, a wrapped size or an oversized result.
pub fn compute_static_layout(
    mods: &[TlsModuleIn],
    tcb_size: usize,
) -> Result<StaticLayout, LayoutError> {
    compute_static_layout_for(TARGET_ABI, mods, tcb_size)
}

/// [`compute_static_layout`] with an explicit ABI, for cross-target tests and the loader.
///
/// # Errors
///
/// See [`compute_static_layout`].
pub fn compute_static_layout_for(
    abi: Abi,
    mods: &[TlsModuleIn],
    tcb_size: usize,
) -> Result<StaticLayout, LayoutError> {
    if mods.len() > MAX_TLS_MODULES {
        return Err(LayoutError::TooLarge);
    }
    let mut align = MIN_TLS_ALIGN;
    let mut first_offset = if abi.is_above_tp() {
        abi.gap_above_tp()
    } else {
        0
    };
    let mut cursor = first_offset;
    let mut first = true;
    for module in mods {
        let module_align = module.normalized_align()?;
        align = align.max(module_align);
        let size = module.aligned_size()?;
        let offset = match abi {
            Abi::VariantII { .. } => {
                // The donor's variant-II accumulation: end of the block, aligned back so the
                // resulting block start is congruent with the image.
                let mut end = cursor
                    .checked_add(size)
                    .ok_or(LayoutError::Overflow)?
                    .checked_add(module_align)
                    .ok_or(LayoutError::Overflow)?
                    .checked_sub(1)
                    .ok_or(LayoutError::Overflow)?;
                let mask = module_align.wrapping_sub(1);
                let bias = end
                    .checked_add(module.image_addr)
                    .ok_or(LayoutError::Overflow)?;
                end = end.checked_sub(bias & mask).ok_or(LayoutError::Overflow)?;
                cursor = end;
                end
            }
            Abi::VariantI { gap_above_tp, .. } => {
                let delta = module_align.wrapping_sub(1) & module.image_addr.wrapping_sub(cursor);
                let offset = cursor.checked_add(delta).ok_or(LayoutError::Overflow)?;
                cursor = offset.checked_add(size).ok_or(LayoutError::Overflow)?;
                if first {
                    first_offset = offset;
                } else {
                    let _ = gap_above_tp;
                }
                offset
            }
        };
        if first {
            if !abi.is_above_tp() {
                first_offset = offset;
            }
            first = false;
        }
        let _ = offset;
    }
    let size = if mods.is_empty() {
        0
    } else if abi.is_above_tp() {
        cursor
            .checked_sub(first_offset)
            .ok_or(LayoutError::Overflow)?
    } else {
        cursor
    };
    let offset = first_offset;
    let total = total_size(abi, size, offset, align, tcb_size)?;
    Ok(StaticLayout {
        size,
        offset,
        align,
        total,
        tls_cnt: mods.len(),
    })
}

/// Per-module TPOFF bases, cumulative in load order (the loader, C62).
///
/// For variant II the bases are counted down from the TCB; for variant I they are counted up
/// from `GAP_ABOVE_TP`. `dtv[i + 1] = block_start_i + DTP_OFFSET`.
///
/// # Errors
///
/// [`LayoutError`] for a bad module or more than [`MAX_TLS_MODULES`] modules.
pub fn module_offsets(mods: &[TlsModuleIn]) -> Result<ModuleOffsets, LayoutError> {
    module_offsets_for(TARGET_ABI, mods)
}

/// [`module_offsets`] with an explicit ABI.
///
/// # Errors
///
/// See [`module_offsets`].
pub fn module_offsets_for(abi: Abi, mods: &[TlsModuleIn]) -> Result<ModuleOffsets, LayoutError> {
    if mods.len() > MAX_TLS_MODULES {
        return Err(LayoutError::TooLarge);
    }
    let mut offsets = [0usize; MAX_TLS_MODULES];
    let mut cursor = if abi.is_above_tp() {
        abi.gap_above_tp()
    } else {
        0
    };
    for (index, module) in mods.iter().enumerate() {
        let module_align = module.normalized_align()?;
        let size = module.aligned_size()?;
        let offset = match abi {
            Abi::VariantII { .. } => {
                let mut end = cursor
                    .checked_add(size)
                    .ok_or(LayoutError::Overflow)?
                    .checked_add(module_align)
                    .ok_or(LayoutError::Overflow)?
                    .checked_sub(1)
                    .ok_or(LayoutError::Overflow)?;
                let bias = end
                    .checked_add(module.image_addr)
                    .ok_or(LayoutError::Overflow)?;
                end = end
                    .checked_sub(bias & module_align.wrapping_sub(1))
                    .ok_or(LayoutError::Overflow)?;
                cursor = end;
                end
            }
            Abi::VariantI { .. } => {
                let delta = module_align.wrapping_sub(1) & module.image_addr.wrapping_sub(cursor);
                let offset = cursor.checked_add(delta).ok_or(LayoutError::Overflow)?;
                cursor = offset.checked_add(size).ok_or(LayoutError::Overflow)?;
                offset
            }
        };
        if let Some(slot) = offsets.get_mut(index) {
            *slot = offset;
        }
    }
    Ok(ModuleOffsets {
        offsets,
        len: mods.len(),
        span: cursor,
    })
}

/// The address of the block of `module` for a thread whose thread pointer is `tp`.
///
/// `offset` is the module's TPOFF base from [`module_offsets`]. Variant II subtracts it from
/// `tp`; variant I adds it.
#[must_use]
pub fn block_start(abi: Abi, tp: usize, offset: usize) -> usize {
    if abi.is_above_tp() {
        tp.wrapping_add(offset)
    } else {
        tp.wrapping_sub(offset)
    }
}

/// The `dtv` entry for a module: the block address biased by `DTP_OFFSET`.
#[must_use]
pub fn dtv_entry(abi: Abi, tp: usize, offset: usize) -> usize {
    block_start(abi, tp, offset).wrapping_add(abi.dtp_offset())
}

#[cfg(test)]
#[path = "layout_ut.rs"]
mod tests;
