// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The one set of real seam implementations shared by the Linux and BSD personality exports.
//!
//! `raven-libc-platform` owns the fd table and the scheme clients, `raven-libc-sys` owns the
//! typed syscall wrappers. This module only adapts them to the two personality trait sets and
//! owns the process-global personality state (epoll tables, eventfd counters, timerfds, the
//! error state, kqueue registrations and the pledge models).
//!
//! Until C57 merges, every libc call is single-threaded, so a spin-protected `Global<T>` is the
//! whole synchronisation story (same assumption as `ffi::platform::global`).

use alloc::collections::BTreeMap;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use raven_libc_abi::structs::Timespec;
use raven_libc_platform::fd::OfdKind;
use raven_libc_platform::{Platform, RealSys};
use raven_libc_proc::backend::ProcBackend as _;
use raven_libc_proc::local_backend::LocalBackend;

use crate::ffi::platform::ShellLock;

/// A process-global cell protected by a spin word.
pub(crate) struct Global<T> {
    locked: AtomicBool,
    value: UnsafeCell<T>,
}

// SAFETY: every access goes through `with`, which holds `locked` for the whole closure; the
// value is never moved after the static initialisation. The process is single-threaded until
// C57 installs the thread runtime (module documentation).
unsafe impl<T> Sync for Global<T> {}

impl<T> Global<T> {
    /// Create the cell.
    pub(crate) const fn new(value: T) -> Self {
        Self {
            locked: AtomicBool::new(false),
            value: UnsafeCell::new(value),
        }
    }

    /// Run `f` with exclusive access.
    pub(crate) fn with<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        while self.locked.swap(true, Ordering::Acquire) {
            core::hint::spin_loop();
        }
        // SAFETY: the spin word gives this closure the only live reference for its duration.
        let value = unsafe { &mut *self.value.get() };
        let result = f(value);
        self.locked.store(false, Ordering::Release);
        result
    }
}

fn platform() -> &'static Platform<RealSys, ShellLock> {
    crate::ffi::platform::global()
}

fn error(value: raven_libc_platform::Errno) -> raven_libc_platform::Errno {
    value
}

fn sys_error(value: raven_abi::errno::Errno) -> raven_libc_platform::Errno {
    raven_libc_platform::Errno(value.0)
}

// ---------------------------------------------------------------------------------------------
// Linux personality seams

/// The real event queue.
pub(crate) struct RealEvq;

impl raven_libc_personality_linux::seams::EvqOps for RealEvq {
    fn create(
        &mut self,
    ) -> Result<
        raven_libc_personality_linux::seams::Handle,
        raven_libc_personality_linux::seams::Errno,
    > {
        raven_libc_sys::sys::evq_create(0).map_err(sys_error)
    }

    fn ctl(
        &mut self,
        q: raven_libc_personality_linux::seams::Handle,
        op: raven_libc_personality_linux::seams::EvqCtl,
        h: raven_libc_personality_linux::seams::Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        raven_libc_sys::sys::evq_ctl(q, op as u32, h, events, cookie).map_err(sys_error)
    }

    fn wait(
        &mut self,
        q: raven_libc_personality_linux::seams::Handle,
        out: &mut [raven_libc_personality_linux::seams::EvqEvent],
        timeout_ns: u64,
    ) -> Result<usize, raven_libc_personality_linux::seams::Errno> {
        raven_libc_sys::sys::evq_wait(q, out, timeout_ns).map_err(sys_error)
    }
}

#[derive(Clone, Default)]
struct SideKinds {
    entries: BTreeMap<
        i32,
        (
            raven_libc_personality_linux::seams::Handle,
            raven_libc_personality_linux::seams::FdKind,
        ),
    >,
}

static SIDE_KINDS: Global<SideKinds> = Global::new(SideKinds {
    entries: BTreeMap::new(),
});

fn platform_kind(kind: raven_libc_personality_linux::seams::FdKind) -> OfdKind {
    use raven_libc_personality_linux::seams::FdKind;
    match kind {
        FdKind::File => OfdKind::File,
        FdKind::Directory => OfdKind::Dir,
        FdKind::Pipe => OfdKind::Pipe,
        FdKind::Tty => OfdKind::Tty,
        FdKind::Socket => OfdKind::Socket,
        FdKind::Epoll => OfdKind::Epoll,
        FdKind::EventFd => OfdKind::EventFd,
        FdKind::TimerFd => OfdKind::Timer,
        // The platform fd table has no kind bit for these three; the side table remembers them.
        FdKind::SignalFd | FdKind::Inotify | FdKind::Kqueue | FdKind::Unknown => OfdKind::Unknown,
    }
}

