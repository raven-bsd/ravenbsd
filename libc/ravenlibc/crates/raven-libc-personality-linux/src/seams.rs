// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The narrow seams between the Linux personality logic and the C-ABI shell.
//!
//! The shell implements each trait over `raven-libc-platform` and `raven-libc-sys`; the host
//! tests in `crate::testutil` implement them deterministically. A primitive that does not
//! exist yet has a default method returning `ENOSYS`, never a silent success (`11` 3.8).
//!
//! `EvqOps` is deliberately duplicated in `raven-libc-personality-bsd` (the two crates are
//! independent siblings, ticket C66 4.2); if a later platform ticket exports one shared trait,
//! both copies are replaced by it.

use alloc::vec::Vec;

pub use raven_libc_platform::Errno;

pub use raven_libc_platform::Handle;
pub use raven_libc_sys::sys::EvqEvent;

/// The Linux `sigset_t` shape used by `epoll_pwait` and `signalfd`.
///
/// `raven-libc-signal` (C58) is not on the C66 branch, so the ticket's fallback of a local
/// two-word type is used. The shell header (`arch/*/include/bits/alltypes.h`) defines the same
/// 64-bit, two-word layout.
pub type SigSet = [u64; 2];

/// Whether the set contains `signal` (1-based Linux signal number).
#[must_use]
pub fn sigset_contains(set: &SigSet, signal: u32) -> bool {
    if signal == 0 || signal > 64 {
        return false;
    }
    let zero_based = signal.saturating_sub(1);
    let index = usize::try_from(zero_based.checked_div(64).unwrap_or(0)).unwrap_or(0);
    let bit = zero_based.checked_rem(64).unwrap_or(0);
    match set.get(index) {
        Some(word) => word & (1u64 << bit) != 0,
        None => false,
    }
}

/// The event-queue control operation (`EVQ_CTL`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvqCtl {
    /// `EVQ_CTL` add.
    Add = 1,
    /// `EVQ_CTL` modify.
    Mod = 2,
    /// `EVQ_CTL` delete.
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
    /// A kqueue instance (BSD personality).
    Kqueue,
    /// Not known yet.
    Unknown,
}

impl FdKind {
    /// Whether the event queue can report readiness for this kind.
    ///
    /// Until `PG-25` a scheme file reports only `HUP`; regular files and directories are not
    /// pollable at all and `epoll_ctl(ADD)` rejects them with `EPERM` (Linux contract).
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

