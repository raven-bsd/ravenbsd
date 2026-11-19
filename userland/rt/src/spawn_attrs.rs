// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Spawn attributes layered over the shared `raven-spawn` loader.
//!
//! `raven-spawn` on this branch starts a child with an empty descriptor table and no signal
//! attributes.  The kernel spawn path already carries an fd map and three 1024-bit signal
//! sets, so the runtime supplies the missing loader step here without copying the loader:
//!
//! * each source handle is transferred into the child with `HANDLE_TRANSFER`;
//! * the child handle values are packed into an `RFDM` block (the format `libc` already decodes)
//!   and the block is mapped below the initial stack;
//! * the three signal sets are mapped next to the block;
//! * the `AT_RAVEN_*` auxiliary-vector pairs are inserted before `AT_NULL`, which shifts `sp0`
//!   down by the inserted pairs.  String pointers stay valid because only the word array moves.
//!
//! The auxiliary-vector keys are the established `AT_RAVEN_FDMAP`/`_N` pair plus the P2n signal
//! keys.  A libc `start.rs` consumer for the signal keys is outside this ticket; the fixture
//! reads them directly, and the report records the remaining integration point.

use alloc_crate::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_abi::types::{map, prot};
#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
use raven_spawn::SpawnError;
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_spawn::SpawnOptions;
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
use raven_spawn::Sys;

/// Auxiliary-vector key for the `RFDM` block.
pub const AT_RAVEN_FDMAP: usize = 0x7003;
/// Auxiliary-vector key for the `RFDM` entry count.
pub const AT_RAVEN_FDMAP_N: usize = 0x7004;
/// Auxiliary-vector key for the child working-directory string (K165).
pub const AT_RAVEN_CWD: usize = 0x7005;
/// Auxiliary-vector key for the ABI version when an attribute page is present.
pub const AT_RAVEN_ABI: usize = 0x7006;
/// Auxiliary-vector key for the mapped initial signal mask.
pub const AT_RAVEN_SIGMASK: usize = 0x7008;
/// Auxiliary-vector key for the mapped default-disposition set.
pub const AT_RAVEN_SIGDEFAULT: usize = 0x7009;
/// Auxiliary-vector key for the mapped ignored-signal set.
pub const AT_RAVEN_SIGIGNORE: usize = 0x700A;

/// One page of initial attribute memory.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
const PAGE: usize = 4096;
/// `RFDM` magic.
const FDMAP_MAGIC: [u8; 4] = *b"RFDM";
/// Current `RFDM` version.
const FDMAP_VERSION: u16 = 1;
/// One `RFDM` entry.
const FDMAP_ENTRY_LEN: usize = 18;

/// The attributes a spawn request asks the runtime to install before the first thread.
#[derive(Clone, Copy, Debug)]
pub struct SpawnAttributes<'a> {
    /// `(child fd, parent handle)` pairs. The parent handle is valid in the caller's table.
    pub fdmap: &'a [(i32, u32)],
    /// Initial blocked-signal set.
    pub sigmask: &'a [u8; 128],
    /// Signals reset to `SIG_DFL`.
    pub sigdefault: &'a [u8; 128],
    /// Signals ignored by the parent.
    pub ignored: &'a [u8; 128],
}

/// A syscall sink that can transfer a handle into a child.
pub trait HandleTransfer {
    /// Transfer `source` into `destination` with all usable rights.
    fn transfer_to(&mut self, destination: Handle, source: Handle) -> Result<Handle, Errno>;
}

impl HandleTransfer for raven_spawn::RtSys {
    fn transfer_to(&mut self, destination: Handle, source: Handle) -> Result<Handle, Errno> {
        crate::sys::handle_transfer(destination, source, Rights::ALL)
    }
}

/// Spawn an initfs image with `attributes` using the real syscalls.
///
/// # Errors
/// [`SpawnError`] from the loader, a rejected handle transfer or a mapping failure.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub fn spawn_with_attributes(
    initfs: &raven_initfs::Initfs<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
    attributes: &SpawnAttributes<'_>,
) -> Result<Handle, SpawnError> {
    let mut sys = raven_spawn::RtSys;
    spawn_with_attributes_with(initfs, path, options, attributes, &mut sys)
}

/// Spawn an initfs image with `attributes` and a caller-provided syscall sink.
///
/// # Errors
/// [`SpawnError`] from the loader, a rejected handle transfer or a mapping failure.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
pub fn spawn_with_attributes_with<S: Sys + HandleTransfer>(
    initfs: &raven_initfs::Initfs<'_>,
    path: &str,
    options: &SpawnOptions<'_>,
    attributes: &SpawnAttributes<'_>,
    sys: &mut S,
) -> Result<Handle, SpawnError> {
    let mut attributed = AttributeSys {
        inner: sys,
        attributes,
        command: crate::spawn::command_name(path, options),
        child: None,
        stack_write: None,
        stack_map: None,
        deferred_unmap: None,
    };
    raven_spawn::spawn_with(initfs, path, options, &mut attributed)
}

