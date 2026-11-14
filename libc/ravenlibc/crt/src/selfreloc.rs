// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:ldso/dlstart.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (donor file has no per-file header; covered by musl/COPYRIGHT)
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

//! The self-relocator of the loader and of static-PIE programs (`donor-map/13` 3.3).
//!
//! This module runs **before any relocation of its own image**. It therefore has no `static`,
//! no `&'static str`, no function pointer of its own, no `dyn` and no allocation: every value
//! lives in registers or on the stack, and every call is to an `#[inline(always)]` function of
//! this file (`A-ld-1`). Slices are never indexed (an index would emit a `panic_bounds_check`
//! call through an unrelocated GOT slot): raw pointers and `wrapping_*` arithmetic only.
//!
//! The loader (`ld/src/lib.rs`, C62) includes this file with
//! `#[path = "../../crt/src/selfreloc.rs"] mod selfreloc;`. It is the one copy.
//!
//! The algorithm is musl `_dlstart_c` with the FDPIC and MIPS branches deleted and one addition
//! of `donor-map/13` 3.3: `IRELATIVE`, applied **after** `RELATIVE` and `RELR` (trap T-25).
//! `DT_REL` is ignored by design: all three targets use `DT_RELA`, and the loader rejects a
//! `DT_REL` object before it reaches a relocation pass.
//!
//! What a caller must guarantee: `sp` points at the C8 stack (`CONTRACTS.md` C8) and `dynv`
//! points at the terminated `_DYNAMIC` array of this very image. Both are the loader's and the
//! static-PIE entry's documented entry values, never user input.

/// `AUX_CNT` of `musl:src/internal/dynlink.h`: the entries the self-relocator keeps.
pub const AUX_CNT: usize = 32;

/// `DYN_CNT` of `musl:src/internal/dynlink.h`; `DT_RELR = 36` is below it.
pub const DYN_CNT: usize = 37;

/// The `DT_*` values gathered from `_DYNAMIC`; addresses are already biased.
#[derive(Clone, Copy)]
pub struct DynInfo {
    /// `DT_RELA`: address of the `Elf64_Rela` array, or 0.
    pub rela: usize,
    /// `DT_RELASZ`: byte size of the `Elf64_Rela` array.
    pub relasz: usize,
    /// `DT_RELR`: address of the RELR entries, or 0.
    pub relr: usize,
    /// `DT_RELRSZ`: byte size of the RELR entries.
    pub relrsz: usize,
    /// `DT_JMPREL`: address of the PLT `Elf64_Rela` array, or 0.
    pub jmprel: usize,
    /// `DT_PLTRELSZ`: byte size of the PLT `Elf64_Rela` array.
    pub pltrelsz: usize,
    /// `DT_PLTGOT`: the GOT address, kept for the loader's diagnostics.
    pub pltgot: usize,
}

const AT_PHDR: usize = 3;
const AT_PHENT: usize = 4;
const AT_PHNUM: usize = 5;
const AT_BASE: usize = 7;
#[allow(dead_code)] // read on AArch64 and RISC-V; the x86-64 tests check the stored value
const AT_HWCAP: usize = 16;
#[allow(dead_code)] // read on AArch64 and RISC-V; the x86-64 tests check the stored value
const AT_HWCAP2: usize = 26;

const PT_DYNAMIC: u32 = 2;

const DT_PLTRELSZ: usize = 2;
const DT_PLTGOT: usize = 3;
const DT_RELA: usize = 7;
const DT_RELASZ: usize = 8;
const DT_JMPREL: usize = 23;
const DT_RELRSZ: usize = 35;
const DT_RELR: usize = 36;

/// Size of one `Elf64_Rela`: `r_offset`, `r_info`, `r_addend`.
const RELA_SIZE: usize = 24;

/// `ifunc_arg_t` of the AArch64 and RISC-V psABI: the five words a resolver receives.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
#[repr(C)]
struct IfuncArg {
    size: u64,
    hwcap: u64,
    hwcap2: u64,
    hwcap3: u64,
    hwcap4: u64,
}

/// `_IFUNC_ARG_HWCAP`: the aarch64 resolver's "the second argument is an `ifunc_arg_t`" bit.
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
const IFUNC_ARG_HWCAP: u64 = 1 << 62;

/// Add the bias to a `DT_*` virtual address; 0 means "absent" and stays 0.
#[inline(always)]
fn biased(base: usize, value: usize) -> usize {
    if value == 0 {
        0
    } else {
        base.wrapping_add(value)
    }
}

