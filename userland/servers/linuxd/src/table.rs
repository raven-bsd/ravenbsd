// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-process Linux state and the regions the server can copy out of.
//!
//! A [`LinuxProcess`] is the dispatcher's `LinuxContext`. It is owned state only: the trap loop
//! pre-fetches user-memory arguments into [`LinuxProcess::user_buffer`] before dispatch, because
//! reading a Linux process's memory needs the memory-object handles the loader keeps and the
//! dispatcher must stay a pure function table.

use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_linux_dispatch::LinuxContext;

/// Largest single user buffer the trap loop copies in.
pub const USER_BUFFER_MAX: usize = 64 * 1024;

/// One mapping of a Linux process, as the server knows it.
///
/// `backing` is the memory object the loader mapped into the child; `backing_offset` is the byte
/// offset of `start` inside it. `Handle::NONE` marks a zero-filled kernel mapping with no object of
/// its own (the trap loop cannot copy out of it and reports `EFAULT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Region {
    /// First Linux virtual address of the mapping.
    pub start: u64,
    /// Length in bytes.
    pub len: u64,
    /// Memory object backing the mapping, or [`Handle::NONE`].
    pub backing: Handle,
    /// Byte offset of `start` inside `backing`.
    pub backing_offset: u64,
}

impl Region {
    /// Whether `address..address + len` is wholly inside this region.
    pub fn contains(&self, address: u64, len: u64) -> bool {
        let end = address.checked_add(len);
        let region_end = self.start.checked_add(self.len);
        match (end, region_end) {
            (Some(end), Some(region_end)) => address >= self.start && end <= region_end,
            _ => false,
        }
    }
}

/// One Linux process the server is responsible for.
#[derive(Debug)]
pub struct LinuxProcess {
    /// Linux pid shown to the embedded program.
    pub pid: i32,
    /// Exit status once `exit_group` was served.
    pub exit: Option<i32>,
    /// Current program break.
    pub brk: u64,
    /// Initial program break (the first byte of the pre-mapped break arena).
    pub brk_start: u64,
    /// First address past the pre-mapped break arena.
    pub brk_limit: u64,
    /// Next address handed out by anonymous `mmap`.
    pub mmap_next: u64,
    /// First address past the pre-mapped anonymous arena.
    pub mmap_limit: u64,
    /// Every mapping the process owns.
    pub regions: Vec<Region>,
    /// Bytes `write` produced, drained by the server to its log sink.
    pub output: Vec<u8>,
    /// User-memory bytes the trap loop copied in for the current call.
    pub user_buffer: Vec<u8>,
    /// Bytes a handler wants the trap loop to copy into the process, with their address.
    pub pending_write: Option<(u64, Vec<u8>)>,
}

impl LinuxProcess {
    /// Create the context for a freshly loaded process.
    pub fn new(pid: i32, brk: u64, brk_limit: u64, mmap_next: u64, mmap_limit: u64) -> Self {
        Self {
            pid,
            exit: None,
            brk,
            brk_start: brk,
            brk_limit,
            mmap_next,
            mmap_limit,
            regions: Vec::new(),
            output: Vec::new(),
            user_buffer: Vec::new(),
            pending_write: None,
        }
    }

    /// The region holding `address..address + len`, if any.
    pub fn region_for(&self, address: u64, len: u64) -> Option<Region> {
        self.regions
            .iter()
            .find(|region| region.contains(address, len))
            .copied()
    }

    /// Drain the bytes queued by `write`.
    pub fn take_output(&mut self) -> Vec<u8> {
        core::mem::take(&mut self.output)
    }
}

impl LinuxContext for LinuxProcess {
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

#[cfg(test)]
#[path = "table_ut.rs"]
mod tests;
