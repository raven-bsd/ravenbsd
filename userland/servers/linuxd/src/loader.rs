// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Map a planned Linux image into a new process through `raven-rt`'s `Sys` surface.
//!
//! The loader mirrors the native `raven_rt::spawn` sequence (image memory object, one mapping per
//! page step, a stack memory object and the first thread) with the Linux addresses from
//! [`crate::image`]. It deliberately stops before the first thread starts: the caller marks the
//! process with `PROC_SET_PERSONALITY` and only then creates the thread, so the very first Linux
//! syscall already traps to `linuxd`.
//!
//! Two read/write arenas (`brk` and anonymous `mmap`) are mapped once, object-backed, so the
//! handlers can serve those calls without a kernel round trip and the trap loop can copy results
//! out of them.

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, map, prot};
use raven_elf::StepKind;

use crate::image::ImageSpec;
use crate::table::Region;

/// The kernel surface the loader needs.
///
/// This mirrors `raven_rt::spawn::Sys` method for method. It is declared here so the loader and its
/// tests build on the host: `raven-rt` exports its `_start`, which cannot be linked into a host test
/// binary. `main.rs` adapts `raven_rt::spawn::RtSys` to it with one delegation per method.
pub trait Sys {
    /// Create an empty process.
    fn proc_create(&mut self) -> Result<Handle, Errno>;
    /// Create an anonymous, zero-filled memory object.
    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno>;
    /// Map `object` into `proc` at `addr`.
    fn map(
        &mut self,
        proc: Handle,
        object: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno>;
    /// Unmap a range in `proc`.
    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno>;
    /// Change the protection of a mapped range.
    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno>;
    /// Close a handle.
    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno>;
    /// Copy bytes into a mapping owned by this process.
    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]);
}

/// Page size used by the loader.
pub const PAGE: u64 = 4096;

/// Base of the pre-mapped `brk` arena.
pub const BRK_BASE: u64 = 0x0000_2000_0000_0000;
/// Size of the pre-mapped `brk` arena.
pub const BRK_SIZE: u64 = 1024 * 1024;

/// Base of the pre-mapped anonymous-`mmap` arena.
pub const MMAP_BASE: u64 = 0x0000_3000_0000_0000;
/// Size of the pre-mapped anonymous-`mmap` arena.
pub const MMAP_SIZE: u64 = 1024 * 1024;

/// Why a load failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoadError {
    /// A kernel call failed.
    Sys(Errno),
    /// An address or length does not fit the target.
    TooLarge,
}

/// A process whose image, stack and arenas are mapped but whose first thread has not started.
#[derive(Clone, Debug)]
pub struct Loaded {
    /// Process handle with all rights.
    pub proc: Handle,
    /// Entry point.
    pub entry: u64,
    /// Initial stack pointer.
    pub sp: u64,
    /// Initial program break.
    pub brk: u64,
    /// End of the break arena.
    pub brk_limit: u64,
    /// First address of the anonymous arena.
    pub mmap: u64,
    /// End of the anonymous arena.
    pub mmap_limit: u64,
    /// Everything the server may copy out of.
    pub regions: Vec<Region>,
}

