// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A host `Platform` over Linux `mmap`, with a copy-load mapping and a write guard.

#![allow(unsafe_code)]
#![allow(
    dead_code,
    unused_variables,
    clippy::undocumented_unsafe_blocks,
    clippy::needless_question_mark
)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::rc::Rc;

use raven_libc_elf::{ApplyError, MemView, RelocApplier};
use raven_libc_rtld::mapper::{FileId, LoaderFile, MapErrKind, MapSource, Mapper, Platform, Prot};

const PROT_NONE: i32 = 0;
const PROT_READ: i32 = 1;
const PROT_WRITE: i32 = 2;
const PROT_EXEC: i32 = 4;
const MAP_PRIVATE: i32 = 2;
const MAP_ANONYMOUS: i32 = 0x20;
const MAP_FIXED_NOREPLACE: i32 = 0x10;

unsafe extern "C" {
    fn mmap(addr: *mut c_void, len: usize, prot: i32, flags: i32, fd: i32, off: i64)
    -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> i32;
    fn mprotect(addr: *mut c_void, len: usize, prot: i32) -> i32;
}

fn failed(pointer: *mut c_void) -> bool {
    pointer as isize == -1
}

fn flags(prot: Prot) -> i32 {
    let mut value = 0;
    if prot.0 & 1 != 0 {
        value |= PROT_READ;
    }
    if prot.0 & 2 != 0 {
        value |= PROT_WRITE;
    }
    if prot.0 & 4 != 0 {
        value |= PROT_EXEC;
    }
    value
}

#[derive(Clone, Debug)]
struct FileData {
    bytes: Vec<u8>,
    id: FileId,
}

/// Shared state between the mapper, the memory view and the applier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    Ifunc(u64),
    Protect(u64),
}

#[derive(Default)]
struct State {
    files: Vec<FileData>,
    ranges: Vec<(u64, u64, bool)>,
    writes_outside: usize,
    events: Vec<Event>,
}

type Shared = Rc<RefCell<State>>;

/// The host mapper.
pub struct HostMapper {
    state: Shared,
    fail_next: usize,
    fail_kind: MapErrKind,
}

impl HostMapper {
    /// A mapper with an empty state.
    pub fn new() -> (Self, HostMem, HostApplier) {
        let state = Rc::new(RefCell::new(State::default()));
        (
            Self {
                state: state.clone(),
                fail_next: 0,
                fail_kind: MapErrKind::Exists,
            },
            HostMem {
                state: state.clone(),
            },
            HostApplier { state },
        )
    }

    /// Inject `count` failures of `kind` on the next `map_fixed` calls.
    pub fn fail_next_map_fixed(&mut self, count: usize, kind: MapErrKind) {
        self.fail_next = count;
        self.fail_kind = kind;
    }

    /// The number of writes outside a mapped range.
    pub fn writes_outside(&self) -> usize {
        self.state.borrow().writes_outside
    }

    /// The ordered IFUNC/RELRO events.
    pub fn events(&self) -> Vec<Event> {
        self.state.borrow().events.clone()
    }

    /// The range containing `address`, if any.
    pub fn range(&self, address: u64) -> Option<(u64, u64)> {
        self.state
            .borrow()
            .ranges
            .iter()
            .find(|(base, len, _)| address >= *base && address < base.saturating_add(*len))
            .map(|(base, len, _)| (*base, *len))
    }
}

impl Mapper for HostMapper {
    fn open(&mut self, path: &[u8]) -> Result<LoaderFile, MapErrKind> {
        let path = std::str::from_utf8(path).map_err(|_| MapErrKind::NoEnt)?;
        let bytes = std::fs::read(path).map_err(|_| MapErrKind::NoEnt)?;
        let metadata = std::fs::metadata(path).map_err(|_| MapErrKind::NoEnt)?;
        let id = FileId {
            dev: 0,
            ino: 0,
            size: metadata.len(),
        };
        let handle =
            u32::try_from(self.state.borrow().files.len()).map_err(|_| MapErrKind::NoMem)?;
        self.state.borrow_mut().files.push(FileData { bytes, id });
        Ok(LoaderFile { handle, id })
    }

