// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Constants and shared scalar types.
//!
//! The error numbers are ravenlibc's (Linux values) but pthread functions return them directly:
//! they never set `errno` (4.14). The object sizes are C36's frozen table and are const-asserted
//! next to each wrapper type.

pub use raven_libc_abi::errno::Errno;
pub use raven_libc_abi::structs::Timespec;

/// `EINVAL`.
pub const EINVAL: Errno = Errno(raven_libc_abi::errno::EINVAL as u16);
/// `EAGAIN`.
pub const EAGAIN: Errno = Errno(raven_libc_abi::errno::EAGAIN as u16);
/// `EBUSY`.
pub const EBUSY: Errno = Errno(raven_libc_abi::errno::EBUSY as u16);
/// `EDEADLK`.
pub const EDEADLK: Errno = Errno(raven_libc_abi::errno::EDEADLK as u16);
/// `EPERM`.
pub const EPERM: Errno = Errno(raven_libc_abi::errno::EPERM as u16);
/// `ESRCH`.
pub const ESRCH: Errno = Errno(raven_libc_abi::errno::ESRCH as u16);
/// `EINTR`.
pub const EINTR: Errno = Errno(raven_libc_abi::errno::EINTR as u16);
/// `ETIMEDOUT`.
pub const ETIMEDOUT: Errno = Errno(raven_libc_abi::errno::ETIMEDOUT as u16);
/// `ENOTSUP`.
pub const ENOTSUP: Errno = Errno(raven_libc_abi::errno::ENOTSUP as u16);
/// `ENOSYS`.
pub const ENOSYS: Errno = Errno(raven_libc_abi::errno::ENOSYS as u16);
/// `EOWNERDEAD`.
pub const EOWNERDEAD: Errno = Errno(raven_libc_abi::errno::EOWNERDEAD as u16);
/// `ENOTRECOVERABLE`.
pub const ENOTRECOVERABLE: Errno = Errno(raven_libc_abi::errno::ENOTRECOVERABLE as u16);
/// `ECANCELED`.
pub const ECANCELED: Errno = Errno(raven_libc_abi::errno::ECANCELED as u16);
/// `EOVERFLOW`.
pub const EOVERFLOW: Errno = Errno(raven_libc_abi::errno::EOVERFLOW as u16);
/// `ERANGE`.
pub const ERANGE: Errno = Errno(raven_libc_abi::errno::ERANGE as u16);
/// `ENOMEM`.
pub const ENOMEM: Errno = Errno(raven_libc_abi::errno::ENOMEM as u16);
/// `ENOEXEC`.
pub const ENOEXEC: Errno = Errno(raven_libc_abi::errno::ENOEXEC as u16);
/// `EEXIST`.
pub const EEXIST: Errno = Errno(raven_libc_abi::errno::EEXIST as u16);
/// `ENOENT`.
pub const ENOENT: Errno = Errno(raven_libc_abi::errno::ENOENT as u16);
/// `ENAMETOOLONG`.
pub const ENAMETOOLONG: Errno = Errno(raven_libc_abi::errno::ENAMETOOLONG as u16);

/// `PTHREAD_KEYS_MAX` (the C36 value).
pub const PTHREAD_KEYS_MAX: usize = raven_libc_tls::tcb::TSD_WORDS;
/// Destructor rounds at thread exit.
pub const PTHREAD_DESTRUCTOR_ITERATIONS: usize = 4;
/// Minimum application stack size.
pub const PTHREAD_STACK_MIN: usize = 16_384;
/// Default stack size (OPEN-thr-10).
pub const DEFAULT_STACK_SIZE: usize = 8 * 1024 * 1024;
/// Default guard size (OPEN-thr-10).
pub const DEFAULT_GUARD_SIZE: usize = 4096;
/// Upper bound applied by `pthread_setattr_default_np`.
pub const DEFAULT_STACK_MAX: usize = 64 * 1024 * 1024;
/// Upper bound applied by `pthread_setattr_default_np`.
pub const DEFAULT_GUARD_MAX: usize = 1024 * 1024;
/// `SEM_VALUE_MAX`.
pub const SEM_VALUE_MAX: i32 = 0x7fff_ffff;
/// `PTHREAD_BARRIER_SERIAL_THREAD`.
pub const PTHREAD_BARRIER_SERIAL_THREAD: i32 = -1;
/// The value `pthread_join` returns for a cancelled thread.
pub const PTHREAD_CANCELED: usize = usize::MAX;
/// Maximum number of atfork handlers in the fixed table.
pub const ATFORK_MAX: usize = 128;
/// LIFO stack cache capacity.
pub const STACK_CACHE_MAX: usize = 16;
/// Maximum number of cooperative registry entries.
pub const REGISTRY_MAX: usize = 32;
/// Maximum number of zomble stacks held when `exit_ex` is absent.
pub const ZOMBIE_MAX: usize = STACK_CACHE_MAX;

/// `CLOCK_REALTIME`.
pub const CLOCK_REALTIME: u32 = 0;
/// `CLOCK_MONOTONIC`.
pub const CLOCK_MONOTONIC: u32 = 1;

