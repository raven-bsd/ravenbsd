// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![no_std]
#![no_main]

use core::arch::asm;
include!("alloc.rs");
use core::ffi::c_void;

unsafe fn write(bytes: &[u8]) {
    let result: isize;
    // SAFETY: Linux write(2) with a valid buffer.
    unsafe { asm!("syscall", in("rax") 1usize, in("rdi") 1usize, in("rsi") bytes.as_ptr(), in("rdx") bytes.len(), lateout("rax") result, lateout("rcx") _, lateout("r11") _, options(nostack)); }
    let _ = result;
}

unsafe fn exit(code: i32) -> ! {
    // SAFETY: Linux exit_group(2) never returns.
    unsafe { asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn)); }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut memory = [0u8; 32];
    // SAFETY: the libc `fmemopen` contract over a writable buffer.
    let file = unsafe { fmemopen(memory.as_mut_ptr().cast::<c_void>(), memory.len(), b"w+\0".as_ptr()) };
    if file.is_null() {
        // SAFETY: Linux exit_group.
        unsafe { exit(1) };
    }
    let written = unsafe { fwrite(b"libc\0".as_ptr().cast::<c_void>(), 1, 4, file) };
    let flushed = unsafe { fflush(file) };
    let seeked = unsafe { fseek(file, 0, 0) };
    let mut output = [0u8; 4];
    let read = unsafe { fread(output.as_mut_ptr().cast::<c_void>(), 1, 4, file) };
    let closed = unsafe { fclose(file) };
    if written != 4 || flushed != 0 || seeked != 0 || read != 4 || closed != 0 || output != *b"libc" {
        // SAFETY: Linux exit_group.
        unsafe { exit(1) };
    }
    // SAFETY: fixed success text.
    unsafe { write(b"H4 OK\n") };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn fmemopen(buffer: *mut c_void, size: usize, mode: *const u8) -> *mut c_void;
    fn fwrite(ptr: *const c_void, size: usize, count: usize, stream: *mut c_void) -> usize;
    fn fflush(stream: *mut c_void) -> i32;
    fn fseek(stream: *mut c_void, offset: i64, whence: i32) -> i32;
    fn fread(ptr: *mut c_void, size: usize, count: usize, stream: *mut c_void) -> usize;
    fn fclose(stream: *mut c_void) -> i32;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: Linux exit_group.
    unsafe { exit(127) }
}
