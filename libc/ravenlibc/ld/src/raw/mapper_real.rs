// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The real `Mapper` over the Raven syscalls.
//!
//! Files are opened through the `file` scheme; a mapped segment is an anonymous fixed mapping
//! filled with `PREAD` (the copy-load path), because a scheme-file handle is not a mappable
//! memory object. The reservation probe and the final fixed mappings are the same
//! `MAP`/`UNMAP`/`PROTECT` sequence spawn uses for a child.

use crate::mapper::{FileId, LoaderFile, MapErrKind, MapSource, Mapper, Platform, Prot};
use crate::raw::sys;
use crate::table::RtldTable;

/// `Handle::SELF_PROC` is zero in the Raven ABI.
const SELF: usize = 0;
/// `Handle::NONE` is the anonymous-object marker in `MAP`.
const NONE: usize = u32::MAX as usize;
/// Preferred library region, above the main PIE and interpreter bases so the link-time vaddr
/// subtracted in `map_object` never produces a negative bias.
const LIB_HINT: usize = 0x0000_4000_0000_0000;
/// Protection bits in the `MAP`/`PROTECT` argument.
const PROT_READ: usize = 1;
const PROT_WRITE: usize = 2;
const PROT_EXEC: usize = 4;
/// `map::FIXED` shifted into the map-flags position.
const MAP_FIXED: usize = 1 << 8;
/// `WIRE_OPEN_READ`.
const OPEN_READ: usize = 1;
/// The scheme prefix the kernel routes to the file server.
const FILE_SCHEME: &[u8] = b"file:";
/// Longest path the loader hands to `OPEN`.
const PATH_MAX: usize = 256;

/// The real platform.
pub struct RealPlatform;

fn map_err(errno: isize) -> MapErrKind {
    match -errno {
        2 => MapErrKind::NoEnt,
        13 => MapErrKind::Access,
        17 => MapErrKind::Exists,
        12 => MapErrKind::NoMem,
        22 => MapErrKind::Inval,
        38 => MapErrKind::NoSys,
        _ => MapErrKind::Io,
    }
}

fn prot_bits(prot: Prot) -> usize {
    let mut bits = 0usize;
    if prot.0 & 1 != 0 {
        bits |= PROT_READ;
    }
    if prot.0 & 2 != 0 {
        bits |= PROT_WRITE;
    }
    if prot.0 & 4 != 0 {
        bits |= PROT_EXEC;
    }
    bits
}

fn map_self(memobj: usize, offset: usize, addr: usize, len: usize, flags: usize) -> isize {
    // SAFETY: MAP takes scalar arguments; the kernel validates the handles and the range.
    unsafe { sys::syscall6(sys::MAP, SELF, memobj, offset, addr, len, flags) }
}

fn unmap_self(addr: usize, len: usize) -> isize {
    // SAFETY: UNMAP takes the process handle, the range and the length.
    unsafe { sys::syscall3(sys::UNMAP, SELF, addr, len) }
}

fn protect_self(addr: usize, len: usize, prot: usize) -> isize {
    // SAFETY: PROTECT takes the process handle, the range and the protection word.
    unsafe { sys::syscall4(sys::PROTECT, SELF, addr, len, prot) }
}

/// Read exactly `len` bytes at `offset` into `destination`; a short file is not an error.
fn read_into(handle: u32, offset: u64, destination: usize, len: usize) -> Result<(), MapErrKind> {
    let offset = usize::try_from(offset).map_err(|_| MapErrKind::Inval)?;
    let mut done = 0usize;
    while done < len {
        let address = destination.checked_add(done).ok_or(MapErrKind::Inval)?;
        let remaining = len.saturating_sub(done);
        // SAFETY: PREAD writes `remaining` bytes into the caller's writable mapping.
        let result = unsafe {
            sys::syscall4(
                sys::PREAD,
                handle as usize,
                address,
                remaining,
                offset + done,
            )
        };
        if result < 0 {
            return Err(map_err(result));
        }
        let count = usize::try_from(result).map_err(|_| MapErrKind::Inval)?;
        if count == 0 {
            break;
        }
        done = done.saturating_add(count);
    }
    Ok(())
}

impl Mapper for RealPlatform {
    fn open(&mut self, path: &[u8]) -> Result<LoaderFile, MapErrKind> {
        if path.len() > PATH_MAX {
            return Err(MapErrKind::Inval);
        }
        let mut buffer = [0u8; PATH_MAX + FILE_SCHEME.len()];
        let prefix = path.starts_with(FILE_SCHEME);
        let start = if prefix { 0 } else { FILE_SCHEME.len() };
        if !prefix {
            if let Some(slot) = buffer.get_mut(..FILE_SCHEME.len()) {
                slot.copy_from_slice(FILE_SCHEME);
            }
        }
        let end = start.checked_add(path.len()).ok_or(MapErrKind::Inval)?;
        if let (Some(destination), Some(source)) = (buffer.get_mut(start..end), Some(path)) {
            destination.copy_from_slice(source);
        } else {
            return Err(MapErrKind::Inval);
        }
        // SAFETY: OPEN reads the path bytes and takes the wire flag word.
        let result = unsafe { sys::syscall3(sys::OPEN, buffer.as_ptr() as usize, end, OPEN_READ) };
        if result < 0 {
            return Err(map_err(result));
        }
        let handle = u32::try_from(result).map_err(|_| MapErrKind::Inval)?;
        Ok(LoaderFile {
            handle,
            id: FileId::default(),
        })
    }