    fn read(
        &mut self,
        file: &LoaderFile,
        offset: u64,
        buf: &mut [u8],
    ) -> Result<usize, MapErrKind> {
        let state = self.state.borrow();
        let file = state
            .files
            .get(usize::try_from(file.handle).map_err(|_| MapErrKind::Inval)?)
            .ok_or(MapErrKind::Inval)?;
        let start = usize::try_from(offset).map_err(|_| MapErrKind::Inval)?;
        let available = file.bytes.len().saturating_sub(start).min(buf.len());
        if let (Some(source), Some(target)) = (
            file.bytes.get(start..start.saturating_add(available)),
            buf.get_mut(..available),
        ) {
            target.copy_from_slice(source);
        }
        Ok(available)
    }

    fn close(&mut self, _file: LoaderFile) {}

    fn reserve(&mut self, len: u64) -> Result<u64, MapErrKind> {
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: an anonymous PROT_NONE reservation.
        let pointer = unsafe {
            mmap(
                std::ptr::null_mut(),
                len,
                PROT_NONE,
                MAP_PRIVATE | MAP_ANONYMOUS,
                -1,
                0,
            )
        };
        if failed(pointer) {
            return Err(MapErrKind::NoMem);
        }
        Ok(pointer as u64)
    }

    fn release(&mut self, base: u64, len: u64) -> Result<(), MapErrKind> {
        let base = usize::try_from(base).map_err(|_| MapErrKind::Inval)?;
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: the range came from `reserve`.
        let result = unsafe { munmap(base as *mut c_void, len) };
        if result == 0 {
            let base64 = u64::try_from(base).unwrap_or(u64::MAX);
            self.state.borrow_mut().ranges.retain(|(start, length, _)| {
                base64 >= start.saturating_add(*length) || base64 < *start
            });
            Ok(())
        } else {
            Err(MapErrKind::Inval)
        }
    }

    fn map_fixed(
        &mut self,
        addr: u64,
        len: u64,
        prot: Prot,
        src: MapSource,
    ) -> Result<(), MapErrKind> {
        if self.fail_next > 0 {
            self.fail_next = self.fail_next.saturating_sub(1);
            return Err(self.fail_kind);
        }
        if prot.is_wx() {
            return Err(MapErrKind::Inval);
        }
        let len = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        let copy_source = matches!(src, MapSource::File { .. } | MapSource::CopyOfFile { .. });
        let map_prot = if copy_source {
            flags(prot) | PROT_WRITE
        } else {
            flags(prot)
        };
        // SAFETY: the caller chose the fixed address and the mapping is private-anonymous.
        let pointer = unsafe {
            mmap(
                addr as *mut c_void,
                len,
                map_prot,
                MAP_PRIVATE | MAP_ANONYMOUS | MAP_FIXED_NOREPLACE,
                -1,
                0,
            )
        };
        if failed(pointer) {
            eprintln!(
                "host mapper: fixed {addr:x} len {len:x} prot {:x} failed: {}",
                prot.0,
                std::io::Error::last_os_error()
            );
            return Err(MapErrKind::Exists);
        }
        if pointer as u64 != addr {
            eprintln!(
                "host mapper: requested {addr:x}, got {:x}, len {len:x}, prot {:x}",
                pointer as u64, prot.0
            );
            // The kernel ignored `MAP_FIXED_NOREPLACE` (or chose a different address): do not
            // write through the requested address, which is not our mapping.
            // SAFETY: `pointer` is a fresh mapping from `mmap`.
            unsafe {
                let _ = munmap(pointer, len);
            }
            return Err(MapErrKind::Exists);
        }
        let copy_len = match src {
            MapSource::CopyOfFile { len, .. } => usize::try_from(len).unwrap_or(0),
            _ => len,
        };
        if let MapSource::File { file, offset }
        | MapSource::CopyOfFile {
            file,
            offset,
            len: _,
        } = src
        {
            let state = self.state.borrow();
            if let Some(data) = state
                .files
                .get(usize::try_from(file).map_err(|_| MapErrKind::Inval)?)
            {
                let start = usize::try_from(offset).map_err(|_| MapErrKind::Inval)?;
                let available = data.bytes.len().saturating_sub(start).min(copy_len);
                if available != 0 {
                    // SAFETY: the destination is the just-mapped range.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            data.bytes.as_ptr().add(start),
                            pointer as *mut u8,
                            available,
                        );
                    }
                }
            }
        }
        if copy_source {
            // The copy needs write access; return to the requested protection now.
            // SAFETY: the range is our fresh mapping.
            let result = unsafe { mprotect(pointer, len, flags(prot)) };
            if result != 0 {
                return Err(MapErrKind::Inval);
            }
        }
        self.state.borrow_mut().ranges.push((
            addr,
            u64::try_from(len).unwrap_or(0),
            prot.0 & 2 != 0,
        ));
        Ok(())
    }

    fn protect(&mut self, addr: u64, len: u64, prot: Prot) -> Result<(), MapErrKind> {
        let address = usize::try_from(addr).map_err(|_| MapErrKind::Inval)?;
        let length = usize::try_from(len).map_err(|_| MapErrKind::Inval)?;
        // SAFETY: the range is one of the loader's mappings.
        let result = unsafe { mprotect(address as *mut c_void, length, flags(prot)) };
        if result != 0 {
            eprintln!(
                "host protect {addr:x} {len:x} prot {:x} failed: {}",
                prot.0,
                std::io::Error::last_os_error()
            );
            return Err(MapErrKind::Inval);
        }
        let mut state = self.state.borrow_mut();
        state.events.push(Event::Protect(addr));
        for (base, size, writable) in &mut state.ranges {
            if addr >= *base && addr < base.saturating_add(*size) {
                *writable = prot.0 & 2 != 0;
            }
        }
        Ok(())
    }
}

