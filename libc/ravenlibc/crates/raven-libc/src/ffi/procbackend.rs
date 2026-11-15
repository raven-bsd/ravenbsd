// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The shell's one process backend (C64 4.9 / P2a).
//!
//! On the target the identity syscalls are real: `PROC_SELF_INFO`, `PROC_SETPGID`, `PROC_SETSID`
//! and `PROC_SETCRED` are called through `raven-libc-sys`. Host tests and the pre-syscall
//! bootstrap keep `LocalBackend`. Nothing is created at start and `fork`/`exec`/`spawn` keep the
//! local backend's honest `ENOSYS` until `procd` exists.

#[cfg(target_os = "none")]
use core::ffi::c_int;

#[cfg(target_os = "none")]
use raven_libc_proc::backend::ProcBackend;
#[cfg(target_os = "none")]
use raven_libc_proc::errors::EFAULT;
#[cfg(target_os = "none")]
use raven_libc_proc::ids::SetCred;
use raven_libc_proc::local_backend::LocalBackend;
#[cfg(target_os = "none")]
use raven_libc_proc::syscall_backend::{CredInfo, SelfInfo, Syscall, SyscallBackend};

#[cfg(target_os = "none")]
use raven_libc_proc::syscall_backend::{
    PROC_CRED_INFO, PROC_GETGROUPS, PROC_PID_INFO, PROC_SELF_INFO, PROC_SETCRED_EX, PROC_SETPGID,
    PROC_SETSID,
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

/// `getresuid(2)`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> c_int {
    let result = backend().getresuid();
    crate::ffi::ret::int(match result {
        Ok((real, effective, saved)) if !ruid.is_null() && !euid.is_null() && !suid.is_null() => {
            // SAFETY: the three pointers are the caller's writable uid_t slots and were all
            // checked non-null before the stores.
            unsafe {
                *ruid = real;
                *euid = effective;
                *suid = saved;
            }
            Ok(0)
        }
        Ok(_) => Err(crate::ffi::map_proc::error(EFAULT)),
        Err(error) => Err(crate::ffi::map_proc::error(error)),
    })
}

/// `getresgid(2)`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> c_int {
    let result = backend().getresgid();
    crate::ffi::ret::int(match result {
        Ok((real, effective, saved)) if !rgid.is_null() && !egid.is_null() && !sgid.is_null() => {
            // SAFETY: the three pointers are the caller's writable gid_t slots and were all
            // checked non-null before the stores.
            unsafe {
                *rgid = real;
                *egid = effective;
                *sgid = saved;
            }
            Ok(0)
        }
        Ok(_) => Err(crate::ffi::map_proc::error(EFAULT)),
        Err(error) => Err(crate::ffi::map_proc::error(error)),
    })
}

/// `setresuid(2)`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn setresuid(ruid: u32, euid: u32, suid: u32) -> c_int {
    let request = SetCred::setresuid(ruid as i32, euid as i32, suid as i32);
    crate::ffi::ret::int(
        backend()
            .setcred(&request)
            .map(|()| 0)
            .map_err(crate::ffi::map_proc::error),
    )
}

/// `setresgid(2)`.
#[cfg(target_os = "none")]
#[unsafe(no_mangle)]
pub extern "C" fn setresgid(rgid: u32, egid: u32, sgid: u32) -> c_int {
    let request = SetCred::setresgid(rgid as i32, egid as i32, sgid as i32);
    crate::ffi::ret::int(
        backend()
            .setcred(&request)
            .map(|()| 0)
            .map_err(crate::ffi::map_proc::error),
    )
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
fn decode_cred(words: [u64; 6]) -> Result<CredInfo, i32> {
    Ok(CredInfo {
        uid: u32::try_from(words[0]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        euid: u32::try_from(words[1]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        suid: u32::try_from(words[2]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        gid: u32::try_from(words[3]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        egid: u32::try_from(words[4]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
        sgid: u32::try_from(words[5]).map_err(|_| i32::from(raven_libc_proc::errors::EOVERFLOW))?,
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

    fn cred_info(&self) -> Result<CredInfo, i32> {
        let mut words = [0u64; 6];
        // SAFETY: `words` is a live six-word array; the kernel writes exactly six u64 values.
        let raw = unsafe {
            raven_libc_sys::arch::syscall2(PROC_CRED_INFO, words.as_mut_ptr() as usize, words.len())
        };
        raven_libc_sys::errno::ret(raw).map_err(errno)?;
        decode_cred(words)
    }

    fn groups(&self, out: &mut [u32]) -> Result<usize, i32> {
        // SAFETY: the kernel copies at most `out.len()` entries after validating the range; a
        // zero-length call only returns the stored count.
        let raw = unsafe {
            raven_libc_sys::arch::syscall2(PROC_GETGROUPS, out.as_mut_ptr() as usize, out.len())
        };
        raven_libc_sys::errno::ret(raw).map_err(errno)
    }

    fn setcred_ex(&self, request: &SetCred) -> Result<(), i32> {
        // SAFETY: all arguments are scalar values plus the live `request.groups` slice; the
        // kernel validates the count and user range before copying.
        let raw = unsafe {
            raven_libc_sys::arch::syscall6(
                PROC_SETCRED_EX,
                request.kind as usize,
                request.a as isize as usize,
                request.b as isize as usize,
                request.c as isize as usize,
                request.groups.as_ptr() as usize,
                request.groups.len(),
            )
        };
        raven_libc_sys::errno::ret(raw).map(|_| ()).map_err(errno)
    }
}
