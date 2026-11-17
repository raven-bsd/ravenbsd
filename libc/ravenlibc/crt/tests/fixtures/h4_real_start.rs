// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! H4: the real `__raven_libc_start` of the shell with a static `crt1.o`.
//!
//! The fixture owns the symbols the shell does not define: the environment pointer, the
//! program-name aliases and the main-thread initialisation that C64 will replace. The canary
//! word and the exit registry are the shell's now (C61 integration). `main` prints `H4 OK`
//! through the Linux write syscall and leaves through `exit_group`; the libc `_exit` would
//! issue a Raven syscall.

#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::ffi::c_int;
use core::sync::atomic::{AtomicUsize, Ordering};

/// Host-run arena: the Raven page provider is not available on Linux, and the shell start path
/// allocates its fd table. A bounded bump allocator keeps this start-path test independent of
/// the target page provider.
#[repr(align(16))]
struct Arena([u8; 4 * 1024 * 1024]);

static mut ARENA: Arena = Arena([0; 4 * 1024 * 1024]);
static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Bump;

// SAFETY: allocations are carved from one static arena; every range is bounds-checked and
// deallocation is a no-op for this short-lived test process.
unsafe impl GlobalAlloc for Bump {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = core::ptr::addr_of_mut!(ARENA.0).cast::<u8>();
        let align = layout.align().max(16);
        let offset = NEXT.load(Ordering::Relaxed);
        let Some(start) = offset.checked_add(align - 1).map(|value| value & !(align - 1)) else {
            return core::ptr::null_mut();
        };
        let Some(end) = start.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        if end > 4 * 1024 * 1024 {
            return core::ptr::null_mut();
        }
        NEXT.store(end, Ordering::Relaxed);
        // SAFETY: `start..end` is inside the static arena and `base` is its first byte.
        unsafe { base.add(start) }
    }

    unsafe fn dealloc(&self, _pointer: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static GLOBAL: Bump = Bump;

// The shell's guard word (C61 integration: `libc.a` provides it now).
unsafe extern "C" {
    static __stack_chk_guard: usize;
}

#[unsafe(no_mangle)]
pub static mut environ: *mut *mut u8 = core::ptr::null_mut();
#[unsafe(no_mangle)]
pub static mut __progname_full: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut __progname: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut program_invocation_name: *const u8 = core::ptr::null();
#[unsafe(no_mangle)]
pub static mut program_invocation_short_name: *const u8 = core::ptr::null();

/// C64 replaces this with the real main-thread initialisation.
#[unsafe(no_mangle)]
pub extern "C" fn __raven_init_main_thread(_auxv: *const usize, _canary: usize) {}

fn write(text: &[u8]) {
    // SAFETY: Linux `write` reads the static buffer; it has no other effect.
    unsafe {
        core::arch::asm!(
            "syscall",
            in("rax") 1usize,
            in("rdi") 1usize,
            in("rsi") text.as_ptr(),
            in("rdx") text.len(),
            lateout("rcx") _,
            lateout("r11") _,
        );
    }
}

fn exit(code: i32) -> ! {
    // SAFETY: Linux `exit_group` never returns.
    unsafe {
        core::arch::asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn));
    }
}

fn c_string(pointer: *const u8) -> &'static [u8] {
    let mut length = 0usize;
    // SAFETY: the C8 stack strings are NUL-terminated.
    while unsafe { *pointer.add(length) } != 0 {
        length += 1;
    }
    // SAFETY: the bytes up to the NUL were just read.
    unsafe { core::slice::from_raw_parts(pointer, length) }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const u8, envp: *const *const u8) -> c_int {
    if argc != 2 {
        exit(1);
    }
    // SAFETY: `argv[1]` is a C8 string.
    if c_string(unsafe { *argv.add(1) }) != b"hello" {
        exit(2);
    }
    if unsafe { environ } != envp as *mut *mut u8 {
        exit(3);
    }
    if unsafe { __progname } == core::ptr::null() {
        exit(4);
    }
    // SAFETY: the program name is a C8 string.
    if !c_string(unsafe { __progname }).ends_with(b"h4-real-start") {
        exit(5);
    }
    if unsafe { program_invocation_name } != unsafe { __progname_full } {
        exit(6);
    }
    let mut count = 0usize;
    loop {
        // SAFETY: the environment array is NUL-terminated.
        if unsafe { *envp.add(count) }.is_null() {
            break;
        }
        count += 1;
    }
    // SAFETY: the auxv pairs follow the environment terminator.
    let auxv = unsafe { envp.add(count + 1) } as *const usize;
    let mut pagesz = 0usize;
    let mut random = 0usize;
    let mut index = 0usize;
    loop {
        // SAFETY: the auxv array is terminated by `(0, 0)`.
        let key = unsafe { *auxv.add(index) };
        if key == 0 {
            break;
        }
        // SAFETY: the value word follows the key.
        let value = unsafe { *auxv.add(index + 1) };
        if key == 6 {
            pagesz = value;
        }
        if key == 25 {
            random = value;
        }
        index += 2;
    }
    if pagesz != 4096 {
        exit(7);
    }
    if random == 0 {
        exit(8);
    }
    // SAFETY: `AT_RANDOM` names 16 bytes.
    let expected = unsafe { *(random as *const usize) };
    if unsafe { __stack_chk_guard } != expected {
        exit(9);
    }
    write(b"H4 OK\n");
    exit(0)
}