fn logical_kind(kind: OfdKind) -> raven_libc_personality_linux::seams::FdKind {
    use raven_libc_personality_linux::seams::FdKind;
    match kind {
        OfdKind::File => FdKind::File,
        OfdKind::Dir => FdKind::Directory,
        OfdKind::Tty => FdKind::Tty,
        OfdKind::Socket => FdKind::Socket,
        OfdKind::Pipe => FdKind::Pipe,
        OfdKind::Epoll => FdKind::Epoll,
        OfdKind::Timer => FdKind::TimerFd,
        OfdKind::EventFd => FdKind::EventFd,
        OfdKind::ProcessFd | OfdKind::Unknown => FdKind::Unknown,
    }
}

/// The real process fd table of the Linux personality.
pub(crate) struct RealFdTable;

impl raven_libc_personality_linux::seams::FdTable for RealFdTable {
    fn handle_of(
        &self,
        fd: i32,
    ) -> Result<
        raven_libc_personality_linux::seams::Handle,
        raven_libc_personality_linux::seams::Errno,
    > {
        platform()
            .get(fd)
            .map(|entry| entry.handle())
            .map_err(error)
    }

    fn install(
        &mut self,
        handle: raven_libc_personality_linux::seams::Handle,
        kind: raven_libc_personality_linux::seams::FdKind,
        cloexec: bool,
    ) -> Result<i32, raven_libc_personality_linux::seams::Errno> {
        let fd = platform()
            .install(handle, platform_kind(kind), 0, None, cloexec)
            .map_err(error)?;
        SIDE_KINDS.with(|side| {
            side.entries.insert(fd, (handle, kind));
        });
        Ok(fd)
    }

    fn kind_of(
        &self,
        fd: i32,
    ) -> Result<
        raven_libc_personality_linux::seams::FdKind,
        raven_libc_personality_linux::seams::Errno,
    > {
        let handle = platform()
            .get(fd)
            .map(|entry| entry.handle())
            .map_err(error)?;
        let kind = platform()
            .get(fd)
            .map(|entry| entry.kind())
            .map_err(error)?;
        Ok(SIDE_KINDS.with(|side| {
            side.entries
                .get(&fd)
                .filter(|(stored, _)| *stored == handle)
                .map_or_else(|| logical_kind(kind), |(_, stored_kind)| *stored_kind)
        }))
    }

    fn epoch(&self, fd: i32) -> u32 {
        platform()
            .get(fd)
            .map(|entry| entry.handle().0)
            .unwrap_or(0)
    }

    fn close_range_from(&mut self, lo: i32) -> usize {
        let lo = lo.max(0);
        let count = SIDE_KINDS.with(|side| {
            let keys: alloc::vec::Vec<i32> = side
                .entries
                .keys()
                .copied()
                .filter(|fd| *fd >= lo)
                .collect();
            let count = keys.len();
            for fd in keys {
                side.entries.remove(&fd);
            }
            count
        });
        let _ = platform().close_from(u32::try_from(lo).unwrap_or(0));
        count
    }

    fn close_fd(&mut self, fd: i32) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        let result = platform().close(fd).map_err(error);
        SIDE_KINDS.with(|side| {
            side.entries.remove(&fd);
        });
        result
    }

    fn set_cloexec(
        &mut self,
        fd: i32,
        on: bool,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        let current = platform()
            .fcntl(fd, raven_libc_abi::fcntl::F_GETFD, 0)
            .map_err(error)?;
        let bit = i64::from(raven_libc_abi::fcntl::FD_CLOEXEC);
        let next = if on { current | bit } else { current & !bit };
        platform()
            .fcntl(fd, raven_libc_abi::fcntl::F_SETFD, next)
            .map_err(error)?;
        Ok(())
    }
}

/// The real file I/O.
pub(crate) struct RealFiles;