/// Gather the `AT_*` entries of the C8 stack and compute the load bias.
///
/// The bias is `AT_BASE`; when that is 0 (the loader for its own image, or a static-PIE
/// program) it is `dynv - PT_DYNAMIC.p_vaddr`, found by scanning the program header table at
/// `AT_PHDR`/`AT_PHNUM`/`AT_PHENT` (`A-ld-2`). Keys `>= AUX_CNT` are ignored; Raven's
/// `0x7001..0x7007` keys therefore never enter this table.
///
/// # Safety
///
/// `sp` must point at the C8 stack and `dynv` at this image's terminated `_DYNAMIC` array.
#[inline(always)]
pub unsafe fn load_base(
    sp: *const usize,
    dynv: *const usize,
    aux_out: &mut [usize; AUX_CNT],
) -> usize {
    let aux = aux_out.as_mut_ptr();
    let mut index = 0usize;
    while index < AUX_CNT {
        // SAFETY: `index < AUX_CNT`, so the write is inside the caller's array.
        unsafe {
            *aux.add(index) = 0;
        }
        index = index.wrapping_add(1);
    }

    // SAFETY: `sp` is the C8 stack: `argc` followed by the argv array.
    let argc = unsafe { *sp };
    let mut word = sp.wrapping_add(1);
    let mut index = 0usize;
    while index < argc {
        word = word.wrapping_add(1);
        index = index.wrapping_add(1);
    }
    // `word` now points at `argv[argc]`, the NULL terminator; the environment array follows.
    word = word.wrapping_add(1);
    loop {
        // SAFETY: the C8 environment array is NUL-terminated.
        let entry = unsafe { *word };
        if entry == 0 {
            break;
        }
        word = word.wrapping_add(1);
    }
    // SAFETY: the auxv pairs follow the environment terminator.
    let auxv = word.wrapping_add(1);
    let mut index = 0usize;
    loop {
        // SAFETY: the auxv array is `(key, value)` pairs terminated by `(0, 0)`.
        let key = unsafe { *auxv.add(index) };
        if key == 0 {
            break;
        }
        // SAFETY: every key has the value word immediately after it.
        let value = unsafe { *auxv.add(index.wrapping_add(1)) };
        if key < AUX_CNT {
            // SAFETY: `key < AUX_CNT` was just checked.
            unsafe {
                *aux.add(key) = value;
            }
        }
        index = index.wrapping_add(2);
    }

    // SAFETY: every slot below AUX_CNT was initialised above.
    let mut base = unsafe { *aux.add(AT_BASE) };
    if base == 0 {
        // SAFETY: the auxiliary entries at these fixed keys were read in the loop.
        let phnum = unsafe { *aux.add(AT_PHNUM) };
        // SAFETY: see above.
        let phentsize = unsafe { *aux.add(AT_PHENT) };
        // SAFETY: see above.
        let mut phdr = unsafe { *aux.add(AT_PHDR) } as *const u8;
        let mut index = 0usize;
        while index < phnum {
            // SAFETY: `phdr` points into the mapped program header table and `p_type` is its
            // first `u32`; the table has `phnum` entries of `phentsize` bytes.
            let p_type = unsafe { *(phdr as *const u32) };
            if p_type == PT_DYNAMIC {
                // SAFETY: `p_vaddr` is the `u64` at offset 16 of `Elf64_Phdr`, and `phdr`
                // points at a live program header.
                let p_vaddr = unsafe { *((phdr.wrapping_add(16)) as *const u64) } as usize;
                base = (dynv as usize).wrapping_sub(p_vaddr);
                break;
            }
            phdr = phdr.wrapping_add(phentsize);
            index = index.wrapping_add(1);
        }
    }
    base
}

/// Walk `_DYNAMIC` once and collect the relocation tables (`musl:ldso/dlstart.c:66-68`).
///
/// Every address is biased with `base` here, so the `apply_*` functions work on absolute
/// pointers. `DT_REL`/`DT_RELSZ` are deliberately not gathered: all three targets use
/// `DT_RELA`, and a `DT_REL` object is a build failure for the loader (`donor-map/13` 3.3).
///
/// # Safety
///
/// `dynv` must point at a terminated `_DYNAMIC` array of this image.
#[inline(always)]
pub unsafe fn gather_dyn(base: usize, dynv: *const usize) -> DynInfo {
    let mut info = DynInfo {
        rela: 0,
        relasz: 0,
        relr: 0,
        relrsz: 0,
        jmprel: 0,
        pltrelsz: 0,
        pltgot: 0,
    };
    let mut index = 0usize;
    loop {
        // SAFETY: `_DYNAMIC` is `(tag, value)` pairs terminated by `DT_NULL`.
        let tag = unsafe { *dynv.add(index) };
        if tag == 0 {
            break;
        }
        // SAFETY: the value word follows the tag.
        let value = unsafe { *dynv.add(index.wrapping_add(1)) };
        if tag < DYN_CNT {
            match tag {
                DT_RELA => info.rela = biased(base, value),
                DT_RELASZ => info.relasz = value,
                DT_RELR => info.relr = biased(base, value),
                DT_RELRSZ => info.relrsz = value,
                DT_JMPREL => info.jmprel = biased(base, value),
                DT_PLTRELSZ => info.pltrelsz = value,
                DT_PLTGOT => info.pltgot = biased(base, value),
                _ => {}
            }
        }
        index = index.wrapping_add(2);
    }
    info
}

