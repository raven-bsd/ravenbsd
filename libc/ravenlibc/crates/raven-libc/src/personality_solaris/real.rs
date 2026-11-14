// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The real seam implementations of the Solaris personality.
//!
//! `raven-libc-platform` owns the fd table and the scheme clients, `raven-libc-sys` owns the
//! typed syscall wrappers. This module adapts them to the personality traits, owns the port
//! table and the concurrency hint, and records the C57 absence: every pthread-backed call
//! answers the documented `ENOSYS` until `raven-libc-thread` merges (C67 4.2 fallback (b)).
//!
//! The `thr_*`/`synch_*` bodies that need no thread runtime are real: the initial-thread
//! identity, the stack constant, the kernel yield, process exit from the only thread, and the
//! process-wide concurrency hint.

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicI32, Ordering};

use raven_libc_abi::structs::Timespec;
use raven_libc_platform::fd::OfdKind;
use raven_libc_platform::{Errno, Handle};

use raven_libc_personality_solaris::port::PortTable;
use raven_libc_personality_solaris::seams::{
    AuxSrc, CondT, DoorbellOps, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, KeyDtor, MutexT,
    PosixThreads, RwlockT, SemaT, SigSet, StartFn, ThreadAttrPlan,
};

use crate::personality_common::Global;

/// The process-wide port table.
pub(crate) static PORTS: Global<PortTable> = Global::new(PortTable::new());

/// The process-wide `thr_setconcurrency` hint.
pub(crate) static CONCURRENCY: AtomicI32 = AtomicI32::new(0);

fn platform() -> &'static raven_libc_platform::Platform<
    raven_libc_platform::RealSys,
    crate::ffi::platform::ShellLock,
> {
    crate::ffi::platform::global()
}

fn sys_error(value: raven_abi::errno::Errno) -> Errno {
    Errno(value.0)
}

/// The event queue.
pub(crate) struct RealEvq;

impl EvqOps for RealEvq {
    fn create(&mut self) -> Result<Handle, Errno> {
        raven_libc_sys::sys::evq_create(0).map_err(sys_error)
    }

    fn ctl(
        &mut self,
        q: Handle,
        op: EvqCtl,
        h: Handle,
        events: u32,
        cookie: u64,
    ) -> Result<(), Errno> {
        raven_libc_sys::sys::evq_ctl(q, op as u32, h, events, cookie).map_err(sys_error)
    }

    fn wait(&mut self, q: Handle, out: &mut [EvqEvent], timeout_ns: u64) -> Result<usize, Errno> {
        raven_libc_sys::sys::evq_wait(q, out, timeout_ns).map_err(sys_error)
    }
}

/// The user-event doorbell.
pub(crate) struct RealDoorbell;

impl DoorbellOps for RealDoorbell {
    fn create(&mut self) -> Result<Handle, Errno> {
        raven_libc_sys::sys::event_create(0).map_err(sys_error)
    }

    fn signal(&mut self, h: Handle, value: u64) -> Result<(), Errno> {
        raven_libc_sys::sys::event_signal(h, value).map_err(sys_error)
    }
}

#[derive(Clone, Default)]
struct SideKinds {
    entries: BTreeMap<i32, (Handle, FdKind)>,
}

static SIDE_KINDS: Global<SideKinds> = Global::new(SideKinds {
    entries: BTreeMap::new(),
});

fn platform_kind(kind: FdKind) -> OfdKind {
    match kind {
        FdKind::File => OfdKind::File,
        FdKind::Directory => OfdKind::Dir,
        FdKind::Pipe => OfdKind::Pipe,
        FdKind::Tty => OfdKind::Tty,
        FdKind::Socket => OfdKind::Socket,
        // The platform fd table has no port kind; the side table remembers it.
        FdKind::Port | FdKind::Unknown => OfdKind::Unknown,
    }
}

fn logical_kind(kind: OfdKind) -> FdKind {
    match kind {
        OfdKind::File => FdKind::File,
        OfdKind::Dir => FdKind::Directory,
        OfdKind::Tty => FdKind::Tty,
        OfdKind::Socket => FdKind::Socket,
        OfdKind::Pipe => FdKind::Pipe,
        OfdKind::Timer => FdKind::Unknown,
        OfdKind::Epoll => FdKind::Unknown,
        OfdKind::EventFd => FdKind::Unknown,
        OfdKind::ProcessFd | OfdKind::Unknown => FdKind::Unknown,
    }
}

/// The real process fd table.
pub(crate) struct RealFds;

impl FdTable for RealFds {
    fn handle_of(&self, fd: i32) -> Result<Handle, Errno> {
        platform().get(fd).map(|entry| entry.handle())
    }

    fn install(&mut self, handle: Handle, kind: FdKind, cloexec: bool) -> Result<i32, Errno> {
        let fd = platform().install(handle, platform_kind(kind), 0, None, cloexec)?;
        SIDE_KINDS.with(|side| {
            side.entries.insert(fd, (handle, kind));
        });
        Ok(fd)
    }

