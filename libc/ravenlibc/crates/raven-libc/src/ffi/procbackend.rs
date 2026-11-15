// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shell's one process backend (C64 4.9 / P2a).
//!
//! On the target the identity syscalls are real: `PROC_SELF_INFO`, `PROC_SETPGID`, `PROC_SETSID`
//! and `PROC_SETCRED` are called through `raven-libc-sys`. Host tests and the pre-syscall
//! bootstrap keep `LocalBackend`. Nothing is created at start and `fork`/`exec`/`spawn` keep the
//! local backend's honest `ENOSYS` until `procd` exists.

use raven_libc_proc::local_backend::LocalBackend;
#[cfg(target_os = "none")]
use raven_libc_proc::syscall_backend::{SelfInfo, Syscall, SyscallBackend};

#[cfg(target_os = "none")]
use raven_libc_proc::syscall_backend::{
    PROC_PID_INFO, PROC_SELF_INFO, PROC_SETCRED, PROC_SETPGID, PROC_SETSID,
};

/// The process backend of the shell.
#[cfg(target_os = "none")]
pub(crate) fn backend() -> SyscallBackend<RealSyscall> {
    SyscallBackend::new(RealSyscall)
}

/// The process backend of the shell before the target syscall seam exists.
#[cfg(not(target_os = "none"))]
pub(crate) fn backend() -> LocalBackend<'static> {
    LocalBackend::new(None)
}

/// Whether a `procd` connection was handed over (`false` on this branch).
pub(crate) fn procd_present() -> bool {
    false
}

#[cfg(target_os = "none")]
pub(crate) struct RealSyscall;

#[cfg(target_os = "none")]
fn errno(error: raven_abi::errno::Errno) -> i32 {
    i32::from(error.0)
}

#[cfg(target_os = "none")]
fn decode_info(words: [u64; 8]) -> Result<SelfInfo, i32> {
    Ok(SelfInfo {
        pid: words[0],
        ppid: words[1],
        pgid: words[2],
        sid: words[3],
        uid: u32::try_from(words[4]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        euid: u32::try_from(words[5]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        gid: u32::try_from(words[6]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        egid: u32::try_from(words[7]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
    })
}

#[cfg(target_os = "none")]
impl Syscall for RealSyscall {
    fn self_info(&self) -> Result<SelfInfo, i32> {
        let mut words = [0u64; 8];
        // SAFETY: `words` is a live eight-word array; the kernel writes exactly eight u64 values
        // after validating the pointer and count.
        let raw = unsafe {
            raven_libc_sys::arch::syscall2(PROC_SELF_INFO, words.as_mut_ptr() as usize, words.len())
        };
        raven_libc_sys::errno::ret(raw).map_err(errno)?;
        decode_info(words)
    }

    fn pid_info(&self, pid: i32) -> Result<SelfInfo, i32> {
        let mut words = [0u64; 8];
        // SAFETY: `words` is a live eight-word array; PID_INFO validates the pid and writes
        // exactly eight u64 values.
        let raw = unsafe {
            raven_libc_sys::arch::syscall3(
                PROC_PID_INFO,
                pid as isize as usize,
                words.as_mut_ptr() as usize,
                words.len(),
            )
        };
        raven_libc_sys::errno::ret(raw).map_err(errno)?;
        decode_info(words)
    }

    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), i32> {
        // SAFETY: both arguments are scalar i32 values sign-extended into the register.
        let raw = unsafe {
            raven_libc_sys::arch::syscall2(
                PROC_SETPGID,
                pid as isize as usize,
                pgid as isize as usize,
            )
        };
        raven_libc_sys::errno::ret(raw).map(|_| ()).map_err(errno)
    }

    fn setsid(&self) -> Result<i32, i32> {
        // SAFETY: `setsid` takes no arguments.
        let raw = unsafe { raven_libc_sys::arch::syscall0(PROC_SETSID) };
        raven_libc_sys::errno::ret_i32(raw).map_err(errno)
    }

    fn setcred(&self, which: u32, value: u32) -> Result<(), i32> {
        // SAFETY: both arguments are scalar u32 values.
        let raw =
            unsafe { raven_libc_sys::arch::syscall2(PROC_SETCRED, which as usize, value as usize) };
        raven_libc_sys::errno::ret(raw).map(|_| ()).map_err(errno)
    }
}
