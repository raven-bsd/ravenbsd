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

/// Install the well-known handles 0, 1 and 2 in a fresh process.
///
/// The first process (the spawner) gets the `EXEC` right on the initfs; child processes only get
/// `READ|MAP`, so they can inspect the archive but cannot map executable file pages.
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
    Ok(())
}