impl raven_libc_personality_linux::seams::FileIo for RealFiles {
    fn read(
        &mut self,
        fd: i32,
        b: &mut [u8],
    ) -> Result<usize, raven_libc_personality_linux::seams::Errno> {
        platform().read(fd, b).map_err(error)
    }

    fn write(
        &mut self,
        fd: i32,
        b: &[u8],
    ) -> Result<usize, raven_libc_personality_linux::seams::Errno> {
        platform().write(fd, b).map_err(error)
    }

    fn pread(
        &mut self,
        fd: i32,
        b: &mut [u8],
        off: i64,
    ) -> Result<usize, raven_libc_personality_linux::seams::Errno> {
        platform().pread(fd, b, off).map_err(error)
    }

    fn pwrite(
        &mut self,
        fd: i32,
        b: &[u8],
        off: i64,
    ) -> Result<usize, raven_libc_personality_linux::seams::Errno> {
        platform().pwrite(fd, b, off).map_err(error)
    }
}

/// The real system-information source.
pub(crate) struct RealSysInfo;

impl raven_libc_personality_linux::seams::SysInfoSrc for RealSysInfo {
    fn sysinfo(
        &mut self,
    ) -> Result<
        raven_libc_personality_linux::seams::RavenSysInfo,
        raven_libc_personality_linux::seams::Errno,
    > {
        use raven_libc_platform::SysBackend;
        let info = platform().backend().sysinfo().map_err(error)?;
        Ok(raven_libc_personality_linux::seams::RavenSysInfo {
            cpus_conf: info.cpus_conf,
            cpus_online: info.cpus_online,
            mem_total_pages: info.mem_total_pages,
            mem_free_pages: info.mem_free_pages,
            boot_time_ns: info.boot_time_ns,
        })
    }

    fn uname(
        &mut self,
    ) -> Result<
        raven_libc_personality_linux::seams::Uname,
        raven_libc_personality_linux::seams::Errno,
    > {
        platform().uname().map_err(error)
    }

    fn hostname(
        &mut self,
    ) -> Result<alloc::vec::Vec<u8>, raven_libc_personality_linux::seams::Errno> {
        Ok(platform().hostname_value())
    }

    fn set_hostname(
        &mut self,
        name: &[u8],
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        platform().sethostname(name).map_err(error)
    }
}

#[derive(Clone, Copy)]
struct CoreProc {
    name: [u8; 16],
    pdeathsig: i32,
    dumpable: i32,
    subreaper: i32,
    no_new_privs: i32,
    timerslack: u64,
}

static PROC: Global<CoreProc> = Global::new(CoreProc {
    name: [0; 16],
    pdeathsig: 0,
    dumpable: 1,
    subreaper: 0,
    no_new_privs: 0,
    timerslack: raven_libc_personality_linux::prctl::TIMERSLACK_DEFAULT,
});

/// The real process record. The title is a `procd` request; `procd` is C64's, so it is `ENOSYS`
/// today and `setproctitle` (a void function) ignores it.
pub(crate) struct RealProc;

impl raven_libc_personality_linux::seams::ProcCtl for RealProc {
    fn prctl_name_set(
        &mut self,
        value: &[u8; 16],
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.name = *value);
        Ok(())
    }

    fn prctl_name_get(&mut self) -> Result<[u8; 16], raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.name))
    }

    fn set_title(
        &mut self,
        title: Option<&[u8]>,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        let backend = LocalBackend::new(None);
        backend.set_title(title.unwrap_or(b"")).map_err(|code| {
            raven_libc_platform::Errno(
                u16::try_from(code).unwrap_or(raven_libc_abi::errno::EINVAL as u16),
            )
        })
    }

    fn prctl_pdeathsig_set(
        &mut self,
        signal: i32,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.pdeathsig = signal);
        Ok(())
    }

    fn prctl_pdeathsig_get(&mut self) -> Result<i32, raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.pdeathsig))
    }

    fn prctl_dumpable_set(
        &mut self,
        value: i32,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.dumpable = value);
        Ok(())
    }

    fn prctl_dumpable_get(&mut self) -> Result<i32, raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.dumpable))
    }

    fn prctl_subreaper_set(
        &mut self,
        value: i32,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.subreaper = value);
        Ok(())
    }

    fn prctl_subreaper_get(&mut self) -> Result<i32, raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.subreaper))
    }

    fn prctl_no_new_privs_set(&mut self) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.no_new_privs = 1);
        Ok(())
    }

    fn prctl_no_new_privs_get(
        &mut self,
    ) -> Result<i32, raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.no_new_privs))
    }

    fn prctl_timerslack_set(
        &mut self,
        nanoseconds: u64,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        PROC.with(|proc| proc.timerslack = nanoseconds);
        Ok(())
    }

    fn prctl_timerslack_get(&mut self) -> Result<u64, raven_libc_personality_linux::seams::Errno> {
        Ok(PROC.with(|proc| proc.timerslack))
    }
}