/// Encode the `RFDM` descriptor block.
///
/// The entry kind is `Unknown` (255), which `libc` treats as transferable and probes on first use.
#[must_use]
pub fn encode_fdmap(entries: &[(i32, u32)]) -> Vec<u8> {
    let count = u16::try_from(entries.len()).unwrap_or(u16::MAX);
    let mut bytes = Vec::with_capacity(
        FDMAP_ENTRY_LEN
            .saturating_mul(entries.len())
            .saturating_add(8),
    );
    bytes.extend_from_slice(&FDMAP_MAGIC);
    bytes.extend_from_slice(&FDMAP_VERSION.to_le_bytes());
    bytes.extend_from_slice(&count.to_le_bytes());
    for (fd, handle) in entries.iter().take(usize::from(count)) {
        bytes.extend_from_slice(&fd.to_le_bytes());
        bytes.extend_from_slice(&handle.to_le_bytes());
        bytes.push(255);
        bytes.push(0);
        bytes.extend_from_slice(&2u32.to_le_bytes());
        bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    }
    bytes
}

/// One prepared attribute page: bytes and the offsets of each block.
#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
struct AttributePage {
    bytes: Vec<u8>,
    fdmap: Option<(usize, usize)>,
    sigmask: Option<usize>,
    sigdefault: Option<usize>,
    ignored: Option<usize>,
}

#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
impl AttributePage {
    fn build(attributes: &SpawnAttributes<'_>, fdmap: &[(i32, u32)]) -> Self {
        let mut bytes = Vec::new();
        let fdmap = if fdmap.is_empty() {
            None
        } else {
            let offset = bytes.len();
            bytes.extend_from_slice(&encode_fdmap(fdmap));
            Some((offset, fdmap.len()))
        };
        let signals = non_zero(attributes.sigmask)
            || non_zero(attributes.sigdefault)
            || non_zero(attributes.ignored);
        let mut page = Self {
            bytes,
            fdmap,
            sigmask: None,
            sigdefault: None,
            ignored: None,
        };
        if signals {
            page.push_signal(attributes.sigmask);
            page.sigmask = Some(page.bytes.len().saturating_sub(128));
            page.push_signal(attributes.sigdefault);
            page.sigdefault = Some(page.bytes.len().saturating_sub(128));
            page.push_signal(attributes.ignored);
            page.ignored = Some(page.bytes.len().saturating_sub(128));
        }
        page
    }

    fn push_signal(&mut self, set: &[u8; 128]) {
        while !self.bytes.len().is_multiple_of(8) {
            self.bytes.push(0);
        }
        self.bytes.extend_from_slice(set);
    }

    fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    fn auxv(&self, address: usize) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        if let Some((offset, count)) = self.fdmap {
            pairs.push((AT_RAVEN_FDMAP, address.saturating_add(offset)));
            pairs.push((AT_RAVEN_FDMAP_N, count));
        }
        if let Some(offset) = self.sigmask {
            pairs.push((AT_RAVEN_SIGMASK, address.saturating_add(offset)));
        }
        if let Some(offset) = self.sigdefault {
            pairs.push((AT_RAVEN_SIGDEFAULT, address.saturating_add(offset)));
        }
        if let Some(offset) = self.ignored {
            pairs.push((AT_RAVEN_SIGIGNORE, address.saturating_add(offset)));
        }
        if !pairs.is_empty() {
            pairs.push((AT_RAVEN_ABI, raven_abi::ABI_VERSION as usize));
        }
        pairs
    }
}

#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
fn non_zero(set: &[u8; 128]) -> bool {
    set.iter().any(|byte| *byte != 0)
}

