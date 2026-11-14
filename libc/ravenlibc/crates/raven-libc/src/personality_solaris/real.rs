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
use raven_libc_thread::ThreadKernel;
use raven_libc_thread::cond::{self, Cond};
use raven_libc_thread::limits::{
    PTHREAD_CREATE_DETACHED, PTHREAD_PROCESS_PRIVATE, PTHREAD_PROCESS_SHARED, SCHED_OTHER,
};
use raven_libc_thread::mutex::{self, Mutex};
use raven_libc_thread::raw::SysKernel;
use raven_libc_thread::rwlock::{self, RwLock};
use raven_libc_thread::sem::{self, Sem};
use raven_libc_thread::thread;

use raven_libc_personality_solaris::port::PortTable;
use raven_libc_personality_solaris::seams::{
    AuxSrc, CondT, DoorbellOps, EvqCtl, EvqEvent, EvqOps, FdKind, FdTable, KeyDtor, MutexT,
    PosixThreads, RwlockT, SemaT, SigSet, StackPlan, StartFn, ThreadAttrPlan,
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

/// The saved auxiliary vector: C61's start path records `AT_EXECFN`, `AT_HWCAP` and `AT_HWCAP2`
/// with their presence bits, so both identity calls answer from the loader's values.
pub(crate) struct RealAux;

impl AuxSrc for RealAux {
    fn get(&self, key: u64) -> Option<u64> {
        let key = usize::try_from(key).ok()?;
        crate::start::saved_aux(key).map(|value| value as u64)
    }
}

fn thread_error(error: raven_libc_thread::limits::Errno) -> Errno {
    Errno(error.0)
}

fn as_mutex(value: &mut MutexT) -> &mut Mutex {
    // SAFETY: MutexT is the frozen 40-byte pthread_mutex_t and Mutex is its repr(C, align(8))
    // owner; the const assert below pins the sizes.
    unsafe { &mut *(core::ptr::from_mut(value).cast::<Mutex>()) }
}

fn as_cond(value: &mut CondT) -> &mut Cond {
    // SAFETY: as_mutex; both objects are 48 bytes with 8-byte alignment.
    unsafe { &mut *(core::ptr::from_mut(value).cast::<Cond>()) }
}

fn as_rwlock(value: &mut RwlockT) -> &mut RwLock {
    // SAFETY: as_mutex; both objects are 56 bytes with 8-byte alignment.
    unsafe { &mut *(core::ptr::from_mut(value).cast::<RwLock>()) }
}

fn as_sem(value: &mut SemaT) -> &mut Sem {
    // SAFETY: as_mutex; both objects are 32 bytes with 4-byte alignment.
    unsafe { &mut *(core::ptr::from_mut(value).cast::<Sem>()) }
}

const _: () = assert!(core::mem::size_of::<MutexT>() == core::mem::size_of::<Mutex>());
const _: () = assert!(core::mem::size_of::<CondT>() == core::mem::size_of::<Cond>());
const _: () = assert!(core::mem::size_of::<RwlockT>() == core::mem::size_of::<RwLock>());
const _: () = assert!(core::mem::size_of::<SemaT>() == core::mem::size_of::<Sem>());

/// The real POSIX thread runtime: every wrapper calls the merged `raven-libc-thread` crate.
///
/// `thr_create` and `thr_kill` still carry the kernel gaps their C57 seams document: thread
/// creation is `ENOSYS` until PG-2 and a kick is `ENOSYS` until PG-7. The wrappers do not hide
/// those errors, and no second thread implementation exists here.
pub(crate) struct RealThreads;

impl PosixThreads for RealThreads {
    fn create(
        &mut self,
        plan: &ThreadAttrPlan,
        start: StartFn,
        arg: usize,
    ) -> Result<usize, Errno> {
        let kernel = SysKernel::new();
        let mut attributes = thread::attr::init();
        if plan.detached {
            thread::attr::setdetachstate(&mut attributes, PTHREAD_CREATE_DETACHED)
                .map_err(thread_error)?;
        }
        match plan.stack {
            StackPlan::Default => {}
            StackPlan::SetStack { base, size } => {
                thread::attr::setstack(&mut attributes, base, size).map_err(thread_error)?;
            }
            StackPlan::SetSize(size) => {
                thread::attr::setstacksize(&mut attributes, size).map_err(thread_error)?;
            }
        }
        // SAFETY: C67 5.A adapts the C `void *(*)(void *)` to the Rust `fn(usize) -> usize`
        // the thread crate stores; both pass one integer argument and return one integer.
        let start: fn(usize) -> usize =
            unsafe { core::mem::transmute::<StartFn, fn(usize) -> usize>(start) };
        thread::create(&kernel, Some(&attributes), start, arg).map_err(thread_error)
    }

    fn join(&mut self, t: usize) -> Result<usize, Errno> {
        thread::join::join(&SysKernel::new(), t).map_err(thread_error)
    }

    fn self_(&self) -> usize {
        thread::join::self_thread(&SysKernel::new())
    }

    fn is_initial(&self) -> bool {
        // PG-2 is absent, so no second thread can exist on the target; the count is exact for
        // the current kernel and the only state the shell can observe without a C57 accessor.
        raven_libc_thread::state_probe::threads() == 1
    }

    fn yield_(&mut self) {
        SysKernel::new().yield_now();
    }

    fn kill(&mut self, t: usize, sig: i32) -> Result<(), Errno> {
        thread::kill::kill(&SysKernel::new(), t, sig).map_err(thread_error)
    }

    fn sigmask(
        &mut self,
        how: i32,
        set: Option<&SigSet>,
        old: Option<&mut SigSet>,
    ) -> Result<(), Errno> {
        let kernel = raven_libc_signal::kernel::SysKernel::new();
        let requested = set.map(|value| raven_libc_signal::sigset::SigSet(*value));
        let mut previous = old
            .as_ref()
            .map(|_| raven_libc_signal::sigset::SigSet([0; 16]));
        let result = raven_libc_signal::maskops::pthread_sigmask(
            &kernel,
            how,
            requested.as_ref(),
            previous.as_mut(),
        );
        if result != 0 {
            return Err(Errno(u16::try_from(result).unwrap_or(22)));
        }
        if let (Some(out), Some(value)) = (old, previous) {
            *out = value.0;
        }
        Ok(())
    }

    fn exit(&mut self, status: usize) -> ! {
        thread::exit(&SysKernel::new(), status)
    }

    fn mutex_init(&mut self, m: &mut MutexT, pshared: bool) -> Result<(), Errno> {
        let kernel = SysKernel::new();
        let mut attributes = mutex::attr_init();
        mutex::attr_setpshared(
            &kernel,
            &mut attributes,
            if pshared {
                PTHREAD_PROCESS_SHARED
            } else {
                PTHREAD_PROCESS_PRIVATE
            },
        )
        .map_err(thread_error)?;
        mutex::init(as_mutex(m), Some(&attributes));
        Ok(())
    }

    fn mutex_lock(&mut self, m: &mut MutexT) -> Result<(), Errno> {
        mutex::lock(&SysKernel::new(), as_mutex(m)).map_err(thread_error)
    }

    fn mutex_trylock(&mut self, m: &mut MutexT) -> Result<(), Errno> {
        mutex::trylock(&SysKernel::new(), as_mutex(m)).map_err(thread_error)
    }

    fn mutex_unlock(&mut self, m: &mut MutexT) -> Result<(), Errno> {
        mutex::unlock(&SysKernel::new(), as_mutex(m)).map_err(thread_error)
    }

    fn mutex_destroy(&mut self, m: &mut MutexT) -> Result<(), Errno> {
        mutex::destroy(&SysKernel::new(), as_mutex(m)).map_err(thread_error)
    }

    fn cond_init(&mut self, c: &mut CondT, pshared: bool) -> Result<(), Errno> {
        let kernel = SysKernel::new();
        let mut attributes = cond::attr_init();
        cond::attr_setpshared(
            &kernel,
            &mut attributes,
            if pshared {
                PTHREAD_PROCESS_SHARED
            } else {
                PTHREAD_PROCESS_PRIVATE
            },
        )
        .map_err(thread_error)?;
        cond::init(as_cond(c), Some(&attributes));
        Ok(())
    }

    fn cond_wait(&mut self, c: &mut CondT, m: &mut MutexT) -> Result<(), Errno> {
        cond::wait(&SysKernel::new(), as_cond(c), as_mutex(m)).map_err(thread_error)
    }

    fn cond_timedwait(
        &mut self,
        c: &mut CondT,
        m: &mut MutexT,
        abstime: &Timespec,
    ) -> Result<(), Errno> {
        let condition = as_cond(c);
        let clock = condition.clock();
        cond::timedwait(
            &SysKernel::new(),
            condition,
            as_mutex(m),
            clock,
            Some(*abstime),
        )
        .map_err(thread_error)
    }

    fn cond_signal(&mut self, c: &mut CondT) -> Result<(), Errno> {
        cond::signal(&SysKernel::new(), as_cond(c)).map_err(thread_error)
    }

    fn cond_broadcast(&mut self, c: &mut CondT) -> Result<(), Errno> {
        cond::broadcast(&SysKernel::new(), as_cond(c)).map_err(thread_error)
    }

    fn cond_destroy(&mut self, c: &mut CondT) -> Result<(), Errno> {
        cond::destroy(as_cond(c)).map_err(thread_error)
    }

    fn rwlock_init(&mut self, r: &mut RwlockT, pshared: bool) -> Result<(), Errno> {
        let kernel = SysKernel::new();
        let mut attributes = rwlock::attr_init();
        rwlock::attr_setpshared(
            &kernel,
            &mut attributes,
            if pshared {
                PTHREAD_PROCESS_SHARED
            } else {
                PTHREAD_PROCESS_PRIVATE
            },
        )
        .map_err(thread_error)?;
        rwlock::init(as_rwlock(r), Some(&attributes));
        Ok(())
    }

    fn rwlock_rdlock(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::rdlock(&SysKernel::new(), as_rwlock(r)).map_err(thread_error)
    }

    fn rwlock_wrlock(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::wrlock(&SysKernel::new(), as_rwlock(r)).map_err(thread_error)
    }

    fn rwlock_tryrdlock(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::tryrdlock(as_rwlock(r)).map_err(thread_error)
    }

    fn rwlock_trywrlock(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::trywrlock(as_rwlock(r)).map_err(thread_error)
    }

    fn rwlock_unlock(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::unlock(&SysKernel::new(), as_rwlock(r)).map_err(thread_error)
    }

    fn rwlock_destroy(&mut self, r: &mut RwlockT) -> Result<(), Errno> {
        rwlock::destroy(as_rwlock(r));
        Ok(())
    }

    fn sem_init(&mut self, s: &mut SemaT, pshared: bool, count: u32) -> Result<(), Errno> {
        sem::init(
            &SysKernel::new(),
            as_sem(s),
            if pshared {
                PTHREAD_PROCESS_SHARED
            } else {
                PTHREAD_PROCESS_PRIVATE
            },
            count,
        )
        .map_err(thread_error)
    }

    fn sem_destroy(&mut self, s: &mut SemaT) -> Result<(), Errno> {
        sem::destroy(as_sem(s)).map_err(thread_error)
    }

    fn sem_wait(&mut self, s: &mut SemaT) -> Result<(), Errno> {
        sem::wait(&SysKernel::new(), as_sem(s)).map_err(thread_error)
    }

    fn sem_trywait(&mut self, s: &mut SemaT) -> Result<(), Errno> {
        sem::trywait(as_sem(s)).map_err(thread_error)
    }

    fn sem_post(&mut self, s: &mut SemaT) -> Result<(), Errno> {
        sem::post(&SysKernel::new(), as_sem(s)).map_err(thread_error)
    }

    fn key_create(&mut self, dtor: Option<KeyDtor>) -> Result<u32, Errno> {
        // SAFETY: a C destructor and a Rust `fn(usize)` both take one integer and return ();
        // the thread crate stores the address and calls it with the TSD value.
        let destructor: Option<fn(usize)> =
            unsafe { core::mem::transmute::<Option<KeyDtor>, Option<fn(usize)>>(dtor) };
        thread::tsd::key_create(&SysKernel::new(), destructor).map_err(thread_error)
    }

    fn setspecific(&mut self, key: u32, value: usize) -> Result<(), Errno> {
        thread::tsd::setspecific(&SysKernel::new(), key, value).map_err(thread_error)
    }

    fn getspecific(&mut self, key: u32) -> Result<usize, Errno> {
        Ok(thread::tsd::getspecific(&SysKernel::new(), key))
    }

    fn getprio(&mut self, t: usize) -> Result<i32, Errno> {
        thread::sched::getschedparam(&SysKernel::new(), t)
            .map(|(_, priority)| priority)
            .map_err(thread_error)
    }

    fn setprio(&mut self, t: usize, prio: i32) -> Result<(), Errno> {
        thread::sched::setschedparam(&SysKernel::new(), t, SCHED_OTHER, prio).map_err(thread_error)
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