/// The real user-event source.
pub(crate) struct RealEvent;

impl raven_libc_personality_linux::seams::EventOps for RealEvent {
    fn create(
        &mut self,
    ) -> Result<
        raven_libc_personality_linux::seams::Handle,
        raven_libc_personality_linux::seams::Errno,
    > {
        raven_libc_sys::sys::event_create(0).map_err(sys_error)
    }

    fn signal(
        &mut self,
        handle: raven_libc_personality_linux::seams::Handle,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        raven_libc_sys::sys::event_signal(handle, 1).map_err(sys_error)
    }
}

/// The real timer source.
pub(crate) struct RealTimer;

impl raven_libc_personality_linux::seams::TimerOps for RealTimer {
    fn create(
        &mut self,
        clock: i32,
    ) -> Result<
        raven_libc_personality_linux::seams::Handle,
        raven_libc_personality_linux::seams::Errno,
    > {
        let id = u32::try_from(clock).map_err(|_| raven_libc_platform::Errno::EINVAL)?;
        raven_libc_sys::sys::timer_create(id, 0).map_err(sys_error)
    }

    fn set(
        &mut self,
        handle: raven_libc_personality_linux::seams::Handle,
        initial_ns: u64,
        interval_ns: u64,
    ) -> Result<(), raven_libc_personality_linux::seams::Errno> {
        raven_libc_sys::sys::timer_set(handle, initial_ns, interval_ns).map_err(sys_error)
    }
}

/// The real signal source: `PG-7` is absent, so the trait defaults (`ENOSYS`) apply.
pub(crate) struct RealSignal;

impl raven_libc_personality_linux::seams::SignalOps for RealSignal {}

/// The real filesystem watch service: `PG-38` is absent, so the trait defaults (`ENOSYS`) apply.
pub(crate) struct RealWatch;

impl raven_libc_personality_linux::seams::FsWatch for RealWatch {}

// ---------------------------------------------------------------------------------------------
// BSD personality seams

/// The BSD event queue.
pub(crate) struct RealEvqBsd;

impl raven_libc_personality_bsd::seams::EvqOps for RealEvqBsd {
    fn create(
        &mut self,
    ) -> Result<raven_libc_personality_bsd::seams::Handle, raven_libc_personality_bsd::seams::Errno>
    {
        raven_libc_sys::sys::evq_create(0).map_err(sys_error)
    }

    fn ctl(
        &mut self,
        q: raven_libc_personality_bsd::seams::Handle,
        op: raven_libc_personality_bsd::seams::EvqCtl,
        h: raven_libc_personality_bsd::seams::Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        raven_libc_sys::sys::evq_ctl(q, op as u32, h, events, cookie).map_err(sys_error)
    }

    fn wait(
        &mut self,
        q: raven_libc_personality_bsd::seams::Handle,
        out: &mut [raven_libc_personality_bsd::seams::EvqEvent],
        timeout_ns: u64,
    ) -> Result<usize, raven_libc_personality_bsd::seams::Errno> {
        raven_libc_sys::sys::evq_wait(q, out, timeout_ns).map_err(sys_error)
    }
}

#[derive(Clone, Default)]
struct BsdSideKinds {
    entries: BTreeMap<
        i32,
        (
            raven_libc_personality_bsd::seams::Handle,
            raven_libc_personality_bsd::seams::FdKind,
        ),
    >,
}

static BSD_SIDE_KINDS: Global<BsdSideKinds> = Global::new(BsdSideKinds {
    entries: BTreeMap::new(),
});

