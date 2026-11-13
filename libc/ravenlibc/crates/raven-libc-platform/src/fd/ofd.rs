// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The OFD slab: one refcounted open-file description per kernel handle.

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::backend::SysBackend;
use crate::lock::Lock;
use crate::platform::Platform;

/// What an OFD refers to, recorded at open time and only probed for adopted handles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OfdKind {
    /// A regular file.
    File,
    /// A directory.
    Dir,
    /// A terminal.
    Tty,
    /// A network socket.
    Socket,
    /// A pipe or FIFO.
    Pipe,
    /// An event queue (`epoll` instance).
    Epoll,
    /// A timer object.
    Timer,
    /// An event counter.
    EventFd,
    /// A process handle.
    ProcessFd,
    /// Not known yet; a probe is required.
    #[default]
    Unknown,
}

/// One open-file description.
#[derive(Debug)]
pub struct Ofd {
    /// The kernel handle this OFD owns.
    pub handle: Handle,
    /// Kind recorded at open time.
    pub kind: OfdKind,
    /// Access mode and the mutable status bits (Linux `F_GETFL` values).
    pub status: AtomicU32,
    /// Number of fds (and in-flight readers) referencing this OFD.
    pub refs: AtomicU32,
    /// Absolute path used at open, for `fchdir`, `ttyname` and `*at`.
    pub path: Option<Box<[u8]>>,
    /// Whether the directory cursor can be rewound (needs PG-22).
    pub dir_cursor_ok: bool,
    /// Table epoch; a reused fd cannot deliver a stale epoll event.
    pub epoch: u32,
}

impl Ofd {
    /// Build an OFD with a fresh epoch.
    pub fn new(
        handle: Handle,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
        epoch: u32,
    ) -> Self {
        Self {
            handle,
            kind,
            status: AtomicU32::new(status),
            refs: AtomicU32::new(1),
            path,
            dir_cursor_ok: false,
            epoch,
        }
    }

    /// Whether the access mode includes read.
    pub fn readable(&self) -> bool {
        self.status.load(Ordering::Relaxed) & 0o3 != 0o1
    }

    /// Whether the access mode includes write.
    pub fn writable(&self) -> bool {
        self.status.load(Ordering::Relaxed) & 0o3 != 0
    }
}

/// The OFD arena; slot zero is permanently free so `ofd: 0` means "no OFD".
pub struct OfdArena {
    slots: Vec<Option<Ofd>>,
    free: Vec<u32>,
    next_epoch: u32,
}

impl Default for OfdArena {
    fn default() -> Self {
        Self::new()
    }
}

impl OfdArena {
    /// An arena with the reserved slot zero.
    pub fn new() -> Self {
        Self {
            slots: alloc::vec![None],
            free: Vec::new(),
            next_epoch: 1,
        }
    }

    /// Insert an OFD and return its slot index.
    pub fn insert(
        &mut self,
        handle: Handle,
        kind: OfdKind,
        status: u32,
        path: Option<Box<[u8]>>,
    ) -> u32 {
        let epoch = self.next_epoch;
        self.next_epoch = self.next_epoch.wrapping_add(1).max(1);
        let ofd = Ofd::new(handle, kind, status, path, epoch);
        if let Some(index) = self.free.pop()
            && let Some(slot) = self.slots.get_mut(index as usize)
        {
            *slot = Some(ofd);
            return index;
        }
        let index = self.slots.len() as u32;
        self.slots.push(Some(ofd));
        index
    }

    /// Borrow an OFD.
    pub fn get(&self, index: u32) -> Option<&Ofd> {
        self.slots.get(index as usize)?.as_ref()
    }

    /// Mutably borrow an OFD.
    pub fn get_mut(&mut self, index: u32) -> Option<&mut Ofd> {
        self.slots.get_mut(index as usize)?.as_mut()
    }

    /// Increment the reference count and return the new value.
    pub fn add_ref(&mut self, index: u32) -> Result<u32, Errno> {
        let ofd = self.get_mut(index).ok_or(Errno::EBADF)?;
        let refs = ofd.refs.load(Ordering::Relaxed);
        if refs == 0 {
            return Err(Errno::EBADF);
        }
        let updated = refs.saturating_add(1);
        ofd.refs.store(updated, Ordering::Relaxed);
        Ok(updated)
    }

