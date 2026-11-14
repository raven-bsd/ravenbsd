// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The narrow seams between the BSD personality logic and the C-ABI shell.
//!
//! `EvqOps` and `FdTable` are deliberately duplicated from the Linux personality crate: the two
//! crates are independent siblings (ticket C66 4.2). A primitive that does not exist yet has a
//! default returning `ENOSYS`, never a silent success.

use alloc::vec::Vec;

pub use raven_libc_platform::Errno;

pub use raven_libc_abi::structs::Timespec;
pub use raven_libc_platform::Handle;
pub use raven_libc_sys::sys::EvqEvent;

/// The event-queue control operation (`EVQ_CTL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvqCtl {
    /// Add.
    Add = 1,
    /// Modify.
    Mod = 2,
    /// Delete.
    Del = 3,
}

/// The kind of descriptor the personality sees.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FdKind {
    /// A regular file.
    File,
    /// A directory.
    Directory,
    /// A pipe or FIFO.
    Pipe,
    /// A terminal.
    Tty,
    /// A socket.
    Socket,
    /// An epoll instance.
    Epoll,
    /// An eventfd counter.
    EventFd,
    /// A timerfd.
    TimerFd,
    /// A signalfd.
    SignalFd,
    /// An inotify instance.
    Inotify,
    /// A kqueue instance.
    Kqueue,
    /// Not known yet.
    Unknown,
}

impl FdKind {
    /// Whether the event queue can report readiness for this kind.
    #[must_use]
    pub const fn pollable(self) -> bool {
        !matches!(self, Self::File | Self::Directory)
    }
}

/// The process fd table, as the personality needs it.
pub trait FdTable {
    /// The kernel handle behind `fd`.
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno>;
    /// Install `handle` as a new descriptor and return the number.
    fn install(&mut self, handle: Handle, kind: FdKind, cloexec: bool) -> Result<i32, Errno>;
    /// The kind of `fd`.
    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno>;
    /// A generation that changes when `fd` is closed and reused; `0` for a closed fd.
    fn epoch(&self, fd: i32) -> u32;
    /// Close every open descriptor `>= lo` and return how many were closed.
    fn close_range_from(&mut self, lo: i32) -> usize;

    /// Close one descriptor; additive over ticket C66 4.2.
    fn close_fd(&mut self, _fd: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    /// Set `FD_CLOEXEC`; additive over ticket C66 4.2.
    fn set_cloexec(&mut self, _fd: i32, _on: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    /// Close every descriptor in `lo..=hi`; additive over ticket C66 4.2.
    fn close_range_between(&mut self, lo: i32, hi: i32) -> usize {
        if lo < 0 || hi < lo {
            return 0;
        }
        let mut closed = 0usize;
        let mut fd = lo;
        loop {
            if self.close_fd(fd).is_ok() {
                closed = closed.saturating_add(1);
            }
            if fd == hi {
                break;
            }
            fd = match fd.checked_add(1) {
                Some(next) => next,
                None => break,
            };
        }
        closed
    }
}

/// The event queue.
pub trait EvqOps {
    /// Create a queue.
    fn create(&mut self) -> Result<Handle, Errno>;
    /// Add, modify or delete one registration.
    fn ctl(
        &mut self,
        q: Handle,
        op: EvqCtl,
        h: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno>;
    /// Wait; `timeout_ns == 0` waits forever.
    fn wait(&mut self, q: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno>;
}

/// The `uname`/hostname/sysinfo record of the BSD personality.
pub use raven_libc_abi::structs::Utsname as Uname;

/// The `SYSINFO` record (`PG-21`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RavenSysInfo {
    /// Configured CPUs.
    pub cpus_conf: u32,
    /// Online CPUs.
    pub cpus_online: u32,
    /// Total physical pages.
    pub mem_total_pages: u64,
    /// Free physical pages.
    pub mem_free_pages: u64,
    /// Boot time in nanoseconds since the epoch.
    pub boot_time_ns: u64,
}

/// System information (T8).
pub trait SysInfoSrc {
    /// `SYSINFO`; `ENOSYS` until `PG-21`.
    fn sysinfo(&mut self) -> Result<RavenSysInfo, Errno>;
    /// `uname`.
    fn uname(&mut self) -> Result<Uname, Errno>;
    /// `gethostname`.
    fn hostname(&mut self) -> Result<Vec<u8>, Errno>;
    /// `sethostname`.
    fn set_hostname(&mut self, name: &[u8]) -> Result<(), Errno>;
}

/// Process identity and title control (`procd`, `PG-44`).
pub trait ProcCtl {
    /// Store the 16-byte process name.
    fn prctl_name_set(&mut self, name: &[u8; 16]) -> Result<(), Errno>;
    /// Read the 16-byte process name.
    fn prctl_name_get(&mut self) -> Result<[u8; 16], Errno>;
    /// `SETTITLE`; `None` restores the original.
    fn set_title(&mut self, title: Option<&[u8]>) -> Result<(), Errno>;
}

/// The `pledge`/`unveil` backend (`procd`/`vfsd`), `PG-39`.
pub trait PledgeBackend {
    /// Apply the promise sets.
    fn apply(&mut self, promises: Option<&[u8]>, exec: Option<&[u8]>) -> Result<(), Errno>;
    /// Apply one unveil entry.
    fn unveil(&mut self, path: Option<&[u8]>, perms: Option<&[u8]>) -> Result<(), Errno>;
}

/// The source objects `kevent` creates for filters that are not fd readiness.
pub trait SrcOps {
    /// `TIMER_CREATE`.
    fn timer_create(&mut self, clock: i32) -> Result<Handle, Errno>;
    /// `TIMER_SET`.
    fn timer_set(&mut self, handle: Handle, initial_ns: u64, interval_ns: u64)
    -> Result<(), Errno>;
    /// `EVENT_CREATE`.
    fn event_create(&mut self) -> Result<Handle, Errno>;
    /// `EVENT_SIGNAL`.
    fn event_signal(&mut self, handle: Handle, value: u64) -> Result<(), Errno>;
    /// The process handle of a pid (`PG-44`); default `ENOSYS`.
    fn process_handle(&mut self, _pid: i32) -> Result<Handle, Errno> {
        Err(Errno::ENOSYS)
    }
}

/// The mount list `getmntinfo` needs (`PG-37`).
pub trait MountList {
    /// The current mount list; default `ENOSYS` until `PG-37`.
    fn list(&mut self) -> Result<Vec<crate::getmntinfo::StatfsLinux>, Errno> {
        Err(Errno::ENOSYS)
    }
}

#[cfg(test)]
#[path = "seams_ut.rs"]
mod tests;