/// Apply `R_*_RELATIVE` of `DT_RELA`: `*where = base + addend`.
///
/// A relocation of any other type is skipped; the loader's build check rejects such an image
/// before it can run (`donor-map/13` 3.3).
///
/// # Safety
///
/// `d.rela`/`d.relasz` must describe an `Elf64_Rela` array of this image and every `r_offset`
/// must name a writable word inside the mapped image.
#[inline(always)]
pub unsafe fn apply_relative(base: usize, d: &DynInfo) {
    let mut offset = 0usize;
    while offset < d.relasz {
        // SAFETY: the loop invariant keeps `offset < relasz`; the array is `offset` bytes in.
        let entry = d.rela.wrapping_add(offset) as *const usize;
        // SAFETY: an `Elf64_Rela` is three words and `offset` is 24-aligned by construction.
        let r_offset = unsafe { *entry };
        // SAFETY: `r_info` is the second word of the entry.
        let r_info = unsafe { *entry.add(1) };
        // SAFETY: `r_addend` is the third word of the entry.
        let r_addend = unsafe { *entry.add(2) };
        if crate::selfreloc_consts::r_type(r_info) == crate::selfreloc_consts::REL_RELATIVE {
            let where_ = base.wrapping_add(r_offset) as *mut usize;
            // SAFETY: the caller guarantees `r_offset` names a writable image word.
            unsafe {
                *where_ = base.wrapping_add(r_addend);
            }
        }
        offset = offset.wrapping_add(RELA_SIZE);
    }
}

/// Apply `DT_RELR` (`donor-map/13` 3.6.4): `*word += base`.
///
/// An even entry is the address of the first word to adjust; an odd entry is a 63-bit bitmap
/// whose bit `n` (1..=63) adjusts the word `addr + (n - 1) * 8`. After a bitmap the next
/// address is 63 words further (`musl:ldso/dlstart.c:144-157`).
///
/// # Safety
///
/// `d.relr`/`d.relrsz` must describe this image's RELR entries and every named address must be
/// a writable word inside the mapped image.
#[inline(always)]
pub unsafe fn apply_relr(base: usize, d: &DynInfo) {
    let mut address = 0usize;
    let mut offset = 0usize;
    while offset < d.relrsz {
        // SAFETY: `offset < relrsz`; each entry is one pointer-sized word.
        let entry = unsafe { *((d.relr.wrapping_add(offset)) as *const usize) };
        if entry & 1 == 0 {
            address = base.wrapping_add(entry);
            // SAFETY: the even entry names a writable image word.
            let slot = address as *mut usize;
            // SAFETY: see above; the word holds its link-time value and only gains the bias.
            unsafe {
                *slot = (*slot).wrapping_add(base);
            }
            address = address.wrapping_add(8);
        } else {
            let mut bit = 1usize;
            while bit < 64 {
                if entry & (1usize << bit) != 0 {
                    let slot =
                        address.wrapping_add(bit.wrapping_sub(1).wrapping_mul(8)) as *mut usize;
                    // SAFETY: the bitmap bit names a writable image word.
                    unsafe {
                        *slot = (*slot).wrapping_add(base);
                    }
                }
                bit = bit.wrapping_add(1);
            }
            address = address.wrapping_add(63usize.wrapping_mul(8));
        }
        offset = offset.wrapping_add(8);
    }
}

/// Apply `R_*_IRELATIVE` of `DT_RELA` and `DT_JMPREL`, after `RELATIVE` and `RELR` (T-25).
///
/// The resolver is `base + addend`. x86-64 calls it with no arguments; AArch64 and RISC-V pass
/// `AT_HWCAP | _IFUNC_ARG_HWCAP` and a pointer to a stack `ifunc_arg_t` (`donor-map/13` 3.8).
/// ravenlibc itself ships no IFUNC (H-17); this path exists for foreign code and compiler-rt.
///
/// # Safety
///
/// The relocation tables must belong to this image, every `r_offset` must name a writable word,
/// and every resolver must be a valid function of this image.
#[inline(always)]
pub unsafe fn apply_irelative(base: usize, d: &DynInfo, aux: &[usize; AUX_CNT]) {
    // SAFETY: the `DT_RELA` table is this image's and the caller vouches for its offsets.
    unsafe {
        apply_irelative_table(base, d.rela, d.relasz, aux);
    }
    // SAFETY: the `DT_JMPREL` table is this image's and the caller vouches for its offsets.
    unsafe {
        apply_irelative_table(base, d.jmprel, d.pltrelsz, aux);
    }
}