fn bsd_platform_kind(kind: raven_libc_personality_bsd::seams::FdKind) -> OfdKind {
    use raven_libc_personality_bsd::seams::FdKind;
    match kind {
        FdKind::File => OfdKind::File,
        FdKind::Directory => OfdKind::Dir,
        FdKind::Pipe => OfdKind::Pipe,
        FdKind::Tty => OfdKind::Tty,
        FdKind::Socket => OfdKind::Socket,
        FdKind::Epoll => OfdKind::Epoll,
        FdKind::EventFd => OfdKind::EventFd,
        FdKind::TimerFd => OfdKind::Timer,
        FdKind::SignalFd | FdKind::Inotify | FdKind::Kqueue | FdKind::Unknown => OfdKind::Unknown,
    }
}

fn bsd_logical_kind(kind: OfdKind) -> raven_libc_personality_bsd::seams::FdKind {
    use raven_libc_personality_bsd::seams::FdKind;
    match kind {
        OfdKind::File => FdKind::File,
        OfdKind::Dir => FdKind::Directory,
        OfdKind::Tty => FdKind::Tty,
        OfdKind::Socket => FdKind::Socket,
        OfdKind::Pipe => FdKind::Pipe,
        OfdKind::Epoll => FdKind::Epoll,
        OfdKind::Timer => FdKind::TimerFd,
        OfdKind::EventFd => FdKind::EventFd,
        OfdKind::ProcessFd | OfdKind::Unknown => FdKind::Unknown,
    }
}

/// The BSD fd table.
pub(crate) struct RealFdTableBsd;

impl raven_libc_personality_bsd::seams::FdTable for RealFdTableBsd {
    fn handle_of(
        &self,
        fd: i32,
    ) -> Result<raven_libc_personality_bsd::seams::Handle, raven_libc_personality_bsd::seams::Errno>
    {
        platform()
            .get(fd)
            .map(|entry| entry.handle())
            .map_err(error)
    }

    fn install(
        &mut self,
        handle: raven_libc_personality_bsd::seams::Handle,
        kind: raven_libc_personality_bsd::seams::FdKind,
        cloexec: bool,
    ) -> Result<i32, raven_libc_personality_bsd::seams::Errno> {
        let fd = platform()
            .install(handle, bsd_platform_kind(kind), 0, None, cloexec)
            .map_err(error)?;
        BSD_SIDE_KINDS.with(|side| {
            side.entries.insert(fd, (handle, kind));
        });
        Ok(fd)
    }

    fn kind_of(
        &self,
        fd: i32,
    ) -> Result<raven_libc_personality_bsd::seams::FdKind, raven_libc_personality_bsd::seams::Errno>
    {
        let handle = platform()
            .get(fd)
            .map(|entry| entry.handle())
            .map_err(error)?;
        let kind = platform()
            .get(fd)
            .map(|entry| entry.kind())
            .map_err(error)?;
        Ok(BSD_SIDE_KINDS.with(|side| {
            side.entries
                .get(&fd)
                .filter(|(stored, _)| *stored == handle)
                .map_or_else(|| bsd_logical_kind(kind), |(_, stored_kind)| *stored_kind)
        }))
    }

    fn epoch(&self, fd: i32) -> u32 {
        platform()
            .get(fd)
            .map(|entry| entry.handle().0)
            .unwrap_or(0)
    }

    fn close_range_from(&mut self, lo: i32) -> usize {
        let lo = lo.max(0);
        let count = BSD_SIDE_KINDS.with(|side| {
            let keys: alloc::vec::Vec<i32> = side
                .entries
                .keys()
                .copied()
                .filter(|fd| *fd >= lo)
                .collect();
            let count = keys.len();
            for fd in keys {
                side.entries.remove(&fd);
            }
            count
        });
        let _ = platform().close_from(u32::try_from(lo).unwrap_or(0));
        count
    }

    fn close_fd(&mut self, fd: i32) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        let result = platform().close(fd).map_err(error);
        BSD_SIDE_KINDS.with(|side| {
            side.entries.remove(&fd);
        });
        result
    }

    fn set_cloexec(
        &mut self,
        fd: i32,
        on: bool,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        let current = platform()
            .fcntl(fd, raven_libc_abi::fcntl::F_GETFD, 0)
            .map_err(error)?;
        let bit = i64::from(raven_libc_abi::fcntl::FD_CLOEXEC);
        let next = if on { current | bit } else { current & !bit };
        platform()
            .fcntl(fd, raven_libc_abi::fcntl::F_SETFD, next)
            .map_err(error)?;
        Ok(())
    }
}