    /// Close one descriptor.
    ///
    /// Additive method over ticket C66 4.2, needed by `close_range(lo, hi, 0)`: the shell
    /// implements it over the platform fd table, a fake over its map. The default is honest
    /// `ENOSYS`, never a silent success.
    fn close_fd(&mut self, _fd: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    /// Set `FD_CLOEXEC` on one descriptor (`CLOSE_RANGE_CLOEXEC`).
    ///
    /// Additive method over ticket C66 4.2; default `ENOSYS`.
    fn set_cloexec(&mut self, _fd: i32, _on: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    /// Close every open descriptor in `lo..=hi`, returning the count.
    ///
    /// Additive method over ticket C66 4.2; the default loops [`FdTable::close_fd`].
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

/// The event queue (`EVQ_CREATE`/`EVQ_CTL`/`EVQ_WAIT`).
pub trait EvqOps {
    /// Create a queue and return its handle.
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
    /// Wait for events; `timeout_ns == 0` waits forever, `Ok(0)` means timed out.
    fn wait(&mut self, q: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno>;
}

/// A monotonic/realtime clock source.
pub trait TimeSrc {
    /// Nanoseconds of `clock` (`CLOCK_REALTIME 0`, `CLOCK_MONOTONIC 1`, `CLOCK_BOOTTIME 7`).
    fn now_ns(&self, clock: i32) -> Result<u64, Errno>;
}

/// File I/O on descriptors; the shell maps it to the platform fd table.
pub trait FileIo {
    /// `read(2)`.
    fn read(&mut self, fd: i32, b: &mut [u8]) -> Result<usize, Errno>;
    /// `write(2)`.
    fn write(&mut self, fd: i32, b: &[u8]) -> Result<usize, Errno>;
    /// `pread(2)`; `ENOSYS` until `PG-22`.
    fn pread(&mut self, fd: i32, b: &mut [u8], off: i64) -> Result<usize, Errno>;
    /// `pwrite(2)`; `ENOSYS` until `PG-22`.
    fn pwrite(&mut self, fd: i32, b: &[u8], off: i64) -> Result<usize, Errno>;
}

/// The `uname`/hostname/sysinfo record of the Linux personality.
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
    /// `PR_SET_NAME`: store the 16-byte name (15 bytes plus NUL).
    fn prctl_name_set(&mut self, name: &[u8; 16]) -> Result<(), Errno>;
    /// `PR_GET_NAME`: return the stored 16-byte name.
    fn prctl_name_get(&mut self) -> Result<[u8; 16], Errno>;
    /// `SETTITLE` to `procd`; `None` restores the original title.
    fn set_title(&mut self, title: Option<&[u8]>) -> Result<(), Errno>;

    /// `PR_SET_PDEATHSIG`. Additive default: `ENOSYS`.
    fn prctl_pdeathsig_set(&mut self, _signal: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_GET_PDEATHSIG`. Additive default: `ENOSYS`.
    fn prctl_pdeathsig_get(&mut self) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_SET_DUMPABLE`. Additive default: `ENOSYS`.
    fn prctl_dumpable_set(&mut self, _value: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_GET_DUMPABLE`. Additive default: `ENOSYS`.
    fn prctl_dumpable_get(&mut self) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_SET_CHILD_SUBREAPER`. Additive default: `ENOSYS`.
    fn prctl_subreaper_set(&mut self, _value: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_GET_CHILD_SUBREAPER`. Additive default: `ENOSYS`.
    fn prctl_subreaper_get(&mut self) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_SET_NO_NEW_PRIVS`. Additive default: `ENOSYS`.
    fn prctl_no_new_privs_set(&mut self) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_GET_NO_NEW_PRIVS`. Additive default: `ENOSYS`.
    fn prctl_no_new_privs_get(&mut self) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_SET_TIMERSLACK`. Additive default: `ENOSYS`.
    fn prctl_timerslack_set(&mut self, _nanoseconds: u64) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
    /// `PR_GET_TIMERSLACK`. Additive default: `ENOSYS`.
    fn prctl_timerslack_get(&mut self) -> Result<u64, Errno> {
        Err(Errno::ENOSYS)
    }
}

/// `EVENT_CREATE`/`EVENT_SIGNAL` plus the `PG-19` readable counter.
pub trait EventOps {
    /// Create a user-event handle.
    fn create(&mut self) -> Result<Handle, Errno>;
    /// Ring the readiness doorbell.
    fn signal(&mut self, handle: Handle) -> Result<(), Errno>;
    /// Read the kernel counter; `ENOSYS` until `PG-19`.
    fn read_counter(&mut self, _handle: Handle) -> Result<u64, Errno> {
        Err(Errno::ENOSYS)
    }
    /// Wait until the handle is readable; `ENOSYS` until `PG-19`.
    fn wait_readable(&mut self, _handle: Handle) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// `TIMER_CREATE`/`TIMER_SET` plus the `PG-19` getter.
pub trait TimerOps {
    /// Create a timer for `clock`.
    fn create(&mut self, clock: i32) -> Result<Handle, Errno>;
    /// Arm or disarm: `initial_ns == 0` disarms.
    fn set(&mut self, handle: Handle, initial_ns: u64, interval_ns: u64) -> Result<(), Errno>;
    /// Read the remaining/interval pair; `ENOSYS` until `PG-19`.
    fn get(&mut self, _handle: Handle) -> Result<(u64, u64), Errno> {
        Err(Errno::ENOSYS)
    }
    /// Read the expiration count; `ENOSYS` until `PG-19`.
    fn read_expirations(&mut self, _handle: Handle) -> Result<u64, Errno> {
        Err(Errno::ENOSYS)
    }
}

/// The signal source `signalfd` needs (`PG-7`).
pub trait SignalOps {
    /// Create a signal-event source; the default is `ENOSYS` until `PG-7`.
    fn create(&mut self, _mask: &SigSet, _flags: i32) -> Result<Handle, Errno> {
        Err(Errno::ENOSYS)
    }
    /// Replace the mask of an existing source; the default is `ENOSYS` until `PG-7`.
    fn update(&mut self, _handle: Handle, _mask: &SigSet) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// The filesystem watch service `inotify` needs (`PG-38`).
pub trait FsWatch {
    /// `inotify_init1`; default `ENOSYS` until `PG-38`.
    fn init(&mut self, _flags: i32) -> Result<Handle, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `inotify_add_watch`; default `ENOSYS` until `PG-38`.
    fn add_watch(&mut self, _fd: i32, _path: &[u8], _mask: u32) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }
    /// `inotify_rm_watch`; default `ENOSYS` until `PG-38`.
    fn rm_watch(&mut self, _fd: i32, _wd: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// `CLOCK_REALTIME`.
pub const CLOCK_REALTIME: i32 = 0;
/// `CLOCK_MONOTONIC`.
pub const CLOCK_MONOTONIC: i32 = 1;
/// `CLOCK_BOOTTIME`.
pub const CLOCK_BOOTTIME: i32 = 7;

#[cfg(test)]
#[path = "seams_ut.rs"]
mod tests;
