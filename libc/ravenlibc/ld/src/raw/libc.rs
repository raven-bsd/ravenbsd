// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Map the pre-passed libc image from the initfs without the file-server copy (K139).
//!
//! The spawner forwards the libc archive offset in `AT_RAVEN_LIBC`. The loader maps the ELF
//! header page from `Handle::INITFS`, plans the segments with `raven-libc-elf`, and maps each
//! file-backed segment directly from the initfs at a fixed bias. The previous `file:` path copied
//! roughly 1.3 MiB through the file server; on TCG the RX segment alone cost about 1.2 s.

use core::ptr;

use raven_libc_elf::consts::PT_PHDR;
use raven_libc_elf::segments::{StepKind, plan_segments};
use raven_libc_elf::{Ehdr, Phdr};

use crate::loadobj::Loader;
use crate::main_flow::register_premapped;
use crate::raw::applier::LiveApplier;
use crate::raw::mapper_real::RealPlatform;
use crate::raw::memview::LiveMem;
use crate::raw::sys;

/// The loader process handle.
const SELF: usize = 0;
/// The fixed initfs handle reserved by the kernel ABI.
const INITFS: usize = 1;
/// The anonymous-object marker in `MAP`.
const NONE: usize = u32::MAX as usize;
/// `map::FIXED` in the flag word.
const MAP_FIXED: usize = 1 << 8;
/// Protection bits in the `MAP`/`PROTECT` argument.
const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const PROT_EXEC: usize = 4;
/// Page size for the archive mapping and the ELF plan.
const PAGE: usize = 4096;
/// Fixed libc bias, between the interpreter and the loader's generic library hint.
const LIBC_BASE: usize = 0x0000_3000_0000_0000;
/// Scratch page for the archive ELF header; must be outside the libc span.
const SCRATCH: usize = 0x0000_5000_0000_0000;

type TargetLoader = Loader<RealPlatform, LiveMem, LiveApplier>;

/// Convert an ELF `PF_*` protection word to the syscall bits.
fn prot_bits(p_flags: u32) -> usize {
    let mut bits = 0usize;
    if p_flags & 4 != 0 {
        bits |= PROT_READ;
    }
    if p_flags & 2 != 0 {
        bits |= PROT_WRITE;
    }
    if p_flags & 1 != 0 {
        bits |= PROT_EXEC;
    }
    bits
}

fn map_memobj(
    memobj: usize,
    offset: usize,
    addr: usize,
    len: usize,
    flags: usize,
) -> Result<(), ()> {
    // SAFETY: MAP takes scalar arguments; the kernel validates the handle and the range.
    let result =
        unsafe { sys::syscall6(sys::MAP, SELF, memobj, offset, addr, len, flags | MAP_FIXED) };
    if result < 0 { Err(()) } else { Ok(()) }
}

fn protect(addr: usize, len: usize, prot: usize) -> Result<(), ()> {
    // SAFETY: PROTECT takes the process handle, range and protection word.
    let result = unsafe { sys::syscall4(sys::PROTECT, SELF, addr, len, prot) };
    if result < 0 { Err(()) } else { Ok(()) }
}

fn unmap(addr: usize, len: usize) {
    // SAFETY: UNMAP takes the process handle, range and length.
    let _ = unsafe { sys::syscall3(sys::UNMAP, SELF, addr, len) };
}

fn zero(address: usize, len: usize) {
    // SAFETY: the caller mapped `address..address+len` writable for this operation.
    unsafe { ptr::write_bytes(address as *mut u8, 0, len) };
}

