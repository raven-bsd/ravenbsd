// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `eventfd` (`11` 3.6 decision 6, `L02`).
//!
//! The 64-bit counter and the `EFD_SEMAPHORE` arithmetic live in libc; `EVENT_CREATE` gives
//! the kernel handle and `EVENT_SIGNAL` is the readiness doorbell. Reading the kernel counter
//! directly is `PG-19`: until it exists a blocking read of an empty counter is `ENOSYS`, never a
//! busy loop and never a fake zero.

use alloc::collections::BTreeMap;

use crate::seams::{Errno, EventOps, FdKind, FdTable};

/// `EFD_SEMAPHORE`.
pub const EFD_SEMAPHORE: i32 = 1;
/// `EFD_CLOEXEC`.
pub const EFD_CLOEXEC: i32 = raven_libc_abi::fcntl::O_CLOEXEC;
/// `EFD_NONBLOCK`.
pub const EFD_NONBLOCK: i32 = raven_libc_abi::fcntl::O_NONBLOCK;
/// The largest counter value (`u64::MAX - 1`).
pub const COUNTER_MAX: u64 = 0xffff_ffff_ffff_fffe;

/// One eventfd counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counter {
    /// The current value.
    pub value: u64,
    /// `EFD_SEMAPHORE` semantics.
    pub semaphore: bool,
    /// `EFD_NONBLOCK`.
    pub nonblock: bool,
}

/// The libc-side counters, keyed by descriptor.
#[derive(Clone, Debug, Default)]
pub struct EventFdTable {
    entries: BTreeMap<i32, Counter>,
}

impl EventFdTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// The counter of `fd`.
    #[must_use]
    pub fn get(&self, fd: i32) -> Option<&Counter> {
        self.entries.get(&fd)
    }

    /// Number of counters.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// `eventfd(initval, flags)`.
pub fn create<T: FdTable, E: EventOps>(
    t: &mut T,
    event: &mut E,
    state: &mut EventFdTable,
    initval: u32,
    flags: i32,
) -> Result<i32, Errno> {
    if flags & !(EFD_SEMAPHORE | EFD_CLOEXEC | EFD_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    let handle = event.create()?;
    let fd = t.install(handle, FdKind::EventFd, flags & EFD_CLOEXEC != 0)?;
    state.entries.insert(
        fd,
        Counter {
            value: u64::from(initval),
            semaphore: flags & EFD_SEMAPHORE != 0,
            nonblock: flags & EFD_NONBLOCK != 0,
        },
    );
    Ok(fd)
}

/// `eventfd_read`: read the counter as a little-endian 64-bit value.
pub fn read<T: FdTable, E: EventOps>(
    t: &T,
    event: &mut E,
    state: &mut EventFdTable,
    fd: i32,
    out: &mut [u8],
) -> Result<usize, Errno> {
    if out.len() < 8 {
        return Err(Errno::EINVAL);
    }
    if t.kind_of(fd)? != FdKind::EventFd {
        return Err(Errno::EINVAL);
    }
    let value = read_value(t, event, state, fd)?;
    let bytes = value.to_le_bytes();
    let Some(slot) = out.get_mut(..8) else {
        return Err(Errno::EINVAL);
    };
    slot.copy_from_slice(&bytes);
    Ok(8)
}

/// `eventfd_read` in its value-returning form.
pub fn read_value<T: FdTable, E: EventOps>(
    t: &T,
    event: &mut E,
    state: &mut EventFdTable,
    fd: i32,
) -> Result<u64, Errno> {
    if t.kind_of(fd)? != FdKind::EventFd {
        return Err(Errno::EINVAL);
    }
    let Some(counter) = state.entries.get(&fd).copied() else {
        return Err(Errno::EINVAL);
    };
    if counter.value == 0 {
        if counter.nonblock {
            return Err(Errno::EAGAIN);
        }
        let handle = t.handle_of(fd)?;
        event.wait_readable(handle)?;
        let Some(after) = state.entries.get(&fd).copied() else {
            return Err(Errno::EINVAL);
        };
        if after.value == 0 {
            return Err(Errno::EAGAIN);
        }
    }
    let Some(entry) = state.entries.get_mut(&fd) else {
        return Err(Errno::EINVAL);
    };
    let value = entry.value;
    if entry.semaphore {
        entry.value = entry.value.saturating_sub(1);
        Ok(1)
    } else {
        entry.value = 0;
        Ok(value)
    }
}

/// `eventfd_write`: add `value` to the counter and ring the readiness doorbell.
pub fn write<T: FdTable, E: EventOps>(
    t: &T,
    event: &mut E,
    state: &mut EventFdTable,
    fd: i32,
    value: u64,
) -> Result<(), Errno> {
    if value == u64::MAX {
        return Err(Errno::EINVAL);
    }
    if t.kind_of(fd)? != FdKind::EventFd {
        return Err(Errno::EINVAL);
    }
    let Some(counter) = state.entries.get(&fd).copied() else {
        return Err(Errno::EINVAL);
    };
    if value > COUNTER_MAX.saturating_sub(counter.value) {
        // A blocking wait for the counter to drain is not expressible before `PG-19`; the
        // honest answer for both modes today is `EAGAIN`.
        return Err(Errno::EAGAIN);
    }
    let Some(entry) = state.entries.get_mut(&fd) else {
        return Err(Errno::EINVAL);
    };
    entry.value = entry.value.saturating_add(value);
    let handle = t.handle_of(fd)?;
    event.signal(handle)
}

#[cfg(test)]
#[path = "eventfd_ut.rs"]
mod tests;