/// The BSD source factory.
pub(crate) struct RealSrc;

impl raven_libc_personality_bsd::seams::SrcOps for RealSrc {
    fn timer_create(
        &mut self,
        clock: i32,
    ) -> Result<raven_libc_personality_bsd::seams::Handle, raven_libc_personality_bsd::seams::Errno>
    {
        let id = u32::try_from(clock).map_err(|_| raven_libc_platform::Errno::EINVAL)?;
        raven_libc_sys::sys::timer_create(id, 0).map_err(sys_error)
    }

    fn timer_set(
        &mut self,
        handle: raven_libc_personality_bsd::seams::Handle,
        initial_ns: u64,
        interval_ns: u64,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        raven_libc_sys::sys::timer_set(handle, initial_ns, interval_ns).map_err(sys_error)
    }

    fn event_create(
        &mut self,
    ) -> Result<raven_libc_personality_bsd::seams::Handle, raven_libc_personality_bsd::seams::Errno>
    {
        raven_libc_sys::sys::event_create(0).map_err(sys_error)
    }

    fn event_signal(
        &mut self,
        handle: raven_libc_personality_bsd::seams::Handle,
        value: u64,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        raven_libc_sys::sys::event_signal(handle, value).map_err(sys_error)
    }
}

/// The BSD system-information source.
pub(crate) struct RealSysInfoBsd;

impl raven_libc_personality_bsd::seams::SysInfoSrc for RealSysInfoBsd {
    fn sysinfo(
        &mut self,
    ) -> Result<
        raven_libc_personality_bsd::seams::RavenSysInfo,
        raven_libc_personality_bsd::seams::Errno,
    > {
        use raven_libc_platform::SysBackend;
        let info = platform().backend().sysinfo().map_err(error)?;
        Ok(raven_libc_personality_bsd::seams::RavenSysInfo {
            cpus_conf: info.cpus_conf,
            cpus_online: info.cpus_online,
            mem_total_pages: info.mem_total_pages,
            mem_free_pages: info.mem_free_pages,
            boot_time_ns: info.boot_time_ns,
        })
    }

    fn uname(
        &mut self,
    ) -> Result<raven_libc_personality_bsd::seams::Uname, raven_libc_personality_bsd::seams::Errno>
    {
        platform().uname().map_err(error)
    }

    fn hostname(
        &mut self,
    ) -> Result<alloc::vec::Vec<u8>, raven_libc_personality_bsd::seams::Errno> {
        Ok(platform().hostname_value())
    }

    fn set_hostname(
        &mut self,
        name: &[u8],
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        platform().sethostname(name).map_err(error)
    }
}

/// The BSD process record (title only).
pub(crate) struct RealProcBsd;

impl raven_libc_personality_bsd::seams::ProcCtl for RealProcBsd {
    fn prctl_name_set(
        &mut self,
        value: &[u8; 16],
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        PROC.with(|proc| proc.name = *value);
        Ok(())
    }

    fn prctl_name_get(&mut self) -> Result<[u8; 16], raven_libc_personality_bsd::seams::Errno> {
        Ok(PROC.with(|proc| proc.name))
    }

    fn set_title(
        &mut self,
        title: Option<&[u8]>,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        let backend = LocalBackend::new(None);
        backend.set_title(title.unwrap_or(b"")).map_err(|code| {
            raven_libc_platform::Errno(
                u16::try_from(code).unwrap_or(raven_libc_abi::errno::EINVAL as u16),
            )
        })
    }
}

/// The phase-1 pledge backend: stores nothing, returns `Ok` (`PG-39` enforcement absent).
pub(crate) struct RealPledge;

impl raven_libc_personality_bsd::seams::PledgeBackend for RealPledge {
    fn apply(
        &mut self,
        _promises: Option<&[u8]>,
        _exec: Option<&[u8]>,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        Ok(())
    }

    fn unveil(
        &mut self,
        _path: Option<&[u8]>,
        _perms: Option<&[u8]>,
    ) -> Result<(), raven_libc_personality_bsd::seams::Errno> {
        Ok(())
    }
}

