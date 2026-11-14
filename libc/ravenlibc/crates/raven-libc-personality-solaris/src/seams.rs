// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The narrow seams between the Solaris personality logic and the C-ABI shell.
//!
//! The shell implements each trait over `raven-libc-platform`, `raven-libc-sys` and (when C57
//! merges) `raven-libc-thread`; the host tests in `crate::testutil` implement them
//! deterministically. A primitive that does not exist yet returns the documented error from the
//! trait implementation, never a silent success (`11` 3.8).
//!
//! `EvqOps` and `FdTable` repeat the shape of C66 (`raven-libc-personality-linux`) because the
//! personalities are independent siblings (C66 4.2); a later platform ticket may replace both
//! copies with one shared trait. `PosixThreads` is the ONLY way [`crate::thread_names`] reaches
//! the thread runtime (C57 4.2); no second thread implementation exists here.
//!
//! CLEANROOM: `06-solaris-design.md` A-sol-4, A-sol-5 and C67 4.2.

pub use raven_libc_abi::structs::Timespec;
pub use raven_libc_platform::Errno;
pub use raven_libc_platform::Handle;
pub use raven_libc_sys::sys::EvqEvent;

/// The `sigset_t` shape used by `thr_sigsetmask`.
///
/// `raven-libc-signal` (C58) is not on this branch, so the frozen LP64 layout of the installed
/// `<signal.h>` is repeated: 128 bytes, sixteen 64-bit words (doc 14 T3). The shell header
/// defines the same layout.
pub type SigSet = [u64; 16];

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
    /// An event port (C67 4.3 rule 1).
    Port,
    /// Not known yet.
    Unknown,
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

/// `EVENT_CREATE`/`EVENT_SIGNAL`: the wake-up a `port_send` rings.
pub trait DoorbellOps {
    /// Create a user-event handle.
    fn create(&mut self) -> Result<Handle, Errno>;
    /// Signal the user event.
    fn signal(&mut self, h: Handle, value: u64) -> Result<(), Errno>;
}

/// The saved auxiliary-vector copy of `raven-libc-sys` (never a file, A-sol-4).
pub trait AuxSrc {
    /// The value of `key`, or `None` when the loader did not publish it.
    fn get(&self, key: u64) -> Option<u64>;
}

/// The stack part of a `thr_create` attribute plan (C67 5.A).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StackPlan {
    /// The platform default stack.
    Default,
    /// An explicit base and size (`pthread_attr_setstack`).
    SetStack {
        /// Stack base address.
        base: usize,
        /// Stack size in bytes.
        size: usize,
    },
    /// A size only (`pthread_attr_setstacksize`).
    SetSize(usize),
}

/// The attribute plan `thr_create` builds from its flags and stack arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadAttrPlan {
    /// `THR_DETACHED` was requested.
    pub detached: bool,
    /// The stack request.
    pub stack: StackPlan,
}

/// A thread start routine (`void *(*)(void *)` after the shell's adaptation).
pub type StartFn = extern "C" fn(usize) -> usize;

/// A thread-specific-data destructor.
pub type KeyDtor = extern "C" fn(usize);

/// The opaque `pthread_mutex_t` (40 bytes on LP64; C38's frozen layout).
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MutexT {
    /// The frozen representation; the logic crate never reads it.
    pub opaque: [u8; 40],
}

/// The opaque `pthread_cond_t` (48 bytes on LP64).
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CondT {
    /// The frozen representation; the logic crate never reads it.
    pub opaque: [u8; 48],
}

/// The opaque `pthread_rwlock_t` (56 bytes on LP64).
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RwlockT {
    /// The frozen representation; the logic crate never reads it.
    pub opaque: [u8; 56],
}

/// The opaque `sem_t` (32 bytes on LP64).
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SemaT {
    /// The frozen representation; the logic crate never reads it.
    pub opaque: [u8; 32],
}

impl MutexT {
    /// An all-zero mutex object.
    pub const ZERO: Self = Self { opaque: [0; 40] };
}

impl CondT {
    /// An all-zero condition variable.
    pub const ZERO: Self = Self { opaque: [0; 48] };
}

impl RwlockT {
    /// An all-zero reader/writer lock.
    pub const ZERO: Self = Self { opaque: [0; 56] };
}

impl SemaT {
    /// An all-zero semaphore.
    pub const ZERO: Self = Self { opaque: [0; 32] };
}

const _: () = assert!(core::mem::size_of::<MutexT>() == 40);
const _: () = assert!(core::mem::size_of::<CondT>() == 48);
const _: () = assert!(core::mem::size_of::<RwlockT>() == 56);
const _: () = assert!(core::mem::size_of::<SemaT>() == 32);

