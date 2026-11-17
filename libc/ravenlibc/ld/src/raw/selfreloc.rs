// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Self-relocation of `ld-elf.so.1` before the loader uses any relocated data.
//!
//! The loader is a shared object (`-shared -z pack-relative-relocs`): its `.data.rel.ro` holds
//! relative pointers to its own functions and strings. `_rtld_start` reaches `_rtld_main`
//! before any generic relocation pass can run, so this small decoder applies exactly the
//! relative forms the linker emits for the loader itself.

use raven_libc_elf::{Dyn, Ehdr, Phdr};

/// The linker places the first `PT_LOAD` at this virtual address (`--image-base=0x1000`), so
/// the ELF header is at `AT_BASE + 0x1000`.
const SELF_HEADER_OFFSET: usize = 0x1000;
/// `e_phoff` of the linked loader (the standard 64-byte ELF header).
const SELF_PHDR_OFFSET: usize = SELF_HEADER_OFFSET + 64;
/// Bound on the dynamic array.
const MAX_DYNAMIC: usize = 256;
/// Bound on the program-header table.
const MAX_PHDRS: usize = 128;
/// `DT_NULL`.
const DT_NULL: i64 = 0;
/// `DT_RELA`.
const DT_RELA: i64 = 7;
/// `DT_RELASZ`.
const DT_RELASZ: i64 = 8;
/// `DT_RELAENT`.
const DT_RELAENT: i64 = 9;
/// `DT_RELR` (the packed relative table).
const DT_RELR: i64 = 0x24;
/// `DT_RELRSZ`.
const DT_RELRSZ: i64 = 0x23;
/// The relative relocation type number on the current architecture.
#[cfg(target_arch = "x86_64")]
const RELATIVE: u64 = 8;
#[cfg(target_arch = "aarch64")]
const RELATIVE: u64 = 1027;
#[cfg(target_arch = "riscv64")]
const RELATIVE: u64 = 3;

/// # Safety
/// `address` is readable for eight bytes in this process.
unsafe fn read_u64(address: usize) -> u64 {
    // SAFETY: the caller names a readable word of the loader image.
    unsafe { (address as *const u64).read_unaligned() }
}

/// # Safety
/// `address` is writable for eight bytes in this process.
unsafe fn write_u64(address: usize, value: u64) {
    // SAFETY: the caller names a writable word of the loader image.
    unsafe { (address as *mut u64).write_unaligned(value) }
}

fn parse_ehdr(base: usize) -> Option<Ehdr> {
    let header = base.checked_add(SELF_HEADER_OFFSET)?;
    // SAFETY: the loader's first load segment is mapped for the ELF header.
    let bytes = unsafe { core::slice::from_raw_parts(header as *const u8, 64) };
    Ehdr::parse(bytes).ok()
}

fn phdr(base: usize, index: usize) -> Option<Phdr> {
    let offset = index.checked_mul(56)?;
    let address = base.checked_add(SELF_PHDR_OFFSET)?.checked_add(offset)?;
    // SAFETY: the program-header table follows the ELF header in the same load segment.
    let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, 56) };
    Phdr::parse(bytes).ok()
}

fn dynamic(base: usize, ehdr: &Ehdr) -> Option<usize> {
    let count = usize::from(ehdr.e_phnum);
    if count > MAX_PHDRS {
        return None;
    }
    for index in 0..count {
        let header = phdr(base, index)?;
        if header.p_type == raven_libc_elf::consts::PT_DYNAMIC {
            return base.checked_add(usize::try_from(header.p_vaddr).ok()?);
        }
    }
    None
}

fn relocate_word(base: usize, offset: u64) -> Result<(), ()> {
    let offset = usize::try_from(offset).map_err(|_| ())?;
    let address = base.checked_add(offset).ok_or(())?;
    // SAFETY: the relocation table names a word inside the loader image.
    let value = unsafe { read_u64(address) };
    // SAFETY: the same word is writable during self-relocation.
    unsafe { write_u64(address, (base as u64).wrapping_add(value)) };
    Ok(())
}