/// One `Elf64_Rela` table of the `IRELATIVE` pass.
///
/// # Safety
///
/// See [`apply_irelative`].
#[inline(always)]
unsafe fn apply_irelative_table(base: usize, table: usize, size: usize, aux: &[usize; AUX_CNT]) {
    #[cfg(target_arch = "x86_64")]
    let _ = aux;
    let mut offset = 0usize;
    while offset < size {
        // SAFETY: the table has at least `offset` bytes and entries are `RELA_SIZE` apart.
        let entry = table.wrapping_add(offset) as *const usize;
        // SAFETY: `r_offset` is the first word of the entry.
        let r_offset = unsafe { *entry };
        // SAFETY: `r_info` is the second word of the entry.
        let r_info = unsafe { *entry.add(1) };
        // SAFETY: `r_addend` is the third word of the entry.
        let r_addend = unsafe { *entry.add(2) };
        if crate::selfreloc_consts::r_type(r_info) == crate::selfreloc_consts::REL_IRELATIVE {
            let where_ = base.wrapping_add(r_offset) as *mut usize;
            let address = base.wrapping_add(r_addend);
            #[cfg(target_arch = "x86_64")]
            {
                // SAFETY: the caller guarantees `address` is a resolver in this image; the
                // x86-64 psABI calls an IRELATIVE resolver with no arguments.
                let resolver: extern "C" fn() -> usize = unsafe { core::mem::transmute(address) };
                let value = resolver();
                // SAFETY: `r_offset` names a writable image word.
                unsafe {
                    *where_ = value;
                }
            }
            #[cfg(any(target_arch = "aarch64", target_arch = "riscv64"))]
            {
                // SAFETY: the auxiliary table has AUX_CNT entries; these keys are below it.
                let hwcap = unsafe { *aux.as_ptr().add(AT_HWCAP) } as u64;
                // SAFETY: see above.
                let hwcap2 = unsafe { *aux.as_ptr().add(AT_HWCAP2) } as u64;
                let argument = IfuncArg {
                    size: core::mem::size_of::<IfuncArg>() as u64,
                    hwcap,
                    hwcap2,
                    hwcap3: 0,
                    hwcap4: 0,
                };
                // SAFETY: the caller guarantees `address` is a resolver of this image and the
                // psABI passes `(hwcap | _IFUNC_ARG_HWCAP, &ifunc_arg_t)`.
                let resolver: extern "C" fn(u64, *const IfuncArg) -> usize =
                    unsafe { core::mem::transmute(address) };
                let value = resolver(hwcap | IFUNC_ARG_HWCAP, &argument);
                // SAFETY: `r_offset` names a writable image word.
                unsafe {
                    *where_ = value;
                }
            }
        }
        offset = offset.wrapping_add(RELA_SIZE);
    }
}

/// The four passes in order: bias, `RELATIVE`, `RELR`, `IRELATIVE`; returns the bias.
///
/// This is the whole self-relocator of a static-PIE image and of the loader (`A-ld-2`).
///
/// # Safety
///
/// `sp` must be the C8 stack pointer and `dynv` must point at this image's terminated
/// `_DYNAMIC` array; every relocation offset must name a writable word of the mapped image.
#[inline(always)]
pub unsafe fn relocate_self(sp: *const usize, dynv: *const usize) -> usize {
    let mut aux = [0usize; AUX_CNT];
    // SAFETY: the caller guarantees the stack and `_DYNAMIC` contracts.
    let base = unsafe { load_base(sp, dynv, &mut aux) };
    // SAFETY: `_DYNAMIC` is this image's terminated array.
    let info = unsafe { gather_dyn(base, dynv) };
    // SAFETY: the tables of `info` belong to this image.
    unsafe {
        apply_relative(base, &info);
    }
    // SAFETY: RELR is applied after `RELATIVE` and before `IRELATIVE` (T-25).
    unsafe {
        apply_relr(base, &info);
    }
    // SAFETY: IRELATIVE is last, so a resolver observes every other relocation of the image.
    unsafe {
        apply_irelative(base, &info, &aux);
    }
    base
}

#[cfg(test)]
#[path = "selfreloc_ut.rs"]
mod tests;