/// Map `spec` and `image` into a fresh process.
pub fn load<S: Sys>(spec: &ImageSpec, image: &[u8], sys: &mut S) -> Result<Loaded, LoadError> {
    let mut regions = Vec::new();
    let image_object = stage_image(image, sys)?;
    let child = sys.proc_create().map_err(LoadError::Sys)?;
    let result = (|| {
        for step in &spec.steps {
            let vaddr = step
                .vaddr
                .checked_add(spec.bias)
                .ok_or(LoadError::TooLarge)?;
            match step.kind {
                StepKind::File { offset } => {
                    map_object(sys, child, image_object, offset, vaddr, step.len, step.prot)?;
                    regions.push(Region {
                        start: vaddr,
                        len: step.len,
                        backing: image_object,
                        backing_offset: offset,
                    });
                }
                StepKind::CopyTail { offset, len } => {
                    let object = stage_tail(image, offset, len, sys)?;
                    map_object(sys, child, object, 0, vaddr, PAGE, step.prot)?;
                    regions.push(Region {
                        start: vaddr,
                        len: PAGE,
                        backing: object,
                        backing_offset: 0,
                    });
                }
                StepKind::Zero => {
                    let flags = map::pack(step.prot as usize, map::FIXED);
                    sys.map(
                        child,
                        Handle::NONE,
                        0,
                        vaddr as usize,
                        step.len as usize,
                        flags,
                    )
                    .map_err(LoadError::Sys)?;
                    regions.push(Region {
                        start: vaddr,
                        len: step.len,
                        backing: Handle::NONE,
                        backing_offset: 0,
                    });
                }
            }
        }
        let stack = map_stack(spec, sys, child, &mut regions)?;
        let brk_object = map_arena(sys, child, BRK_BASE, BRK_SIZE)?;
        regions.push(Region {
            start: BRK_BASE,
            len: BRK_SIZE,
            backing: brk_object,
            backing_offset: 0,
        });
        let mmap_object = map_arena(sys, child, MMAP_BASE, MMAP_SIZE)?;
        regions.push(Region {
            start: MMAP_BASE,
            len: MMAP_SIZE,
            backing: mmap_object,
            backing_offset: 0,
        });
        Ok(Loaded {
            proc: child,
            entry: spec.entry,
            sp: stack,
            brk: spec.brk_start,
            brk_limit: spec.brk_start.saturating_add(BRK_SIZE),
            mmap: MMAP_BASE,
            mmap_limit: MMAP_BASE.saturating_add(MMAP_SIZE),
            regions,
        })
    })();
    match result {
        Ok(loaded) => Ok(loaded),
        Err(error) => {
            let _ = sys.handle_close(child);
            Err(error)
        }
    }
}

/// Copy the whole file image into one memory object the child's pages can be mapped from.
fn stage_image<S: Sys>(image: &[u8], sys: &mut S) -> Result<Handle, LoadError> {
    let length = align_up(image.len() as u64, PAGE).ok_or(LoadError::TooLarge)?;
    let length = usize::try_from(length).map_err(|_| LoadError::TooLarge)?;
    let object = sys.memobj_create(length).map_err(LoadError::Sys)?;
    let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, length, flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.handle_close(object);
            return Err(LoadError::Sys(error));
        }
    };
    sys.write_parent_mem(scratch, image);
    if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, length) {
        let _ = sys.handle_close(object);
        return Err(LoadError::Sys(error));
    }
    Ok(object)
}

/// A one-page object holding the tail of a segment's last file page.
fn stage_tail<S: Sys>(
    image: &[u8],
    offset: u64,
    len: u64,
    sys: &mut S,
) -> Result<Handle, LoadError> {
    let start = usize::try_from(offset).map_err(|_| LoadError::TooLarge)?;
    let len = usize::try_from(len).map_err(|_| LoadError::TooLarge)?;
    let end = start.checked_add(len).ok_or(LoadError::TooLarge)?;
    let bytes = image.get(start..end).ok_or(LoadError::TooLarge)?;
    let page = PAGE as usize;
    let object = sys.memobj_create(page).map_err(LoadError::Sys)?;
    let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, page, flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.handle_close(object);
            return Err(LoadError::Sys(error));
        }
    };
    sys.write_parent_mem(scratch, bytes);
    if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, page) {
        let _ = sys.handle_close(object);
        return Err(LoadError::Sys(error));
    }
    Ok(object)
}