impl Platform for HostMapper {
    fn debug_write(&mut self, bytes: &[u8]) {
        let _ = std::io::Write::write_all(&mut std::io::stderr(), bytes);
    }

    fn exit(&mut self, code: i32) -> ! {
        panic!("host loader exit {code}")
    }
}

/// A `MemView` over the host mappings.
pub struct HostMem {
    state: Shared,
}

impl MemView for HostMem {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        let state = self.state.borrow();
        let (base, size, _) = state
            .ranges
            .iter()
            .find(|(base, size, _)| {
                vaddr >= *base
                    && vaddr.saturating_add(u64::try_from(len).unwrap_or(0))
                        <= base.saturating_add(*size)
            })
            .copied()?;
        let start = usize::try_from(vaddr.checked_sub(base)?).ok()?;
        let end = start.checked_add(len)?;
        // SAFETY: the range is mapped and the bounds check above proves the slice fits.
        Some(unsafe { std::slice::from_raw_parts((base as *const u8).add(start), end - start) })
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        let state = self.state.borrow();
        let (base, size, _) = state
            .ranges
            .iter()
            .find(|(base, size, _)| vaddr >= *base && vaddr < base.saturating_add(*size))
            .copied()?;
        let start = usize::try_from(vaddr.checked_sub(base)?).ok()?;
        let length = usize::try_from(size).ok()?;
        // SAFETY: the range is mapped; the scan stops at the range end.
        let bytes =
            unsafe { std::slice::from_raw_parts((base as *const u8).add(start), length - start) };
        let end = bytes.iter().position(|byte| *byte == 0)?;
        bytes.get(..end)
    }
}

/// A `RelocApplier` over the host mappings with a read-only write guard.
pub struct HostApplier {
    state: Shared,
}

impl RelocApplier for HostApplier {
    fn write64(&mut self, addr: u64, val: u64) -> Result<(), ApplyError> {
        self.write(addr, &val.to_le_bytes())
    }

    fn write32(&mut self, addr: u64, val: u32) -> Result<(), ApplyError> {
        self.write(addr, &val.to_le_bytes())
    }

    fn read64(&self, addr: u64) -> Option<u64> {
        let state = self.state.borrow();
        let (base, size, _) = state
            .ranges
            .iter()
            .find(|(base, size, _)| {
                addr >= *base && addr.saturating_add(8) <= base.saturating_add(*size)
            })
            .copied()?;
        let start = usize::try_from(addr.checked_sub(base)?).ok()?;
        let bytes: [u8; 8] = unsafe {
            std::slice::from_raw_parts((base as *const u8).add(start), 8)
                .try_into()
                .ok()?
        };
        Some(u64::from_le_bytes(bytes))
    }