/// The POSIX thread runtime, exactly as `thread_names` needs it (C57 4.2).
///
/// One method per `pthread_*`/`sem_*` call the wrappers make, plus the three queries a valid
/// Solaris spelling needs and POSIX answers differently (`self_`, `is_initial`, `getprio`).
/// When `raven-libc-thread` merges, the shell implements this trait over its public API; until
/// then the shell implementation returns `ENOSYS` for every call that needs a thread runtime.
pub trait PosixThreads {
    /// `pthread_create` with the attribute plan.
    fn create(&mut self, attr: &ThreadAttrPlan, start: StartFn, arg: usize)
    -> Result<usize, Errno>;
    /// `pthread_join`.
    fn join(&mut self, t: usize) -> Result<usize, Errno>;
    /// `pthread_self`.
    fn self_(&self) -> usize;
    /// Whether the calling thread is the initial thread (`thr_main`).
    fn is_initial(&self) -> bool;
    /// `sched_yield`.
    fn yield_(&mut self);
    /// `pthread_kill`.
    fn kill(&mut self, t: usize, sig: i32) -> Result<(), Errno>;
    /// `pthread_sigmask`; `old` receives the previous mask when present.
    fn sigmask(
        &mut self,
        how: i32,
        set: Option<&SigSet>,
        old: Option<&mut SigSet>,
    ) -> Result<(), Errno>;
    /// `pthread_exit`.
    fn exit(&mut self, status: usize) -> !;
    /// `pthread_mutex_init` with the `pshared` attribute.
    fn mutex_init(&mut self, m: &mut MutexT, pshared: bool) -> Result<(), Errno>;
    /// `pthread_mutex_lock`.
    fn mutex_lock(&mut self, m: &mut MutexT) -> Result<(), Errno>;
    /// `pthread_mutex_trylock`.
    fn mutex_trylock(&mut self, m: &mut MutexT) -> Result<(), Errno>;
    /// `pthread_mutex_unlock`.
    fn mutex_unlock(&mut self, m: &mut MutexT) -> Result<(), Errno>;
    /// `pthread_mutex_destroy`.
    fn mutex_destroy(&mut self, m: &mut MutexT) -> Result<(), Errno>;
    /// `pthread_cond_init` with the `pshared` attribute.
    fn cond_init(&mut self, c: &mut CondT, pshared: bool) -> Result<(), Errno>;
    /// `pthread_cond_wait`.
    fn cond_wait(&mut self, c: &mut CondT, m: &mut MutexT) -> Result<(), Errno>;
    /// `pthread_cond_timedwait` with an absolute timespec.
    fn cond_timedwait(
        &mut self,
        c: &mut CondT,
        m: &mut MutexT,
        abstime: &Timespec,
    ) -> Result<(), Errno>;
    /// `pthread_cond_signal`.
    fn cond_signal(&mut self, c: &mut CondT) -> Result<(), Errno>;
    /// `pthread_cond_broadcast`.
    fn cond_broadcast(&mut self, c: &mut CondT) -> Result<(), Errno>;
    /// `pthread_cond_destroy`.
    fn cond_destroy(&mut self, c: &mut CondT) -> Result<(), Errno>;
    /// `pthread_rwlock_init` with the `pshared` attribute.
    fn rwlock_init(&mut self, r: &mut RwlockT, pshared: bool) -> Result<(), Errno>;
    /// `pthread_rwlock_rdlock`.
    fn rwlock_rdlock(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `pthread_rwlock_wrlock`.
    fn rwlock_wrlock(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `pthread_rwlock_tryrdlock`.
    fn rwlock_tryrdlock(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `pthread_rwlock_trywrlock`.
    fn rwlock_trywrlock(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `pthread_rwlock_unlock`.
    fn rwlock_unlock(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `pthread_rwlock_destroy`.
    fn rwlock_destroy(&mut self, r: &mut RwlockT) -> Result<(), Errno>;
    /// `sem_init` with the `pshared` attribute.
    fn sem_init(&mut self, s: &mut SemaT, pshared: bool, count: u32) -> Result<(), Errno>;
    /// `sem_destroy`.
    fn sem_destroy(&mut self, s: &mut SemaT) -> Result<(), Errno>;
    /// `sem_wait`.
    fn sem_wait(&mut self, s: &mut SemaT) -> Result<(), Errno>;
    /// `sem_trywait`.
    fn sem_trywait(&mut self, s: &mut SemaT) -> Result<(), Errno>;
    /// `sem_post`.
    fn sem_post(&mut self, s: &mut SemaT) -> Result<(), Errno>;
    /// `pthread_key_create`.
    fn key_create(&mut self, dtor: Option<KeyDtor>) -> Result<u32, Errno>;
    /// `pthread_setspecific`.
    fn setspecific(&mut self, key: u32, value: usize) -> Result<(), Errno>;
    /// `pthread_getspecific`.
    fn getspecific(&mut self, key: u32) -> Result<usize, Errno>;
    /// `pthread_getschedparam`'s priority for `t` (`TBD-11`: only 0 is supported).
    fn getprio(&mut self, t: usize) -> Result<i32, Errno>;
    /// `pthread_setschedparam`'s priority for `t` (`TBD-11`: only 0 is supported).
    fn setprio(&mut self, t: usize, prio: i32) -> Result<(), Errno>;
}

#[cfg(test)]
#[path = "seams_ut.rs"]
mod tests;
