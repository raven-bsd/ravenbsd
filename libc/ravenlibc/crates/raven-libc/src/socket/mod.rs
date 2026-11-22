// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The socket C ABI over C63's `raven-libc-net` translation layer.
//!
//! The process owns one `Net` object: C63's socket records are process state and every public
//! method takes `&self`. C55's `Platform` provides the fd table and the scheme backend; the
//! `NetEnv` clock/entropy seam uses the sys wrappers.

use core::cell::UnsafeCell;
use core::ffi::c_int;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicU8, Ordering};

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::Msghdr;
use raven_libc_net::NetEnv;
use raven_libc_net::backend_platform::PlatformBackend;
use raven_libc_net::fdglue::FdGlue;
use raven_libc_net::{Net, SendErr};
use raven_libc_platform::backend::RealSys;

use crate::ffi::platform::ShellLock;

mod accept;
mod duplex;
mod getpeereid;
mod getsockname;
#[path = "socket.rs"]
mod listen;
mod recv;
mod sendmmsg;
mod socketpair;

use duplex::ShellUnix;

/// The C `socklen_t`.
pub(crate) type SockLen = core::ffi::c_uint;

/// The process clock and entropy seam.
pub(crate) struct ShellEnv;

impl NetEnv for ShellEnv {
    fn monotonic_ms(&self) -> u64 {
        match raven_libc_sys::sys::clock_get(raven_libc_abi::time::CLOCK_MONOTONIC as u32) {
            Ok(time) => (time.tv_sec.max(0) as u64)
                .saturating_mul(1000)
                .saturating_add(time.tv_nsec.max(0) as u64 / 1_000_000),
            Err(_) => 0,
        }
    }

    fn entropy(&self, buf: &mut [u8]) -> bool {
        raven_libc_sys::pending::getrandom(buf, 0)
            .map(|count| count == buf.len())
            .unwrap_or(false)
    }

    fn now_secs(&self) -> i64 {
        raven_libc_sys::sys::clock_get(raven_libc_abi::time::CLOCK_REALTIME as u32)
            .map(|time| time.tv_sec)
            .unwrap_or(0)
    }
}

pub(crate) type ShellNet = Net<
    PlatformBackend<'static, RealSys, ShellLock>,
    FdGlue<'static, RealSys, ShellLock>,
    ShellEnv,
    ShellUnix,
>;

const UNINIT: u8 = 0;
const READY: u8 = 2;

struct Global(UnsafeCell<MaybeUninit<ShellNet>>);

// SAFETY: the shell is single-threaded until C57's thread support is wired; the object is
// written once before READY is published with Release, and readers only obtain `&Net` after an
// Acquire load. `Net`'s interior `RefCell`s are the C63 contract (one thread at a time).
unsafe impl Sync for Global {}

static STATE: AtomicU8 = AtomicU8::new(UNINIT);
static NET: Global = Global(UnsafeCell::new(MaybeUninit::uninit()));

/// The process-wide socket translation object.
pub(crate) fn net() -> &'static ShellNet {
    if STATE
        .compare_exchange(UNINIT, 1, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
    {
        let platform = crate::ffi::platform::global();
        // SAFETY: this caller won the one-shot initialisation and owns the slot.
        unsafe {
            (&mut *NET.0.get()).write(Net::with_unix(
                PlatformBackend::new(platform),
                FdGlue::new(platform),
                ShellEnv,
                Some(ShellUnix),
            ));
        }
        STATE.store(READY, Ordering::Release);
    }
    while STATE.load(Ordering::Acquire) != READY {
        core::hint::spin_loop();
    }
    // SAFETY: READY was published after the write and the storage is a static.
    unsafe { (&*NET.0.get()).assume_init_ref() }
}

/// Run `f` with the process socket object.
pub(crate) fn with_net<R>(f: impl FnOnce(&ShellNet) -> R) -> R {
    f(net())
}

/// Refresh the socket records' cached handles after the child's fork hook duplicated them.
pub(crate) fn refresh_after_fork() {
    with_net(|net| net.refresh_handles());
}

/// Publish the live socket records to the platform before `execve` serializes the fd map.
pub(crate) fn sync_exec_meta() {
    with_net(|net| net.sync_meta());
}

/// The ABI `struct mmsghdr`.
#[repr(C)]
pub(crate) struct Mmsghdr {
    pub(crate) header: Msghdr,
    pub(crate) len: u32,
}

pub(crate) fn fault() -> Errno {
    Errno(EFAULT as u16)
}

pub(crate) fn send_result(result: Result<usize, SendErr>) -> isize {
    match result {
        Ok(count) => isize::try_from(count).unwrap_or(isize::MAX),
        Err(error) => {
            if error.raise_sigpipe {
                let _ = raven_libc_signal::send::raise(
                    &raven_libc_signal::SysKernel,
                    &crate::signal::DISP,
                    raven_libc_abi::signal::SIGPIPE,
                );
            }
            crate::ffi::ret::ssize(Err(error.errno))
        }
    }
}

pub(crate) fn socket_result(result: Result<c_int, Errno>) -> c_int {
    crate::ffi::ret::int(result)
}

pub(crate) fn unit_result(result: Result<(), Errno>) -> c_int {
    crate::ffi::ret::unit(result)
}