    fn call_ifunc(
        &mut self,
        resolver: u64,
        _arg: &raven_libc_elf::IfuncArg,
    ) -> Result<u64, ApplyError> {
        self.state.borrow_mut().events.push(Event::Ifunc(resolver));
        // SAFETY: the fixture's resolver is a no-argument function returning an address.
        let function: extern "C" fn() -> usize = unsafe { std::mem::transmute(resolver as usize) };
        Ok(u64::try_from(function()).map_err(|_| ApplyError::BadIfunc)?)
    }

    fn tls_module_id(&self, obj: raven_libc_elf::ObjId) -> u64 {
        u64::from(obj.0).saturating_add(1)
    }

    fn tls_static_offset(&mut self, _obj: raven_libc_elf::ObjId) -> Result<i64, ApplyError> {
        Ok(0)
    }

    fn tls_desc_alloc(
        &mut self,
        _obj: raven_libc_elf::ObjId,
        _sym_value: u64,
        _addend: i64,
    ) -> Result<raven_libc_elf::TlsDesc, ApplyError> {
        Ok(raven_libc_elf::TlsDesc {
            resolver: 0,
            arg: 0,
        })
    }

    fn copy(&mut self, dst: u64, src: u64, len: u64) -> Result<(), ApplyError> {
        let bytes = {
            let state = self.state.borrow();
            let (base, size, _) = state
                .ranges
                .iter()
                .find(|(base, size, _)| {
                    src >= *base && src.saturating_add(len) <= base.saturating_add(*size)
                })
                .copied()
                .ok_or(ApplyError::BadCopy)?;
            let start = usize::try_from(src.checked_sub(base).ok_or(ApplyError::BadCopy)?)
                .map_err(|_| ApplyError::BadCopy)?;
            let length = usize::try_from(len).map_err(|_| ApplyError::BadCopy)?;
            // SAFETY: the range is mapped and bounds-checked.
            unsafe { std::slice::from_raw_parts((base as *const u8).add(start), length) }.to_vec()
        };
        self.write(dst, &bytes)
    }
}

impl HostApplier {
    fn write(&mut self, addr: u64, bytes: &[u8]) -> Result<(), ApplyError> {
        let mut state = self.state.borrow_mut();
        let len = u64::try_from(bytes.len()).map_err(|_| ApplyError::BadAlignment)?;
        let Some((base, size, writable)) = state
            .ranges
            .iter()
            .find(|(base, size, _)| {
                addr >= *base && addr.saturating_add(len) <= base.saturating_add(*size)
            })
            .copied()
        else {
            state.writes_outside = state.writes_outside.saturating_add(1);
            return Err(ApplyError::OutOfRange);
        };
        if !writable {
            state.writes_outside = state.writes_outside.saturating_add(1);
            return Err(ApplyError::OutOfRange);
        }
        let start = usize::try_from(addr.checked_sub(base).ok_or(ApplyError::OutOfRange)?)
            .map_err(|_| ApplyError::OutOfRange)?;
        // SAFETY: the range is mapped, writable and bounds-checked.
        unsafe {
            std::ptr::copy_nonoverlapping(
                bytes.as_ptr(),
                (base as *mut u8).add(start),
                bytes.len(),
            );
        }
        Ok(())
    }
}

/// A per-object memory view for a fixed range (used by the digest tests).
pub struct OneMem {
    base: u64,
    bytes: Vec<u8>,
}

impl OneMem {
    /// A view over a byte vector at `base`.
    pub fn new(base: u64, bytes: Vec<u8>) -> Self {
        Self { base, bytes }
    }
}

impl MemView for OneMem {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        let start = usize::try_from(vaddr.checked_sub(self.base)?).ok()?;
        self.bytes.get(start..start.checked_add(len)?)
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        let start = usize::try_from(vaddr.checked_sub(self.base)?).ok()?;
        let rest = self.bytes.get(start..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        rest.get(..end)
    }
}

/// A file-id map used by the dedup test.
pub fn identities(mapper: &HostMapper) -> BTreeMap<u32, FileId> {
    mapper
        .state
        .borrow()
        .files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| Some((u32::try_from(index).ok()?, file.id)))
        .collect()
}
