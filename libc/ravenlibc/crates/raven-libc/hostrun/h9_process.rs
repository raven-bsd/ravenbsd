// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-run h9: the process map tables of C64 — identity values, the local rlimit table, the
//! zero usage record, the single-policy scheduler answers, the affinity fill, the honest
//! `ENOSYS` of `fork`/`exec`/spawn/credentials and the wait `ECHILD` path.
//!
//! The pre-`procd` backend never enters the kernel, so every check runs on the Linux host. The
//! exit code is the index of the first failed check.

#![no_std]
#![no_main]

use core::arch::asm;
use core::ffi::{c_char, c_int};

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
    let mut text = *b"H9 FAIL 0\n";
    if let Some(byte) = text.get_mut(8) {
        *byte = b'0' + u8::try_from(index).unwrap_or(0);
    }
    // SAFETY: fixed local buffer and exit_group never returns.
    unsafe {
        write(&text);
        exit(index)
    }
}

fn errno() -> i32 {
    // SAFETY: the shell's process errno cell.
    unsafe { *__errno_location() }
}

fn text_equal(left: *const c_char, right: &[u8]) -> bool {
    let mut index = 0usize;
    loop {
        // SAFETY: both sides are NUL-terminated byte arrays.
        let left_byte = unsafe { *left.cast::<u8>().add(index) };
        let right_byte = right.get(index).copied().unwrap_or(0);
        if left_byte != right_byte {
            return false;
        }
        if left_byte == 0 {
            return true;
        }
        index = index.wrapping_add(1);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut limit = [0u64; 2];
    let mut usage = [0u64; 34];
    let mut mask = [0u8; 16];
    let mut param: c_int = 1;
    let mut status: c_int = 0;
    let mut login = [0i8; 32];
    // SAFETY: every pointer below follows its C contract.
    unsafe {
        if getpid() != 1 || getppid() != 0 {
            fail(1);
        }
        if getuid() != 0 || geteuid() != 0 || getgid() != 0 || getegid() != 0 {
            fail(2);
        }
        if getpgid(0) != 1 || getpgrp() != 1 || getsid(0) != 1 {
            fail(3);
        }
        if getgroups(0, core::ptr::null_mut()) != 0 {
            fail(4);
        }
        if issetugid() != 0 {
            fail(5);
        }
        if getrlimit(7, &mut limit) != 0 || limit != [1024, 32_768] {
            fail(6);
        }
        if getrusage(0, &mut usage) != 0 || usage.iter().any(|word| *word != 0) {
            fail(7);
        }
        if getpriority(0, 0) != 0 {
            fail(8);
        }
        if sched_getcpu() != 0
            || sched_getscheduler(0) != 0
            || sched_get_priority_max(0) != 0
            || sched_get_priority_min(0) != 0
        {
            fail(9);
        }
        if fork() != -1 || errno() != ENOSYS {
            fail(10);
        }
        if waitpid(-1, &mut status, 0) != -1 || errno() != ECHILD {
            fail(11);
        }
        if !text_equal(getprogname(), b"") {
            fail(13);
        }
        setprogname(b"path/tool\0".as_ptr().cast());
        if !text_equal(getprogname(), b"tool") {
            fail(14);
        }
        if !getlogin().is_null() || errno() != ENXIO {
            fail(15);
        }
        if getlogin_r(login.as_mut_ptr(), 32) != ENXIO {
            fail(16);
        }
        if system(core::ptr::null()) != 1 {
            fail(17);
        }
        if daemon(1, 1) != -1 || errno() != ENOSYS {
            fail(18);
        }
        if nice(0) != -1 || errno() != ENOSYS {
            fail(19);
        }
        if setuid(0) != -1 || errno() != ENOSYS {
            fail(20);
        }
        if setgid(0) != -1 || errno() != ENOSYS {
            fail(21);
        }
        if getpgid(2) != -1 || errno() != ESRCH {
            fail(22);
        }
        if sched_getaffinity(0, 16, mask.as_mut_ptr()) != 0 || mask[0] != 1 {
            fail(23);
        }
        if sched_setaffinity(0, 16, mask.as_ptr()) != -1 || errno() != ENOSYS {
            fail(24);
        }
        if sched_getparam(0, &mut param) != 0 || param != 0 {
            fail(25);
        }
        if sched_setparam(0, &param) != 0 {
            fail(26);
        }
        if sched_setscheduler(0, 0, &param) != 0 {
            fail(27);
        }
    }
    // SAFETY: fixed success text and exit_group never returns.
    unsafe {
        write(b"H9 OK\n");
        exit(0)
    }
}

const ENOSYS: c_int = 38;
const ECHILD: c_int = 10;
const ENXIO: c_int = 6;
const ESRCH: c_int = 3;

unsafe extern "C" {
    fn __errno_location() -> *mut c_int;
    fn getpid() -> c_int;
    fn getppid() -> c_int;
    fn getuid() -> u32;
    fn geteuid() -> u32;
    fn getgid() -> u32;
    fn getegid() -> u32;
    fn getpgid(pid: c_int) -> c_int;
    fn getpgrp() -> c_int;
    fn getsid(pid: c_int) -> c_int;
    fn getgroups(size: c_int, list: *mut u32) -> c_int;
    fn issetugid() -> c_int;
    fn getrlimit(resource: c_int, limit: *mut [u64; 2]) -> c_int;
    fn getrusage(who: c_int, usage: *mut [u64; 34]) -> c_int;
    fn getpriority(which: c_int, who: c_int) -> c_int;
    fn sched_getcpu() -> c_int;
    fn sched_getscheduler(pid: c_int) -> c_int;
    fn sched_get_priority_max(policy: c_int) -> c_int;
    fn sched_get_priority_min(policy: c_int) -> c_int;
    fn sched_getaffinity(pid: c_int, size: usize, mask: *mut u8) -> c_int;
    fn sched_setaffinity(pid: c_int, size: usize, mask: *const u8) -> c_int;
    fn sched_getparam(pid: c_int, param: *mut c_int) -> c_int;
    fn sched_setparam(pid: c_int, param: *const c_int) -> c_int;
    fn sched_setscheduler(pid: c_int, policy: c_int, param: *const c_int) -> c_int;
    fn fork() -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn getprogname() -> *const c_char;
    fn setprogname(name: *const c_char);
    fn getlogin() -> *mut c_char;
    fn getlogin_r(buffer: *mut c_char, size: usize) -> c_int;
    fn system(command: *const c_char) -> c_int;
    fn daemon(nochdir: c_int, noclose: c_int) -> c_int;
    fn nice(increment: c_int) -> c_int;
    fn setuid(uid: u32) -> c_int;
    fn setgid(gid: u32) -> c_int;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: exit_group never returns.
    unsafe { exit(127) }
}
