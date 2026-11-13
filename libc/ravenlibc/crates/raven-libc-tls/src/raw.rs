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

//! The audited raw module: initial-image writes, DTV publication and `tp` conversion.
//!
//! Everything unsafe in `raven-libc-tls` lives here. Each block names the invariant that makes
//! it sound: a caller-supplied mapping of at least `layout.total` writable bytes, a DTV word
//! array with a published count, or a thread pointer that belongs to the calling thread.

#![allow(clippy::arithmetic_side_effects)] // checked/wrapping arithmetic around addresses

use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicPtr, Ordering};

use crate::consts::{Abi, BUILTIN_TLS_SIZE, MAX_TLS_MODULES, TARGET_ABI};
use crate::dtv::Dtv;
use crate::init::CopyPlan;
use crate::layout::{LayoutError, StaticLayout, TlsModuleIn, block_start, module_offsets_for};
use crate::tcb::{Pthread, tp_to_pthread};

/// The static TLS buffer used when the whole layout fits [`BUILTIN_TLS_SIZE`].
#[repr(C, align(16))]
pub struct BuiltinTls {
    bytes: UnsafeCell<[u8; BUILTIN_TLS_SIZE]>,
}

// SAFETY: the buffer is only written during the single-threaded startup path or through the
// caller's exclusive TLS area; `as_ptr` hands out a raw pointer and never a shared reference.
unsafe impl Sync for BuiltinTls {}

impl BuiltinTls {
    /// A zeroed buffer.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bytes: UnsafeCell::new([0; BUILTIN_TLS_SIZE]),
        }
    }

    /// The start of the buffer.
    #[must_use]
    pub fn as_ptr(&self) -> *mut u8 {
        self.bytes.get() as *mut u8
    }

    /// The buffer capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        BUILTIN_TLS_SIZE
    }
}

impl Default for BuiltinTls {
    fn default() -> Self {
        Self::new()
    }
}

/// The process-wide static TLS buffer.
pub static BUILTIN_TLS: BuiltinTls = BuiltinTls::new();

/// Read DTV word `module` (1-based) through the flexible array that follows word 0.
///
/// The caller has already checked `module <= dtv.count()`; the words are part of the same
/// allocation as the `Dtv` view.
#[must_use]
pub fn dtv_word(dtv: &Dtv, module: usize) -> usize {
    debug_assert!(module > 0 && module <= dtv.count());
    // SAFETY: `dtv` points at word 0 of an array with at least `count + 1` words (the contract
    // of `publish_dtv` and of `copy_tls`); `module <= count` keeps the index inside it.
    let word = unsafe { &*(dtv.words().add(module)) };
    word.load(Ordering::Acquire)
}

/// Load a published DTV pointer and return the view, or `None` before the first publication.
#[must_use]
pub fn load_dtv(slot: &AtomicPtr<Dtv>) -> Option<&Dtv> {
    let pointer = slot.load(Ordering::Acquire);
    if pointer.is_null() {
        return None;
    }
    // SAFETY: `publish_dtv` is the only writer and its contract guarantees the array outlives
    // every reader; a null pointer was rejected above.
    Some(unsafe { &*pointer })
}

/// Publish a grown DTV with Release ordering (A-tls-1).
///
/// # Safety
///
/// `new` must point at word 0 of a live array with `new.count()` following words, and the array
/// must stay alive for as long as any thread can load it from `slot`.
pub unsafe fn publish_dtv(slot: &AtomicPtr<Dtv>, new: *mut Dtv) {
    slot.store(new, Ordering::Release);
    crate::dtv::bump_generation();
}

