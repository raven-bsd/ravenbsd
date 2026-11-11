// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel objects stored in a process handle table.

use alloc::sync::Arc;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Rights;
use raven_handles::HandleTable;

use crate::event::{EventQueue, Timer, UserEvent};
#[cfg(target_arch = "x86_64")]
use crate::ipc::{PendingRequest, SchemeFile, SchemeServer};
use crate::memory::frame;

use super::Process;
use super::thread::Thread;

/// An anonymous memory object: a refcounted list of eagerly allocated, zeroed frames.
pub struct MemObject {
    frames: Vec<u64>,
}

impl MemObject {
    /// Allocate `pages` zeroed frames. On failure every frame allocated so far is returned.
    pub fn new(pages: u64) -> Result<Arc<MemObject>, Errno> {
        let mut frames = Vec::new();
        let mut index = 0u64;
        while index < pages {
            let Some(frame) = frame::alloc_zeroed_frame() else {
                for physical in &frames {
                    let _ = frame::free_frame(frame::PhysAddr(*physical));
                }
                return Err(Errno::ENOMEM);
            };
            frames.push(frame.0);
            index = index.saturating_add(1);
        }
        Ok(Arc::new(MemObject { frames }))
    }

    /// Allocate page-rounded frames and copy `bytes` into them. Padding stays zeroed.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a boot blobs are x86-64.
    pub fn from_bytes(bytes: &[u8]) -> Result<Arc<MemObject>, Errno> {
        let pages = bytes.len().div_ceil(frame::PAGE_SIZE as usize);
        let pages = u64::try_from(pages.max(1)).map_err(|_| Errno::EINVAL)?;
        let object = MemObject::new(pages)?;
        let mut index = 0u64;
        let mut copied = 0usize;
        while copied < bytes.len() {
            let Some(physical) = object.frame_at(index) else {
                return Err(Errno::ENOMEM);
            };
            let destination = frame::phys_to_virt(frame::PhysAddr(physical));
            if destination.is_null() {
                return Err(Errno::ENOMEM);
            }
            let chunk = core::cmp::min(
                frame::PAGE_SIZE as usize,
                bytes.len().saturating_sub(copied),
            );
            // SAFETY: `destination` is a fresh frame mapped writable through the HHDM and `chunk`
            // stays within both the frame and the remaining source bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(bytes.as_ptr().add(copied), destination, chunk);
            }
            copied = copied.saturating_add(chunk);
            index = index.saturating_add(1);
        }
        Ok(object)
    }

    /// Length in bytes.
    pub fn len(&self) -> u64 {
        u64::try_from(self.frames.len())
            .unwrap_or(u64::MAX)
            .saturating_mul(frame::PAGE_SIZE)
    }

    /// Physical frame for page `index` inside the object.
    pub fn frame_at(&self, index: u64) -> Option<u64> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.frames.get(index).copied())
    }
}

impl Drop for MemObject {
    fn drop(&mut self) {
        for physical in &self.frames {
            let _ = frame::free_frame(frame::PhysAddr(*physical));
        }
    }
}

/// A device-memory window a driver can map (`MMIO_CREATE`). Immutable after creation.
#[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a implements the device syscalls on x86-64.
pub struct MmioRegion {
    /// Physical start address (page aligned).
    phys: u64,
    /// Length in bytes (page multiple, non-zero).
    len: u64,
    /// Creation attributes; only 0 is defined today.
    attrs: u32,
}

impl MmioRegion {
    /// Construct a region. Callers must have validated the range.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a is x86-64.
    pub const fn new(phys: u64, len: u64, attrs: u32) -> Self {
        Self { phys, len, attrs }
    }

    /// Physical start address.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a is x86-64.
    pub const fn phys(&self) -> u64 {
        self.phys
    }

    /// Length in bytes.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a is x86-64.
    pub const fn len(&self) -> u64 {
        self.len
    }

    /// Creation attributes.
    #[cfg_attr(target_arch = "aarch64", allow(dead_code))] // S21a is x86-64.
    #[allow(dead_code)] // Reported to drivers by later tickets.
    pub const fn attrs(&self) -> u32 {
        self.attrs
    }
}

/// Objects a handle can refer to in S08.
#[allow(dead_code)] // Object access is used by later MAP/THREAD tickets; PID 1 only closes/dups now.
#[derive(Clone)]
pub enum Object {
    /// The process itself (well-known handle 0).
    Process(Arc<Process>),
    /// The boot initfs memory object (well-known handle 1).
    MemObjectInitfs,
    /// An anonymous memory object created by `MEMOBJ_CREATE`.
    Memory(Arc<MemObject>),
    /// Root namespace placeholder (well-known handle 2).
    Namespace,
    /// A thread object (created by later tickets).
    Thread(Arc<Thread>),
    /// A wait-for-many event queue.
    EventQueue(Arc<EventQueue>),
    /// A monotonic timer event source.
    Timer(Arc<Timer>),
    /// A user-space doorbell event source.
    UserEvent(Arc<UserEvent>),
    /// The root device capability held by the first process (`Handle::ROOT_DEVICE`).
    RootDevice,
    /// A device-memory window created by `MMIO_CREATE`.
    #[cfg(target_arch = "x86_64")]
    MmioRegion(Arc<MmioRegion>),
    /// A read-only boot-time blob (`BOOT_BLOB`).
    #[cfg(target_arch = "x86_64")]
    BootBlob(Arc<MemObject>),
    /// An open scheme file.
    #[cfg(target_arch = "x86_64")]
    SchemeFile(Arc<SchemeFile>),
    /// A registered scheme server capability.
    #[cfg(target_arch = "x86_64")]
    SchemeServer(Arc<SchemeServer>),
    /// A server-side request handle returned by `RECV`.
    #[cfg(target_arch = "x86_64")]
    SchemeRequest(Arc<PendingRequest>),
}

/// The kernel's handle table type.
pub type Table = HandleTable<Object>;

/// Install the well-known handles 0, 1 and 2 (and 3 for the first process) in a fresh process.
///
/// The first process (the spawner) gets the `EXEC` right on the initfs and the `RootDevice`
/// capability at `Handle::ROOT_DEVICE`; child processes only get `READ|MAP` on the initfs, so they
/// can inspect the archive but cannot map executable file pages, and no root capability.
pub fn install_initial(
    table: &mut Table,
    process: &Arc<Process>,
    initfs_exec: bool,
) -> Result<(), Errno> {
    table.insert(Object::Process(Arc::clone(process)), Rights::ALL)?;
    let mut initfs = Rights::READ.union(Rights::MAP);
    if initfs_exec {
        initfs = initfs.union(Rights::EXEC);
    }
    table.insert(Object::MemObjectInitfs, initfs)?;
    table.insert(Object::Namespace, Rights::READ)?;
    if initfs_exec {
        let root = Rights::ADMIN.union(Rights::DUP).union(Rights::TRANSFER);
        table.insert(Object::RootDevice, root)?;
    }
    Ok(())
}