fn apply_rela(base: usize, address: usize, size: usize, entry_size: usize) -> Result<(), ()> {
    if entry_size < 24 || size % entry_size != 0 {
        return Err(());
    }
    let mut offset = 0usize;
    while offset < size {
        let record = address.checked_add(offset).ok_or(())?;
        // SAFETY: the relocation table is inside the loader image and `offset` is bounded by
        // its declared size.
        let r_offset = unsafe { read_u64(record) };
        // SAFETY: `r_info` is the second word.
        let r_info = unsafe { read_u64(record.checked_add(8).ok_or(())?) };
        // SAFETY: `r_addend` is the third word.
        let r_addend = unsafe { read_u64(record.checked_add(16).ok_or(())?) };
        let kind = r_info & 0xffff_ffff;
        if kind == RELATIVE {
            let place = usize::try_from(r_offset).map_err(|_| ())?;
            let value = (base as u64).wrapping_add(r_addend);
            // SAFETY: a RELATIVE relocation names a writable word of the image.
            unsafe { write_u64(base.checked_add(place).ok_or(())?, value) };
        } else if kind != 0 {
            return Err(());
        }
        offset = offset.checked_add(entry_size).ok_or(())?;
    }
    Ok(())
}

fn apply_relr(base: usize, address: usize, size: usize) -> Result<(), ()> {
    if size % 8 != 0 {
        return Err(());
    }
    let mut offset = 0usize;
    let mut cursor = 0u64;
    while offset < size {
        // SAFETY: the RELR table is inside the loader image and `offset` is bounded.
        let entry = unsafe { read_u64(address.checked_add(offset).ok_or(())?) };
        if entry & 1 == 0 {
            relocate_word(base, entry)?;
            cursor = entry;
        } else {
            for bit in 1..64u32 {
                if entry & (1u64 << bit) != 0 {
                    relocate_word(base, cursor.wrapping_add(u64::from(bit).wrapping_mul(8)))?;
                }
            }
            // A bitmap covers the 63 words after its address entry; the next bitmap (or
            // address) continues after that run.
            cursor = cursor.wrapping_add(63u64.wrapping_mul(8));
        }
        offset = offset.checked_add(8).ok_or(())?;
    }
    Ok(())
}

/// Apply the loader's own relative relocations.
///
/// # Errors
///
/// `Err(())` when the header is not where the link puts it, when a non-relative relocation
/// appears in the loader (the link is `-Bsymbolic` and must not need one), or when a table is
/// malformed.
pub fn relocate_self(base: usize) -> Result<(), ()> {
    let ehdr = parse_ehdr(base).ok_or(())?;
    let dynamic = dynamic(base, &ehdr).ok_or(())?;
    let mut rela = 0u64;
    let mut rela_size = 0usize;
    let mut rela_entry = 24usize;
    let mut relr = 0u64;
    let mut relr_size = 0usize;
    let mut index = 0usize;
    while index < MAX_DYNAMIC {
        let address = dynamic
            .checked_add(index.checked_mul(16).ok_or(())?)
            .ok_or(())?;
        // SAFETY: the dynamic array is inside the loader image; the loop is bounded.
        let bytes = unsafe { core::slice::from_raw_parts(address as *const u8, 16) };
        let entry = Dyn::parse(bytes).map_err(|_| ())?;
        if entry.tag == DT_NULL {
            break;
        }
        match entry.tag {
            DT_RELA => rela = entry.val,
            DT_RELASZ => rela_size = usize::try_from(entry.val).map_err(|_| ())?,
            DT_RELAENT => rela_entry = usize::try_from(entry.val).map_err(|_| ())?,
            DT_RELR => relr = entry.val,
            DT_RELRSZ => relr_size = usize::try_from(entry.val).map_err(|_| ())?,
            _ => {}
        }
        index = index.checked_add(1).ok_or(())?;
    }
    if rela != 0 && rela_size != 0 {
        let address = base
            .checked_add(usize::try_from(rela).map_err(|_| ())?)
            .ok_or(())?;
        apply_rela(base, address, rela_size, rela_entry)?;
    }
    if relr != 0 && relr_size != 0 {
        let address = base
            .checked_add(usize::try_from(relr).map_err(|_| ())?)
            .ok_or(())?;
        apply_relr(base, address, relr_size)?;
    }
    Ok(())
}
