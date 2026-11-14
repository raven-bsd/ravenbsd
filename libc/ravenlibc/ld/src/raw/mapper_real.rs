// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The real `Mapper` over the Raven syscalls.
//!
//! The file layer is stubbed until the pager/`EXEC` PG items; the memory layer is the real
//! `MAP/UNMAP/PROTECT` sequence. `libc-link` only needs the object file to link, and C62's
//! host tests use `HostMapper`; the on-target half is C73.

use crate::mapper::{FileId, LoaderFile, MapErrKind, MapSource, Mapper, Platform, Prot};
use crate::raw::sys;

/// The real platform.
pub struct RealPlatform;

impl Mapper for RealPlatform {
    fn open(&mut self, _path: &[u8]) -> Result<LoaderFile, MapErrKind> {
        Err(MapErrKind::NoEnt)
    }

    fn read(
        &mut self,
        _file: &LoaderFile,
        _offset: u64,
        _buf: &mut [u8],
    ) -> Result<usize, MapErrKind> {
        Err(MapErrKind::Io)
    }

    fn close(&mut self, _file: LoaderFile) {}

    fn reserve(&mut self, len: u64) -> Result<u64, MapErrKind> {
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: an anonymous PROT_NONE map with a null hint.
        let result = unsafe { sys::syscall6(sys::MAP, 0, len, 0, 0, 0, 0) };
        if result < 0 {
            return Err(MapErrKind::NoMem);
        }
        Ok(result as u64)
    }

    fn release(&mut self, base: u64, len: u64) -> Result<(), MapErrKind> {
        let base = usize::try_from(base).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: the range came from `reserve`.
        let result = unsafe { sys::syscall2(sys::UNMAP, base, len) };
        if result < 0 {
            Err(MapErrKind::Inval)
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
        let flags = 1usize | (usize::from(prot.0) << 8);
        let source = match src {
            MapSource::Anonymous => 0usize,
            MapSource::File { file, .. } | MapSource::CopyOfFile { file, .. } => {
                usize::try_from(file).map_err(|_| MapErrKind::Inval)?
            }
        };
        // SAFETY: the caller chose the fixed address and the source handle.
        let result = unsafe { sys::syscall6(sys::MAP, addr, len, flags, source, 0, 0) };
        if result < 0 {
            Err(MapErrKind::Exists)
        } else {
            Ok(())
        }
    }

    fn protect(&mut self, addr: u64, len: u64, prot: Prot) -> Result<(), MapErrKind> {
        let addr = usize::try_from(addr).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: the range is one of the loader's mappings.
        let result = unsafe { sys::syscall3(sys::PROTECT, addr, len, usize::from(prot.0)) };
        if result < 0 {
            Err(MapErrKind::Inval)
        } else {
            Ok(())
        }
    }
}

impl Platform for RealPlatform {
    fn debug_write(&mut self, bytes: &[u8]) {
        sys::debug_write(bytes);
    }

    fn exit(&mut self, code: i32) -> ! {
        sys::exit(code)
    }
}

/// The file identity is not available until PG-36; keep the type used.
#[must_use]
pub fn empty_file_id() -> FileId {
    FileId::default()
}
