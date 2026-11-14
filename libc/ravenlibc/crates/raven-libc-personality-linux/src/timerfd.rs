// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `timerfd` (`11` 3.6 decision 6, `L03`).
//!
//! `TIMER_CREATE` and `TIMER_SET` exist; a relative arm is a `TIMER_SET(initial_ns,
//! interval_ns)`. `TFD_TIMER_ABSTIME` and `timerfd_gettime` need `PG-17`/`PG-19` and therefore
//! return the documented `EINVAL`/`ENOSYS` today; a read of the expiration counter needs
//! `PG-19` and returns `ENOSYS`. Nothing is faked as success.

use alloc::collections::BTreeMap;

use crate::seams::{
    CLOCK_BOOTTIME, CLOCK_MONOTONIC, CLOCK_REALTIME, Errno, FdKind, FdTable, TimerOps,
};

/// `TFD_TIMER_ABSTIME`.
pub const TFD_TIMER_ABSTIME: i32 = 1;
/// `TFD_TIMER_CANCEL_ON_SET`.
pub const TFD_TIMER_CANCEL_ON_SET: i32 = 2;
/// `TFD_CLOEXEC`.
pub const TFD_CLOEXEC: i32 = raven_libc_abi::fcntl::O_CLOEXEC;
/// `TFD_NONBLOCK`.
pub const TFD_NONBLOCK: i32 = raven_libc_abi::fcntl::O_NONBLOCK;

/// One timerfd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TimerFd {
    /// `CLOCK_REALTIME`, `CLOCK_MONOTONIC` or `CLOCK_BOOTTIME`.
    pub clock: i32,
    /// The creation flags.
    pub flags: i32,
    /// Current first expiry, relative nanoseconds.
    pub initial_ns: u64,
    /// Current interval, nanoseconds.
    pub interval_ns: u64,
}

/// The libc-side timerfds, keyed by descriptor.
#[derive(Clone, Debug, Default)]
pub struct TimerFdTable {
    entries: BTreeMap<i32, TimerFd>,
}

impl TimerFdTable {
    /// An empty table.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// The timerfd of `fd`.
    #[must_use]
    pub fn get(&self, fd: i32) -> Option<&TimerFd> {
        self.entries.get(&fd)
    }

    /// Number of timerfds.
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

/// Whether `clock` is one of the three accepted clocks.
#[must_use]
pub const fn clock_supported(clock: i32) -> bool {
    clock == CLOCK_REALTIME || clock == CLOCK_MONOTONIC || clock == CLOCK_BOOTTIME
}

/// `timerfd_create(clock, flags)`.
pub fn create<T: FdTable, O: TimerOps>(
    t: &mut T,
    timer: &mut O,
    state: &mut TimerFdTable,
    clock: i32,
    flags: i32,
) -> Result<i32, Errno> {
    if !clock_supported(clock) {
        return Err(Errno::EINVAL);
    }
    if flags & !(TFD_CLOEXEC | TFD_NONBLOCK) != 0 {
        return Err(Errno::EINVAL);
    }
    let handle = timer.create(clock)?;
    let fd = t.install(handle, FdKind::TimerFd, flags & TFD_CLOEXEC != 0)?;
    state.entries.insert(
        fd,
        TimerFd {
            clock,
            flags,
            initial_ns: 0,
            interval_ns: 0,
        },
    );
    Ok(fd)
}

/// `timerfd_settime(fd, flags, initial_ns, interval_ns)`.
pub fn settime<T: FdTable, O: TimerOps>(
    t: &T,
    timer: &mut O,
    state: &mut TimerFdTable,
    fd: i32,
    flags: i32,
    initial_ns: u64,
    interval_ns: u64,
) -> Result<(), Errno> {
    if flags & !(TFD_TIMER_ABSTIME | TFD_TIMER_CANCEL_ON_SET) != 0 {
        return Err(Errno::EINVAL);
    }
    if flags & TFD_TIMER_CANCEL_ON_SET != 0 && flags & TFD_TIMER_ABSTIME == 0 {
        return Err(Errno::EINVAL);
    }
    if flags & TFD_TIMER_ABSTIME != 0 {
        // Absolute arming needs `PG-17` (absolute `TIMER_SET`) and `PG-19` (readable timer).
        return Err(Errno::EINVAL);
    }
    if t.kind_of(fd)? != FdKind::TimerFd {
        return Err(Errno::EINVAL);
    }
    let Some(entry) = state.entries.get(&fd).copied() else {
        return Err(Errno::EINVAL);
    };
    let handle = t.handle_of(fd)?;
    timer.set(handle, initial_ns, interval_ns)?;
    if let Some(slot) = state.entries.get_mut(&fd) {
        slot.initial_ns = initial_ns;
        slot.interval_ns = interval_ns;
        slot.flags = entry.flags | flags;
    }
    Ok(())
}

/// `timerfd_gettime(fd)`.
pub fn gettime<T: FdTable, O: TimerOps>(
    t: &T,
    timer: &mut O,
    state: &TimerFdTable,
    fd: i32,
) -> Result<(u64, u64), Errno> {
    if t.kind_of(fd)? != FdKind::TimerFd {
        return Err(Errno::EINVAL);
    }
    let _ = state.entries.get(&fd);
    let handle = t.handle_of(fd)?;
    // `TIMER_GET` is `PG-17`/`PG-19`; the trait default is `ENOSYS`.
    timer.get(handle)
}

/// `read(timerfd, ...)`: the expiration counter is readable after `PG-19`.
pub fn read<T: FdTable, O: TimerOps>(
    t: &T,
    timer: &mut O,
    fd: i32,
    out: &mut [u8],
) -> Result<usize, Errno> {
    if out.len() < 8 {
        return Err(Errno::EINVAL);
    }
    if t.kind_of(fd)? != FdKind::TimerFd {
        return Err(Errno::EINVAL);
    }
    let handle = t.handle_of(fd)?;
    let count = timer.read_expirations(handle)?;
    let bytes = count.to_le_bytes();
    let Some(slot) = out.get_mut(..8) else {
        return Err(Errno::EINVAL);
    };
    slot.copy_from_slice(&bytes);
    Ok(8)
}

#[cfg(test)]
#[path = "timerfd_ut.rs"]
mod tests;