    /// Decrement the reference count; `true` when it reached zero and the handle must be closed.
    pub fn drop_ref(&mut self, index: u32) -> bool {
        let Some(ofd) = self.get_mut(index) else {
            return false;
        };
        let refs = ofd.refs.load(Ordering::Relaxed).saturating_sub(1);
        ofd.refs.store(refs, Ordering::Relaxed);
        refs == 0
    }

    /// The current reference count.
    pub fn refs(&self, index: u32) -> u32 {
        self.get(index)
            .map_or(0, |ofd| ofd.refs.load(Ordering::Relaxed))
    }

    /// Remove a dead OFD and push its slot on the free list.
    pub fn release(&mut self, index: u32) -> Option<Ofd> {
        let slot = self.slots.get_mut(index as usize)?;
        let ofd = slot.take()?;
        self.free.push(index);
        Some(ofd)
    }

    /// Number of live OFDs.
    pub fn live(&self) -> usize {
        self.slots.iter().filter(|slot| slot.is_some()).count()
    }

    /// Sum of all reference counts (must equal the number of fds plus in-flight readers).
    pub fn total_refs(&self) -> u32 {
        self.slots.iter().flatten().fold(0u32, |sum, ofd| {
            sum.saturating_add(ofd.refs.load(Ordering::Relaxed))
        })
    }

    /// Every live `(index, OFD)`, ascending.
    pub fn entries(&self) -> Vec<(u32, Ofd)> {
        let mut out = Vec::new();
        let mut index = 0u32;
        while (index as usize) < self.slots.len() {
            if let Some(ofd) = self.slots.get(index as usize).and_then(Option::as_ref) {
                out.push((index, ofd.clone()));
            }
            index = index.saturating_add(1);
        }
        out
    }
}

/// A counted reference to an OFD.
///
/// The reference keeps the kernel handle alive even after another thread closes the fd: the
/// reader continues on this OFD and the `CLOSE` happens when the last reference is dropped
/// (Linux behaviour, doc 11 section 3.5).
pub struct OfdRef<'a, B: SysBackend, L: Lock> {
    platform: &'a Platform<B, L>,
    index: u32,
}

impl<'a, B: SysBackend, L: Lock> OfdRef<'a, B, L> {
    /// Create a new counted reference; the caller already incremented the count.
    pub fn new(platform: &'a Platform<B, L>, index: u32) -> Self {
        Self { platform, index }
    }

    /// The arena slot.
    pub const fn index(&self) -> u32 {
        self.index
    }

    fn snapshot(&self) -> Option<Ofd> {
        self.platform.ofd_snapshot(self.index)
    }

    /// The kernel handle.
    pub fn handle(&self) -> Handle {
        self.snapshot().map_or(Handle::NONE, |ofd| ofd.handle)
    }

    /// The OFD kind.
    pub fn kind(&self) -> OfdKind {
        self.snapshot().map_or(OfdKind::Unknown, |ofd| ofd.kind)
    }

    /// The OFD status (`F_GETFL`).
    pub fn status(&self) -> u32 {
        self.snapshot().map_or(0, |ofd| {
            ofd.status.load(core::sync::atomic::Ordering::Relaxed)
        })
    }

    /// The remembered absolute path.
    pub fn path(&self) -> Option<Box<[u8]>> {
        self.snapshot().and_then(|ofd| ofd.path)
    }

    /// Change the cached status and return the old value.
    pub fn set_status(&self, status: u32) -> u32 {
        self.platform.set_ofd_status(self.index, status)
    }

    /// Change the remembered kind (a successful probe caches it).
    pub fn set_kind(&self, kind: OfdKind) {
        self.platform.set_ofd_kind(self.index, kind);
    }

    /// Mark the directory cursor as rewindable.
    pub fn set_dir_cursor(&self, ok: bool) {
        self.platform.set_ofd_dir_cursor(self.index, ok);
    }
}

impl<B: SysBackend, L: Lock> Drop for OfdRef<'_, B, L> {
    fn drop(&mut self) {
        self.platform.release_ofd_ref(self.index);
    }
}

impl Clone for Ofd {
    fn clone(&self) -> Self {
        Self {
            handle: self.handle,
            kind: self.kind,
            status: AtomicU32::new(self.status.load(Ordering::Relaxed)),
            refs: AtomicU32::new(self.refs.load(Ordering::Relaxed)),
            path: self.path.clone(),
            dir_cursor_ok: self.dir_cursor_ok,
            epoch: self.epoch,
        }
    }
}