/// The mount list: `PG-37` is absent, so the trait default (`ENOSYS`) applies.
pub(crate) struct RealMounts;

impl raven_libc_personality_bsd::seams::MountList for RealMounts {}

// ---------------------------------------------------------------------------------------------
// Global personality state and small helpers

/// The epoll registrations.
pub(crate) static EPOLL: Global<raven_libc_personality_linux::epoll::EpollTable> =
    Global::new(raven_libc_personality_linux::epoll::EpollTable::new());
/// The eventfd counters.
pub(crate) static EVENT_FDS: Global<raven_libc_personality_linux::eventfd::EventFdTable> =
    Global::new(raven_libc_personality_linux::eventfd::EventFdTable::new());
/// The timerfds.
pub(crate) static TIMER_FDS: Global<raven_libc_personality_linux::timerfd::TimerFdTable> =
    Global::new(raven_libc_personality_linux::timerfd::TimerFdTable::new());
/// GNU `error_at_line` state.
pub(crate) static ERROR_STATE: Global<raven_libc_personality_linux::error::ErrorState> =
    Global::new(raven_libc_personality_linux::error::ErrorState::new());
/// The kqueue registrations.
pub(crate) static KQUEUES: Global<raven_libc_personality_bsd::kqueue::Kqueue> =
    Global::new(raven_libc_personality_bsd::kqueue::Kqueue::new());
/// The pledge promise sets.
pub(crate) static PLEDGE_STATE: Global<raven_libc_pledge::reduce::PledgeState> =
    Global::new(raven_libc_pledge::reduce::PledgeState::new());
/// The unveil table.
pub(crate) static UNVEIL_STATE: Global<raven_libc_pledge::unveil::UnveilTable> =
    Global::new(raven_libc_pledge::unveil::UnveilTable::new());
/// The `setproctitle_fast` cache.
pub(crate) static TITLE_CACHE: Global<raven_libc_personality_bsd::setproctitle::Cache> =
    Global::new(raven_libc_personality_bsd::setproctitle::Cache::new());

/// The retained auxv values this branch can answer (`AT_PAGESZ`, `AT_CLKTCK`, `AT_RANDOM`,
/// `AT_SECURE`). C61's start does not publish the full table yet.
pub(crate) fn aux_lookup(key: u64) -> Option<u64> {
    match key {
        6 => Some(u64::try_from(crate::start::page_size()).unwrap_or(0)),
        17 => Some(u64::try_from(crate::start::clk_tck()).unwrap_or(0)),
        23 => Some(0),
        25 => Some(u64::try_from(crate::start::random_pointer()).unwrap_or(0)),
        _ => None,
    }
}

/// `Ok(i32)` or `errno`/`-1`.
pub(crate) fn ret_int(
    value: Result<core::ffi::c_int, raven_libc_platform::Errno>,
) -> core::ffi::c_int {
    crate::ffi::ret::platform_int(value)
}

/// An `i64` result as a C `int` with the `errno`/`-1` contract.
pub(crate) fn ret_i64(value: Result<i64, raven_libc_platform::Errno>) -> core::ffi::c_int {
    match value {
        Ok(number) => core::ffi::c_int::try_from(number).unwrap_or(-1),
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}

/// `Ok(())` -> 0, `Err` -> `errno`/`-1`.
pub(crate) fn ret_unit(value: Result<(), raven_libc_platform::Errno>) -> core::ffi::c_int {
    match value {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}

/// `Ok(usize)` as the C `ssize_t`.
pub(crate) fn ret_ssize(value: Result<usize, raven_libc_platform::Errno>) -> isize {
    crate::ffi::ret::platform_ssize(value)
}

/// `Ok(usize)` as a C `int` count.
pub(crate) fn ret_count(value: Result<usize, raven_libc_platform::Errno>) -> core::ffi::c_int {
    match value {
        Ok(count) => core::ffi::c_int::try_from(count).unwrap_or(-1),
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}

/// A `Timespec` pointer as a value.
///
/// # Safety
/// `pointer` is NULL or points to a readable `struct timespec`.
pub(crate) unsafe fn timespec(pointer: *const Timespec) -> Option<Timespec> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller's pointer contract.
        Some(unsafe { core::ptr::read_unaligned(pointer) })
    }
}