/// Execute a [`CopyPlan`] in a freshly mapped TLS area.
///
/// # Safety
///
/// `mem` must be the start of a writable mapping of at least `layout.total` bytes that is not
/// concurrently accessed, and every module image must be readable for `filesz` bytes.
pub unsafe fn copy_tls(
    mem: usize,
    layout: &StaticLayout,
    mods: &[TlsModuleIn],
    tcb_size: usize,
) -> Result<CopyPlan, LayoutError> {
    let plan = crate::init::plan_copy(layout, mem, tcb_size, mods)?;
    // SAFETY: the caller guarantees `mem..mem+total` is writable; zeroing the whole area makes a
    // reused block observe zeroed `.tbss` (A-tls-4).
    unsafe {
        core::ptr::write_bytes(mem as *mut u8, 0, layout.total);
    }
    // SAFETY: word 0 and the `words - 1` following words are inside the mapping: variant I puts
    // them at `mem + total - words*8`, variant II at `mem`, and `total` includes the headroom.
    unsafe {
        let words = plan.dtv as *mut usize;
        words.write(plan.len);
        for index in 0..plan.len {
            let block = plan.blocks.get(index).copied().unwrap_or(0);
            let entry = block.wrapping_add(TARGET_ABI.dtp_offset());
            words.add(index.wrapping_add(1)).write(entry);
        }
        let tcb = plan.tcb as *mut Pthread;
        (*tcb).dtv.store(plan.dtv as *mut Dtv, Ordering::Release);
    }
    copy_images(&plan, mods);
    Ok(plan)
}

/// Copy the initialised bytes and zero the `.tbss` tail of every module block.
fn copy_images(plan: &CopyPlan, mods: &[TlsModuleIn]) {
    for (index, module) in mods.iter().enumerate() {
        let Some(block) = plan.blocks.get(index).copied() else {
            break;
        };
        if module.filesz > 0 {
            // SAFETY: `copy_tls` verified the mapping; the image is readable for `filesz` bytes
            // by the ELF program header contract.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    module.image_addr as *const u8,
                    block as *mut u8,
                    module.filesz,
                );
            }
        }
        let tail = module.memsz.saturating_sub(module.filesz);
        if tail > 0 {
            // SAFETY: the block has `memsz` bytes inside the mapping and `filesz <= memsz`.
            unsafe {
                core::ptr::write_bytes(block.wrapping_add(module.filesz) as *mut u8, 0, tail);
            }
        }
    }
}

/// Re-initialise the TLS of the surviving thread after `fork` (`__reset_tls`).
///
/// # Safety
///
/// `tp` must be the calling thread's thread pointer and every module image must be readable for
/// `filesz` bytes.
pub unsafe fn reset_tls(tp: usize, mods: &[TlsModuleIn]) -> Result<(), LayoutError> {
    let offsets = module_offsets_for(TARGET_ABI, mods)?;
    for (index, module) in mods.iter().enumerate() {
        let Some(offset) = offsets.get(index) else {
            break;
        };
        let block = block_start(TARGET_ABI, tp, offset);
        if module.filesz > 0 {
            // SAFETY: the thread's TLS block is mapped and private to this thread; the image is
            // readable for `filesz` bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(
                    module.image_addr as *const u8,
                    block as *mut u8,
                    module.filesz,
                );
            }
        }
        let tail = module.memsz.saturating_sub(module.filesz);
        if tail > 0 {
            // SAFETY: the block has `memsz` bytes and `filesz <= memsz`.
            unsafe {
                core::ptr::write_bytes(block.wrapping_add(module.filesz) as *mut u8, 0, tail);
            }
        }
    }
    Ok(())
}

/// Run `f` with the `Pthread` of `tp`.
///
/// # Safety
///
/// `tp` must be the calling thread's thread pointer and the TCB must be live for the call.
pub unsafe fn with_pthread<R>(tp: usize, f: impl FnOnce(&Pthread) -> R) -> R {
    // SAFETY: the caller guarantees `tp` maps to a live TCB.
    let pthread = unsafe { &*(tp_to_pthread(tp) as *const Pthread) };
    f(pthread)
}

/// Run `f` with a mutable `Pthread` of `tp`.
///
/// # Safety
///
/// `tp` must be the calling thread's thread pointer, the TCB must be live and no other reference
/// to it may be used for the duration of the call.
pub unsafe fn with_pthread_mut<R>(tp: usize, f: impl FnOnce(&mut Pthread) -> R) -> R {
    // SAFETY: the caller guarantees `tp` maps to a live, exclusively accessed TCB.
    let pthread = unsafe { &mut *(tp_to_pthread(tp) as *mut Pthread) };
    f(pthread)
}

/// The DTV word count for a module list.
#[must_use]
pub fn dtv_words(mods: &[TlsModuleIn]) -> usize {
    mods.len().min(MAX_TLS_MODULES).wrapping_add(1)
}

/// The ABI used by this build; exposed for tests that build both variants on one host.
#[must_use]
pub const fn abi() -> Abi {
    TARGET_ABI
}

#[cfg(test)]
#[path = "raw_ut.rs"]
mod tests;
