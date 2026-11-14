// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-run h6: the platform glue's argument layer (`stat`, `lstat`, `fstat`, `getcwd`,
//! `uname`, `access`, `unlink`, `open`/`openat`/`creat`, `poll`) of C60c.
//!
//! The checks stop before the Raven `CALL`/page provider: a NULL path is `EFAULT`, a NULL
//! `struct stat` is `EFAULT`, a zero `getcwd` size with a buffer is `EINVAL`, and a NULL poll
//! array with a non-zero count is `EFAULT`. A successful `stat`/`getcwd` needs the path resolver
//! and the Raven page provider for its `Vec`, which a Linux host cannot serve; the report records
//! that split instead of faking a server answer. The exit code is the first failed check.
#![no_std]
#![no_main]

use core::arch::asm;
use core::ffi::{c_char, c_int};

const EFAULT: i32 = 14;
const EINVAL: i32 = 22;
const AT_FDCWD: c_int = -100;

unsafe fn write(bytes: &[u8]) {
    let result: isize;
    // SAFETY: Linux write(2) with a valid buffer.
    unsafe {
        asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack));
    }
    let _ = result;
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)) }
}

fn fail(index: i32) -> ! {
    let mut text = *b"H6 FAIL 0\n";
    if let Some(byte) = text.get_mut(8) {
        *byte = b'0' + u8::try_from(index).unwrap_or(0);
    }
    // SAFETY: fixed local buffer.
    unsafe { write(&text) };
    // SAFETY: exit_group never returns.
    unsafe { exit(index) }
}

fn errno() -> i32 {
    // SAFETY: the shell's process errno cell.
    unsafe { *__errno_location() }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut metadata = [0u64; 16];
    // SAFETY: every pointer and buffer below follows its C contract.
    unsafe {
        if uname(core::ptr::null_mut()) != -1 || errno() != EFAULT {
            fail(1);
        }
        if stat(core::ptr::null(), &mut metadata) != -1 || errno() != EFAULT {
            fail(2);
        }
        if lstat(core::ptr::null(), &mut metadata) != -1 || errno() != EFAULT {
            fail(3);
        }
        if fstat(-1, core::ptr::null_mut()) != -1 || errno() != EFAULT {
            fail(4);
        }
        if access(core::ptr::null(), 0) != -1 || errno() != EFAULT {
            fail(5);
        }
        if unlink(core::ptr::null()) != -1 || errno() != EFAULT {
            fail(6);
        }
        let mut buffer = [0i8; 64];
        if !getcwd(buffer.as_mut_ptr(), 0).is_null() || errno() != EINVAL {
            fail(7);
        }
        if open(core::ptr::null(), 0) != -1 || errno() != EFAULT {
            fail(8);
        }
        if openat(AT_FDCWD, core::ptr::null(), 0) != -1 || errno() != EFAULT {
            fail(9);
        }
        if creat(core::ptr::null(), 0) != -1 || errno() != EFAULT {
            fail(10);
        }
        if poll(core::ptr::null_mut(), 1, 0) != -1 || errno() != EFAULT {
            fail(11);
        }
    }
    // SAFETY: fixed success text.
    unsafe { write(b"H6 OK\n") };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn __errno_location() -> *mut i32;
    fn uname(buf: *mut core::ffi::c_void) -> c_int;
    fn stat(path: *const c_char, buf: *mut [u64; 16]) -> c_int;
    fn lstat(path: *const c_char, buf: *mut [u64; 16]) -> c_int;
    fn fstat(fd: c_int, buf: *mut [u64; 16]) -> c_int;
    fn access(path: *const c_char, mode: c_int) -> c_int;
    fn unlink(path: *const c_char) -> c_int;
    fn getcwd(buf: *mut c_char, size: usize) -> *mut c_char;
    fn poll(fds: *mut core::ffi::c_void, nfds: usize, timeout: c_int) -> c_int;
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn openat(dirfd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    fn creat(path: *const c_char, mode: c_int) -> c_int;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: exit_group never returns.
    unsafe { exit(127) }
}