/// Map one page step from `object`, turning an executable mapping into RW + `protect`.
fn map_object<S: Sys>(
    sys: &mut S,
    child: Handle,
    object: Handle,
    offset: u64,
    vaddr: u64,
    len: u64,
    protection: u8,
) -> Result<(), LoadError> {
    let offset = usize::try_from(offset).map_err(|_| LoadError::TooLarge)?;
    let len = usize::try_from(len).map_err(|_| LoadError::TooLarge)?;
    let vaddr = usize::try_from(vaddr).map_err(|_| LoadError::TooLarge)?;
    let executable = protection & prot::EXEC as u8 != 0;
    let flags = if executable {
        map::pack(
            (protection as usize & !prot::EXEC) | prot::WRITE,
            map::FIXED,
        )
    } else {
        map::pack(protection as usize, map::FIXED)
    };
    sys.map(child, object, offset, vaddr, len, flags)
        .map_err(LoadError::Sys)?;
    if executable {
        // Generic memory objects carry no EXEC right in ABI v1 (S24); the text is populated RW and
        // protected read-execute before the first thread exists.
        sys.protect(child, vaddr, len, protection)
            .map_err(LoadError::Sys)?;
    }
    Ok(())
}

/// Map the initial stack and return its initial pointer.
fn map_stack<S: Sys>(
    spec: &ImageSpec,
    sys: &mut S,
    child: Handle,
    regions: &mut Vec<Region>,
) -> Result<u64, LoadError> {
    let base = spec.stack.base;
    let page_base = base - (base % PAGE);
    let end = align_up(base + spec.stack.bytes.len() as u64, PAGE).ok_or(LoadError::TooLarge)?;
    let length = end.checked_sub(page_base).ok_or(LoadError::TooLarge)?;
    let length = usize::try_from(length).map_err(|_| LoadError::TooLarge)?;
    let object = sys.memobj_create(length).map_err(LoadError::Sys)?;
    let flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
    let scratch = match sys.map(Handle::SELF_PROC, object, 0, 0, length, flags) {
        Ok(address) => address,
        Err(error) => {
            let _ = sys.handle_close(object);
            return Err(LoadError::Sys(error));
        }
    };
    let offset = usize::try_from(base - page_base).map_err(|_| LoadError::TooLarge)?;
    let write_at = scratch.checked_add(offset).ok_or(LoadError::TooLarge)?;
    sys.write_parent_mem(write_at, &spec.stack.bytes);
    if let Err(error) = sys.unmap(Handle::SELF_PROC, scratch, length) {
        let _ = sys.handle_close(object);
        return Err(LoadError::Sys(error));
    }
    let page_base = usize::try_from(page_base).map_err(|_| LoadError::TooLarge)?;
    let child_flags = map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED);
    if let Err(error) = sys.map(child, object, 0, page_base, length, child_flags) {
        let _ = sys.handle_close(object);
        return Err(LoadError::Sys(error));
    }
    regions.push(Region {
        start: page_base as u64,
        len: length as u64,
        backing: object,
        backing_offset: 0,
    });
    Ok(spec.stack.sp)
}

/// Map a private read/write arena at a fixed address.
fn map_arena<S: Sys>(
    sys: &mut S,
    child: Handle,
    base: u64,
    size: u64,
) -> Result<Handle, LoadError> {
    let size = usize::try_from(size).map_err(|_| LoadError::TooLarge)?;
    let base = usize::try_from(base).map_err(|_| LoadError::TooLarge)?;
    let object = sys.memobj_create(size).map_err(LoadError::Sys)?;
    let flags = map::pack(prot::READ | prot::WRITE, map::SHARED | map::FIXED);
    match sys.map(child, object, 0, base, size, flags) {
        Ok(_) => Ok(object),
        Err(error) => {
            let _ = sys.handle_close(object);
            Err(LoadError::Sys(error))
        }
    }
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    let remainder = value % align;
    if remainder == 0 {
        Some(value)
    } else {
        value.checked_add(align - remainder)
    }
}

#[cfg(test)]
#[path = "loader_ut.rs"]
mod tests;