/// Map and register libc from the initfs archive at `data_offset`.
///
/// Returns `false` on any malformed image or mapping failure, so the loader can fall back to the
/// existing `file:` path instead of failing the process.
pub(crate) fn map_premapped(loader: &mut TargetLoader, data_offset: u64) -> bool {
    let Ok(data_offset) = usize::try_from(data_offset) else {
        return false;
    };
    let header_base = SCRATCH;
    if map_memobj(
        INITFS,
        data_offset,
        header_base,
        PAGE,
        PROT_READ | PROT_WRITE,
    )
    .is_err()
    {
        return false;
    }
    // SAFETY: the header page is mapped for reading.
    let header_bytes = unsafe { core::slice::from_raw_parts(header_base as *const u8, 64) };
    let Ok(ehdr) = Ehdr::parse(header_bytes) else {
        return false;
    };
    let Ok(phoff) = usize::try_from(ehdr.e_phoff) else {
        return false;
    };
    let phnum = usize::from(ehdr.e_phnum);
    let Some(table_len) = phnum.checked_mul(56) else {
        return false;
    };
    let Some(table_end) = phoff.checked_add(table_len) else {
        return false;
    };
    if table_end > PAGE || phnum > 16 {
        return false;
    }
    // SAFETY: the table lies inside the mapped header page.
    let table =
        unsafe { core::slice::from_raw_parts((header_base + phoff) as *const u8, table_len) };
    let Ok(empty) = Phdr::parse(&[0u8; 56]) else {
        return false;
    };
    let mut phdrs = [empty; 16];
    for index in 0..phnum {
        let start = index.saturating_mul(56);
        let Some(bytes) = table.get(start..start.saturating_add(56)) else {
            return false;
        };
        let Ok(phdr) = Phdr::parse(bytes) else {
            return false;
        };
        phdrs[index] = phdr;
    }
    let Ok(plan) = plan_segments(&phdrs[..phnum], PAGE as u64) else {
        return false;
    };
    unmap(SCRATCH, PAGE);
    let Some(bias) = (LIBC_BASE as u64).checked_sub(plan.base_vaddr) else {
        return false;
    };
    let mut phdr_vaddr = None;
    for phdr in &phdrs[..phnum] {
        if phdr.p_type == PT_PHDR {
            phdr_vaddr = Some(phdr.p_vaddr);
        }
    }
    let Some(phdr_vaddr) = phdr_vaddr else {
        return false;
    };

    for (index, step) in plan.steps.iter().enumerate() {
        if step.kind == StepKind::Tail {
            continue;
        }
        let Some(relative) = step.vaddr.checked_sub(plan.base_vaddr) else {
            return false;
        };
        let Ok(relative) = usize::try_from(relative) else {
            return false;
        };
        let address = LIBC_BASE.saturating_add(relative);
        let Ok(len) = usize::try_from(step.len) else {
            return false;
        };
        let prot = prot_bits(u32::from(step.prot));
        let tail = plan.steps.get(index.saturating_add(1)).filter(|next| {
            next.kind == StepKind::Tail
                && next.vaddr >= step.vaddr
                && next.vaddr < step.vaddr.saturating_add(step.len)
        });
        match step.kind {
            StepKind::File => {
                let Ok(file_off) = usize::try_from(step.file_off) else {
                    return false;
                };
                let Some(offset) = data_offset.checked_add(file_off) else {
                    return false;
                };
                if let Some(tail) = tail {
                    if map_memobj(INITFS, offset, address, len, PROT_READ | PROT_WRITE).is_err() {
                        return false;
                    }
                    let Some(tail_relative) = tail.vaddr.checked_sub(step.vaddr) else {
                        return false;
                    };
                    let Ok(tail_relative) = usize::try_from(tail_relative) else {
                        return false;
                    };
                    let Ok(tail_len) = usize::try_from(tail.len) else {
                        return false;
                    };
                    zero(address.saturating_add(tail_relative), tail_len);
                    if protect(address, len, prot).is_err() {
                        return false;
                    }
                } else if map_memobj(INITFS, offset, address, len, prot).is_err() {
                    return false;
                }
            }
            StepKind::Zero => {
                if map_memobj(NONE, 0, address, len, prot).is_err() {
                    return false;
                }
            }
            StepKind::Tail => {}
        }
    }

    let Some(phdr) = bias.checked_add(phdr_vaddr) else {
        return false;
    };
    register_premapped(loader, phdr, ehdr.e_phnum, b"libc.so.1").is_ok()
}