    fn read(
        &mut self,
        file: &LoaderFile,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, MapErrKind> {
        let offset = usize::try_from(offset).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: PREAD writes at most `buf.len()` bytes into the valid destination slice.
        let result = unsafe {
            sys::syscall4(
                sys::PREAD,
                file.handle as usize,
                buf.as_mut_ptr() as usize,
                buf.len(),
                offset,
            )
        };
        if result < 0 {
            return Err(map_err(result));
        }
        usize::try_from(result).map_err(|_| MapErrKind::Inval)
    }

    fn close(&mut self, file: LoaderFile) {
        // SAFETY: CLOSE takes the handle; the result is intentionally ignored.
        let _ = unsafe { sys::syscall1(sys::CLOSE, file.handle as usize) };
    }

    fn reserve(&mut self, len: u64) -> Result<u64, MapErrKind> {
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        if len == 0 {
            return Err(MapErrKind::Inval);
        }
        let result = map_self(NONE, 0, LIB_HINT, len, PROT_READ);
        if result < 0 {
            return Err(map_err(result));
        }
        u64::try_from(result).map_err(|_| MapErrKind::Inval)
    }

    fn release(&mut self, base: u64, len: u64) -> Result<(), MapErrKind> {
        let base = usize::try_from(base).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        let result = unmap_self(base, len);
        if result < 0 {
            Err(map_err(result))
        } else {
            Ok(())
        }
    }

    fn map_fixed(
        &mut self,
        addr: u64,
        len: u64,
        prot: Prot,
        src: MapSource,
    ) -> Result<(), MapErrKind> {
        if prot.is_wx() {
            return Err(MapErrKind::Inval);
        }
        let addr = usize::try_from(addr).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        match src {
            MapSource::Anonymous => {
                let result = map_self(NONE, 0, addr, len, prot_bits(prot) | MAP_FIXED);
                if result < 0 {
                    return Err(map_err(result));
                }
                Ok(())
            }
            MapSource::File { file, offset } => {
                self.map_copy(file, offset, addr, len, len as u64, prot)
            }
            MapSource::CopyOfFile {
                file,
                offset,
                len: copy,
            } => self.map_copy(file, offset, addr, len, copy, prot),
        }
    }

    fn protect(&mut self, addr: u64, len: u64, prot: Prot) -> Result<(), MapErrKind> {
        let addr = usize::try_from(addr).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        let result = protect_self(addr, len, prot_bits(prot));
        if result < 0 {
            Err(map_err(result))
        } else {
            Ok(())
        }
    }
}

impl RealPlatform {
    fn map_copy(
        &mut self,
        file: u32,
        offset: u64,
        addr: usize,
        len: usize,
        copy: u64,
        prot: Prot,
    ) -> Result<(), MapErrKind> {
        // The copy needs a writable mapping even for RX text; the kernel rejects W|X, so drop
        // EXEC for the copy and apply the final `PROTECT` afterwards.
        let writable = (prot_bits(prot) & !PROT_EXEC) | PROT_WRITE;
        let result = map_self(NONE, 0, addr, len, writable | MAP_FIXED);
        if result < 0 {
            return Err(map_err(result));
        }
        let copy = usize::try_from(copy).map_err(|_| MapErrKind::Inval)?;
        if let Err(error) = read_into(file, offset, addr, copy.min(len)) {
            let _ = unmap_self(addr, len);
            return Err(error);
        }
        if prot_bits(prot) != writable {
            let result = protect_self(addr, len, prot_bits(prot));
            if result < 0 {
                let _ = unmap_self(addr, len);
                return Err(map_err(result));
            }
        }
        Ok(())
    }
}

impl Platform for RealPlatform {
    fn debug_write(&mut self, bytes: &[u8]) {
        sys::debug_write(bytes);
    }

    fn exit(&mut self, code: i32) -> ! {
        sys::exit(code)
    }

    fn install_rtld_table(&mut self, address: usize, table: *const RtldTable) -> bool {
        if address == 0 || table.is_null() {
            return false;
        }
        // SAFETY: the symbol lookup returned the ABI of `__rl_dl_install`, which takes one
        // pointer to the loader-owned table and stores it for the process lifetime.
        let install: extern "C" fn(*const RtldTable) = unsafe { core::mem::transmute(address) };
        install(table);
        true
    }

    fn call_function(&mut self, address: usize) -> bool {
        if address == 0 {
            return false;
        }
        // SAFETY: the caller passes a relocated destructor or constructor from a validated
        // object; the C ABI entry takes no arguments.
        let function: extern "C" fn() = unsafe { core::mem::transmute(address) };
        function();
        true
    }
}

/// The file identity is not available until PG-36; keep the type used.
#[must_use]
pub fn empty_file_id() -> FileId {
    FileId::default()
}