    fn kind_of(&self, fd: i32) -> Result<FdKind, Errno> {
        let entry = platform().get(fd)?;
        let handle = entry.handle();
        let kind = entry.kind();
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
}

/// The saved auxiliary vector; `PG-50`/`PG-51` do not publish the three keys yet.
pub(crate) struct RealAux;

impl AuxSrc for RealAux {
    fn get(&self, _key: u64) -> Option<u64> {
        // C61's start retains only AT_PAGESZ, AT_CLKTCK, AT_RANDOM and AT_SECURE. AT_EXECFN,
        // AT_HWCAP and AT_HWCAP2 arrive with PG-50/PG-51; until then the documented absent-key
        // answers apply (`getexecname` NULL, `getisax` 0 words).
        None
    }
}

/// The thread runtime before C57: no thread can be created and no lock can be taken.
pub(crate) struct RealThreads;

impl PosixThreads for RealThreads {
    fn create(
        &mut self,
        _attr: &ThreadAttrPlan,
        _start: StartFn,
        _arg: usize,
    ) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn join(&mut self, _t: usize) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn self_(&self) -> usize {
        // The only thread exists but has no pthread identity before C57.
        0
    }

    fn is_initial(&self) -> bool {
        // A process without a thread runtime is exactly its initial thread.
        true
    }

    fn yield_(&mut self) {
        // sched_yield is the kernel YIELD; it needs no thread runtime.
        let _ = raven_libc_sys::sys::yield_now();
    }

    fn kill(&mut self, _t: usize, _sig: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sigmask(
        &mut self,
        _how: i32,
        _set: Option<&SigSet>,
        _old: Option<&mut SigSet>,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn exit(&mut self, _status: usize) -> ! {
        // With no thread runtime the calling thread is the process; pthread_exit ends it.
        raven_libc_sys::sys::exit(0)
    }

    fn mutex_init(&mut self, _m: &mut MutexT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_lock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_trylock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_unlock(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn mutex_destroy(&mut self, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_init(&mut self, _c: &mut CondT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_wait(&mut self, _c: &mut CondT, _m: &mut MutexT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_timedwait(
        &mut self,
        _c: &mut CondT,
        _m: &mut MutexT,
        _abstime: &Timespec,
    ) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_signal(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_broadcast(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn cond_destroy(&mut self, _c: &mut CondT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_init(&mut self, _r: &mut RwlockT, _pshared: bool) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_rdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_wrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_tryrdlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_trywrlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_unlock(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn rwlock_destroy(&mut self, _r: &mut RwlockT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_init(&mut self, _s: &mut SemaT, _pshared: bool, _count: u32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_destroy(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_wait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_trywait(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn sem_post(&mut self, _s: &mut SemaT) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn key_create(&mut self, _dtor: Option<KeyDtor>) -> Result<u32, Errno> {
        Err(Errno::ENOSYS)
    }

    fn setspecific(&mut self, _key: u32, _value: usize) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }

    fn getspecific(&mut self, _key: u32) -> Result<usize, Errno> {
        Err(Errno::ENOSYS)
    }

    fn getprio(&mut self, _t: usize) -> Result<i32, Errno> {
        Err(Errno::ENOSYS)
    }

    fn setprio(&mut self, _t: usize, _prio: i32) -> Result<(), Errno> {
        Err(Errno::ENOSYS)
    }
}

/// The concurrency hint as a plain value for the pure `thr_setconcurrency` function.
pub(crate) fn concurrency() -> i32 {
    CONCURRENCY.load(Ordering::Relaxed)
}

/// Store the concurrency hint.
pub(crate) fn set_concurrency(value: i32) {
    CONCURRENCY.store(value, Ordering::Relaxed);
}

/// Read a C `timespec`.
pub(crate) fn timespec(pointer: *const Timespec) -> Option<Timespec> {
    // SAFETY: the caller's pointer is NULL or a readable `struct timespec` (C prototype).
    unsafe { crate::personality_common::timespec(pointer) }
}

/// The `port` module's error result as `errno`/`-1`.
pub(crate) fn ret_port(value: Result<(), Errno>) -> core::ffi::c_int {
    crate::personality_common::ret_unit(value)
}

/// `EINVAL` with the `errno`/`-1` convention for a null output pointer.
pub(crate) fn invalid() -> core::ffi::c_int {
    crate::ffi::errno::set_raw(raven_libc_abi::errno::EINVAL);
    -1
}

/// The `port_create` result as `fd` or `errno`/`-1`.
pub(crate) fn ret_fd(value: Result<i32, Errno>) -> core::ffi::c_int {
    crate::personality_common::ret_int(value)
}

/// A thread wrapper's error number as the C `int`.
pub(crate) fn ret_code(value: i32) -> core::ffi::c_int {
    value
}