/// Insert `pairs` before the `AT_NULL` terminator in a C8 stack image.
///
/// Returns the rewritten block and the number of bytes the caller must subtract from `sp0`.
/// Only the count/pointer/auxv array moves; every string keeps its address.
#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
fn augment_stack(image: &[u8], pairs: &[(usize, usize)]) -> Result<(Vec<u8>, usize), SpawnError> {
    let word = core::mem::size_of::<usize>();
    let argc = read_word(image, 0).ok_or(SpawnError::TooLarge)?;
    if argc > 4096 {
        return Err(SpawnError::TooLarge);
    }
    let argv_end = word
        .checked_add(
            argc.checked_add(1)
                .and_then(|count| count.checked_mul(word))
                .ok_or(SpawnError::TooLarge)?,
        )
        .ok_or(SpawnError::TooLarge)?;
    let mut cursor = argv_end;
    let mut env_count = 0usize;
    loop {
        let value = read_word(image, cursor).ok_or(SpawnError::TooLarge)?;
        cursor = cursor.checked_add(word).ok_or(SpawnError::TooLarge)?;
        if value == 0 {
            break;
        }
        env_count = env_count.checked_add(1).ok_or(SpawnError::TooLarge)?;
        if env_count > 4096 {
            return Err(SpawnError::TooLarge);
        }
    }
    let mut at_null = cursor;
    let mut auxv_count = 0usize;
    loop {
        let key = read_word(image, at_null).ok_or(SpawnError::TooLarge)?;
        let value = read_word(
            image,
            at_null.checked_add(word).ok_or(SpawnError::TooLarge)?,
        )
        .ok_or(SpawnError::TooLarge)?;
        if key == 0 && value == 0 {
            break;
        }
        at_null = at_null
            .checked_add(word.checked_mul(2).ok_or(SpawnError::TooLarge)?)
            .ok_or(SpawnError::TooLarge)?;
        auxv_count = auxv_count.checked_add(1).ok_or(SpawnError::TooLarge)?;
        if auxv_count > 64 {
            return Err(SpawnError::TooLarge);
        }
    }
    let extra = pairs
        .len()
        .checked_mul(word.checked_mul(2).ok_or(SpawnError::TooLarge)?)
        .ok_or(SpawnError::TooLarge)?;
    let mut rewritten = Vec::with_capacity(
        at_null
            .checked_add(extra)
            .and_then(|length| length.checked_add(word * 2))
            .ok_or(SpawnError::TooLarge)?,
    );
    rewritten.extend_from_slice(image.get(..at_null).ok_or(SpawnError::TooLarge)?);
    for (key, value) in pairs {
        rewritten.extend_from_slice(&key.to_le_bytes());
        rewritten.extend_from_slice(&value.to_le_bytes());
    }
    rewritten.extend_from_slice(&[0u8; 16]);
    Ok((rewritten, extra))
}

#[cfg(any(target_os = "none", target_os = "ravenbsd", test))]
fn read_word(image: &[u8], offset: usize) -> Option<usize> {
    let word = core::mem::size_of::<usize>();
    let bytes = image.get(offset..offset.checked_add(word)?)?;
    let mut value = [0u8; core::mem::size_of::<usize>()];
    value.copy_from_slice(bytes);
    Some(usize::from_le_bytes(value))
}

