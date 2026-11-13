// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handle table of the fake kernel: generations, rights and derived-handle revocation.
//!
//! Mirrors `kernel/src/process/handle.rs` and `kernel/src/syscall/basic.rs:111-160`
//! (`HANDLE_CLOSE`, `HANDLE_DUP`, `HANDLE_REVOKE`) and
//! `kernel/src/syscall/handle_xfer.rs:16-79` (`HANDLE_TRANSFER`).

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};

/// A fake-kernel object referenced by a handle.
#[derive(Clone, Debug)]
pub enum Object {
    /// The calling process itself (slot 0, rights `ALL`).
    SelfProcess,
    /// A child process created by `PROC_CREATE`.
    Process(usize),
    /// A memory object.
    Memory(usize),
    /// A scheme file: owning scheme name, handler file id and access rights.
    SchemeFile {
        /// Scheme name bytes without the colon.
        scheme: host::vec::Vec<u8>,
        /// Opaque handler file id.
        file: u32,
        /// Open for reading.
        read: bool,
        /// Open for writing.
        write: bool,
    },
    /// A registered scheme server.
    SchemeServer {
        /// Registered name.
        name: host::vec::Vec<u8>,
    },
    /// An event queue.
    EventQueue(usize),
    /// A timer.
    Timer(usize),
    /// A user event.
    Event(usize),
    /// A thread handle.
    Thread(usize),
    /// An interrupt source (driver calls are `ENOSYS` in the fake).
    Irq,
    /// An MMIO region (driver calls are `ENOSYS` in the fake).
    Mmio,
    /// A boot blob (driver calls are `ENOSYS` in the fake).
    BootBlob,
    /// A DMA buffer (driver calls are `ENOSYS` in the fake).
    Dma,
}

/// One handle-table entry.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Rights granted to this handle.
    pub rights: Rights,
    /// Lineage root id shared by every duplicate.
    pub root: u32,
    /// The referenced object.
    pub object: Object,
}

#[derive(Clone, Debug)]
struct Slot {
    generation: u8,
    entry: Option<Entry>,
}

/// A per-process handle table.
#[derive(Clone, Debug)]
pub struct HandleTable {
    slots: host::vec::Vec<Slot>,
    first_index: u32,
}

impl Default for HandleTable {
    fn default() -> Self {
        Self::new()
    }
}

impl HandleTable {
    /// An empty table (no self-process handle).
    pub fn new() -> Self {
        Self {
            slots: host::vec::Vec::new(),
            first_index: 0,
        }
    }

    /// The current-process table: slot 0 is `Handle::SELF_PROC` with all rights.
    pub fn for_self() -> Self {
        let mut table = Self::new();
        table.first_index = 4;
        table.slots.push(Slot {
            generation: 0,
            entry: Some(Entry {
                rights: Rights::ALL,
                root: 0,
                object: Object::SelfProcess,
            }),
        });
        for _ in 1..table.first_index {
            table.slots.push(Slot {
                generation: 0,
                entry: None,
            });
        }
        table
    }

    /// Insert an object and return its handle.
    pub fn insert(&mut self, object: Object, rights: Rights, root: u32) -> Result<Handle, Errno> {
        let entry = Entry {
            rights,
            root,
            object,
        };
        for (index, slot) in self.slots.iter_mut().enumerate() {
            if (index as u32) < self.first_index {
                continue;
            }
            if slot.entry.is_none() {
                slot.entry = Some(entry);
                return u32::try_from(index)
                    .map(|index| Handle::new(index, slot.generation))
                    .map_err(|_| Errno::EMFILE);
            }
        }
        let index = self.slots.len();
        let generation = 0u8;
        self.slots.push(Slot {
            generation,
            entry: Some(entry),
        });
        u32::try_from(index)
            .map(|index| Handle::new(index, generation))
            .map_err(|_| Errno::EMFILE)
    }

    /// Look up a handle and require `needed`.
    pub fn get(&self, handle: Handle, needed: Rights) -> Result<&Entry, Errno> {
        let entry = self.entry(handle)?;
        if !entry.rights.contains(needed) {
            return Err(Errno::EACCES);
        }
        Ok(entry)
    }

    /// Look up a valid handle without checking rights.
    pub fn entry(&self, handle: Handle) -> Result<&Entry, Errno> {
        let slot = self
            .slots
            .get(handle.index() as usize)
            .ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() {
            return Err(Errno::EBADF);
        }
        slot.entry.as_ref().ok_or(Errno::EBADF)
    }

    /// Remove a handle; returns the object and lineage root.
    pub fn close(&mut self, handle: Handle) -> Result<(Object, u32), Errno> {
        let slot = self
            .slots
            .get_mut(handle.index() as usize)
            .ok_or(Errno::EBADF)?;
        if slot.generation != handle.generation() {
            return Err(Errno::EBADF);
        }
        let entry = slot.entry.take().ok_or(Errno::EBADF)?;
        slot.generation = slot.generation.wrapping_add(1);
        Ok((entry.object, entry.root))
    }

    /// Remove rights from every entry in this table with `root`.
    pub fn revoke(&mut self, root: u32, mask: u32) {
        for slot in &mut self.slots {
            if let Some(entry) = &mut slot.entry
                && entry.root == root
            {
                entry.rights = entry.rights.intersect(Rights(entry.rights.bits() & !mask));
            }
        }
    }

    /// True when any entry in this table belongs to `root`.
    pub fn has_root(&self, root: u32) -> bool {
        self.slots
            .iter()
            .filter_map(|slot| slot.entry.as_ref())
            .any(|entry| entry.root == root)
    }

    /// Number of live handles.
    pub fn len(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.entry.is_some())
            .count()
    }

    /// True when the table has no live handles.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