/// `PTHREAD_MUTEX_NORMAL`.
pub const PTHREAD_MUTEX_NORMAL: i32 = 0;
/// `PTHREAD_MUTEX_RECURSIVE`.
pub const PTHREAD_MUTEX_RECURSIVE: i32 = 1;
/// `PTHREAD_MUTEX_ERRORCHECK`.
pub const PTHREAD_MUTEX_ERRORCHECK: i32 = 2;
/// `PTHREAD_MUTEX_ROBUST`.
pub const PTHREAD_MUTEX_ROBUST: i32 = 1;
/// `PTHREAD_MUTEX_STALLED`.
pub const PTHREAD_MUTEX_STALLED: i32 = 0;
/// `PTHREAD_PROCESS_PRIVATE`.
pub const PTHREAD_PROCESS_PRIVATE: i32 = 0;
/// `PTHREAD_PROCESS_SHARED`.
pub const PTHREAD_PROCESS_SHARED: i32 = 1;
/// `PTHREAD_PRIO_NONE`.
pub const PTHREAD_PRIO_NONE: i32 = 0;
/// `PTHREAD_PRIO_INHERIT`.
pub const PTHREAD_PRIO_INHERIT: i32 = 1;
/// `PTHREAD_PRIO_PROTECT`.
pub const PTHREAD_PRIO_PROTECT: i32 = 2;
/// `PTHREAD_SCOPE_SYSTEM`.
pub const PTHREAD_SCOPE_SYSTEM: i32 = 0;
/// `PTHREAD_SCOPE_PROCESS`.
pub const PTHREAD_SCOPE_PROCESS: i32 = 1;
/// `PTHREAD_CREATE_JOINABLE`.
pub const PTHREAD_CREATE_JOINABLE: i32 = 0;
/// `PTHREAD_CREATE_DETACHED`.
pub const PTHREAD_CREATE_DETACHED: i32 = 1;
/// `PTHREAD_INHERIT_SCHED`.
pub const PTHREAD_INHERIT_SCHED: i32 = 0;
/// `PTHREAD_EXPLICIT_SCHED`.
pub const PTHREAD_EXPLICIT_SCHED: i32 = 1;
/// `PTHREAD_CANCEL_ENABLE`.
pub const PTHREAD_CANCEL_ENABLE: i32 = 0;
/// `PTHREAD_CANCEL_DISABLE`.
pub const PTHREAD_CANCEL_DISABLE: i32 = 1;
/// `PTHREAD_CANCEL_MASKED`.
pub const PTHREAD_CANCEL_MASKED: i32 = 2;
/// `PTHREAD_CANCEL_DEFERRED`.
pub const PTHREAD_CANCEL_DEFERRED: i32 = 0;
/// `PTHREAD_CANCEL_ASYNCHRONOUS`.
pub const PTHREAD_CANCEL_ASYNCHRONOUS: i32 = 1;
/// `SCHED_OTHER`.
pub const SCHED_OTHER: i32 = 0;
/// `SCHED_FIFO`.
pub const SCHED_FIFO: i32 = 1;
/// `SCHED_RR`.
pub const SCHED_RR: i32 = 2;

/// Lock-word masks (musl `pthread_impl.h` and `pthread_mutex_*`).
pub const OWNER_MASK: u32 = 0x3fff_ffff;
/// Bit 31: waiters are present.
pub const WAITERS_BIT: u32 = 0x8000_0000;
/// Bit 30: the previous owner died holding the robust mutex.
pub const OWNER_DIED: u32 = 0x4000_0000;
/// The not-recoverable lock value.
pub const MUTEX_UNRECOVERABLE: u32 = 0x7fff_ffff;
/// Type bits: kind mask.
pub const MUTEX_TYPE_MASK: i32 = 15;
/// Type bit: robust.
pub const MUTEX_TYPE_ROBUST: i32 = 4;
/// Type bit: priority inheritance.
pub const MUTEX_TYPE_PI: i32 = 8;
/// Type bit: process-shared.
pub const MUTEX_TYPE_SHARED: i32 = 128;

/// The cancel word bits (4.17).
pub const CANCEL_ENABLE: u32 = 1;
/// Asynchronous cancellation.
pub const CANCEL_ASYNC: u32 = 2;
/// A cancellation request is pending.
pub const CANCEL_REQUESTED: u32 = 4;
/// A cancellation point masks cancellation while it waits.
pub const CANCEL_MASKED: u32 = 8;

/// Nanoseconds in one second.
pub const NS_PER_SEC: u64 = 1_000_000_000;
/// Nanoseconds in one microsecond.
pub const NS_PER_MS: u64 = 1_000_000;

/// Validate a `timespec` as an absolute deadline and return nanoseconds.
#[must_use]
pub fn timespec_to_ns(ts: Timespec) -> Option<u64> {
    if ts.tv_sec < 0 || ts.tv_nsec < 0 || ts.tv_nsec >= NS_PER_SEC as i64 {
        return None;
    }
    let sec = u64::try_from(ts.tv_sec).ok()?;
    let nsec = u64::try_from(ts.tv_nsec).ok()?;
    sec.checked_mul(NS_PER_SEC)?.checked_add(nsec)
}

/// The relative nanoseconds until `deadline` from `now`, or `None` when it has passed.
///
/// A result of zero is never returned: the kernel reads `timeout_ns == 0` as forever (A-thr-5).
#[must_use]
pub fn relative_ns(now_ns: u64, deadline_ns: u64) -> Option<u64> {
    let remaining = deadline_ns.checked_sub(now_ns)?;
    if remaining == 0 {
        None
    } else {
        Some(remaining)
    }
}

#[cfg(test)]
#[path = "limits_ut.rs"]
mod tests;