/// A `Sys` proxy that observes the stack layout and installs the attribute page.
#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
struct AttributeSys<'a, S: Sys + HandleTransfer> {
    inner: &'a mut S,
    attributes: &'a SpawnAttributes<'a>,
    command: &'a str,
    child: Option<Handle>,
    stack_write: Option<(usize, Vec<u8>)>,
    stack_map: Option<(usize, usize)>,
    deferred_unmap: Option<(usize, usize)>,
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl<S: Sys + HandleTransfer> AttributeSys<'_, S> {
    fn prepare_page(
        &mut self,
        child: Handle,
        stack_base: usize,
        fdmap: &[(i32, u32)],
    ) -> Result<Vec<(usize, usize)>, SpawnError> {
        let page = AttributePage::build(self.attributes, fdmap);
        if page.is_empty() {
            return Ok(Vec::new());
        }
        let len = page
            .bytes
            .len()
            .checked_add(PAGE - 1)
            .map(|value| value / PAGE * PAGE)
            .ok_or(SpawnError::TooLarge)?;
        let object = self.inner.memobj_create(len).map_err(SpawnError::Sys)?;
        let scratch_flags = map::pack(prot::READ | prot::WRITE, map::SHARED);
        let scratch = match self
            .inner
            .map(Handle::SELF_PROC, object, 0, 0, len, scratch_flags)
        {
            Ok(address) => address,
            Err(error) => {
                let _ = self.inner.handle_close(object);
                return Err(SpawnError::Sys(error));
            }
        };
        self.inner.write_parent_mem(scratch, &page.bytes);
        if let Err(error) = self.inner.unmap(Handle::SELF_PROC, scratch, len) {
            let _ = self.inner.handle_close(object);
            return Err(SpawnError::Sys(error));
        }
        let address = stack_base.checked_sub(len).ok_or(SpawnError::TooLarge)?;
        let child_flags = map::pack(prot::READ, map::SHARED | map::FIXED);
        if let Err(error) = self.inner.map(child, object, 0, address, len, child_flags) {
            let _ = self.inner.handle_close(object);
            return Err(SpawnError::Sys(error));
        }
        let _ = self.inner.handle_close(object);
        Ok(page.auxv(address))
    }

    fn unmap_deferred(&mut self) -> Result<(), SpawnError> {
        if let Some((address, len)) = self.deferred_unmap.take() {
            self.inner
                .unmap(Handle::SELF_PROC, address, len)
                .map_err(SpawnError::Sys)?;
        }
        Ok(())
    }
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl<S: Sys + HandleTransfer> Drop for AttributeSys<'_, S> {
    fn drop(&mut self) {
        if let Some((address, len)) = self.deferred_unmap.take() {
            let _ = self.inner.unmap(Handle::SELF_PROC, address, len);
        }
    }
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
impl<S: Sys + HandleTransfer> Sys for AttributeSys<'_, S> {
    fn proc_create(&mut self) -> Result<Handle, Errno> {
        let child = self.inner.proc_create()?;
        self.child = Some(child);
        Ok(child)
    }

    fn memobj_create(&mut self, len: usize) -> Result<Handle, Errno> {
        self.inner.memobj_create(len)
    }

    fn map(
        &mut self,
        proc: Handle,
        memobj: Handle,
        offset: usize,
        addr: usize,
        len: usize,
        flags: usize,
    ) -> Result<usize, Errno> {
        let address = self.inner.map(proc, memobj, offset, addr, len, flags)?;
        if self.child == Some(proc) && self.stack_write.is_some() && self.stack_map.is_none() {
            self.stack_map = Some((address, len));
        }
        Ok(address)
    }

    fn unmap(&mut self, proc: Handle, addr: usize, len: usize) -> Result<(), Errno> {
        if proc == Handle::SELF_PROC && self.deferred_unmap.is_none() && self.stack_map.is_some() {
            self.deferred_unmap = Some((addr, len));
            return Ok(());
        }
        self.inner.unmap(proc, addr, len)
    }

    fn protect(
        &mut self,
        proc: Handle,
        addr: usize,
        len: usize,
        protection: u8,
    ) -> Result<(), Errno> {
        self.inner.protect(proc, addr, len, protection)
    }

    fn thread_create(
        &mut self,
        proc: Handle,
        entry: usize,
        stack_top: usize,
        arg: usize,
    ) -> Result<Handle, Errno> {
        let Some(child) = self.child else {
            return self.inner.thread_create(proc, entry, stack_top, arg);
        };
        let Some((write_address, image)) = self.stack_write.take() else {
            return Err(Errno::EIO);
        };
        let Some((stack_base, _)) = self.stack_map else {
            let _ = self.unmap_deferred();
            return Err(Errno::EIO);
        };

        let result = (|| -> Result<usize, SpawnError> {
            crate::proc::set_command(child, self.command).map_err(SpawnError::Sys)?;
            let mut transferred = Vec::with_capacity(self.attributes.fdmap.len());
            for (fd, source) in self.attributes.fdmap {
                let handle = self
                    .inner
                    .transfer_to(child, Handle(*source))
                    .map_err(SpawnError::Sys)?;
                transferred.push((*fd, handle.0));
            }
            let pairs = self.prepare_page(child, stack_base, &transferred)?;
            let (block, shift) = augment_stack(&image, &pairs)?;
            let start = write_address
                .checked_sub(shift)
                .ok_or(SpawnError::TooLarge)?;
            self.inner.write_parent_mem(start, &block);
            Ok(shift)
        })();
        let unmap = self.unmap_deferred();
        match (result, unmap) {
            (Ok(shift), Ok(())) => {
                let new_stack = stack_top.checked_sub(shift).ok_or(Errno::EINVAL)?;
                self.inner.thread_create(proc, entry, new_stack, new_stack)
            }
            (Err(error), _) => Err(to_errno(error)),
            (Ok(_), Err(error)) => Err(to_errno(error)),
        }
    }

    fn exec_cred(&mut self, proc: Handle, file: Handle, nosuid: bool) -> Result<(), Errno> {
        self.inner.exec_cred(proc, file, nosuid)
    }

    fn handle_close(&mut self, handle: Handle) -> Result<(), Errno> {
        self.inner.handle_close(handle)
    }

    fn write_parent_mem(&mut self, addr: usize, bytes: &[u8]) {
        if self.child.is_some() && self.stack_write.is_none() {
            self.stack_write = Some((addr, bytes.to_vec()));
        }
        self.inner.write_parent_mem(addr, bytes);
    }

    fn copy_mapping(&mut self, destination: usize, source: usize, len: usize) {
        self.inner.copy_mapping(destination, source, len);
    }
}

#[cfg(any(target_os = "none", target_os = "ravenbsd"))]
fn to_errno(error: SpawnError) -> Errno {
    match error {
        SpawnError::Sys(error) => error,
        SpawnError::TooLarge => Errno::E2BIG,
        SpawnError::NotFound | SpawnError::FileNotFound => Errno::ENOENT,
        SpawnError::BadElf(_) | SpawnError::HasInterpreter => Errno::ENOEXEC,
        SpawnError::BadCwd => Errno::EINVAL,
    }
}

#[cfg(test)]
#[path = "spawn_attrs_ut.rs"]
mod tests;
