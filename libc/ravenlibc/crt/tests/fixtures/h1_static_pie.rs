// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! H1: a static-PIE program's `__raven_libc_start`.
//!
//! Linked with `rcrt1.o`; verifies that the self-relocator made every `static` table absolute
//! before this function runs. Exit code 0 means all checks passed.

#![no_std]

#[used]
#[unsafe(no_mangle)]
pub static A: u8 = 11;
#[used]
#[unsafe(no_mangle)]
pub static B: u8 = 22;
#[used]
#[unsafe(no_mangle)]
pub static C: u8 = 33;

/// A function-pointer table: RELATIVE relocations, packed into RELR when the linker is asked.
#[used]
#[unsafe(no_mangle)]
pub static TABLE: [&u8; 3] = [&A, &B, &C];

/// A second relocated array, covered by the same RELR bitmap.
#[used]
#[unsafe(no_mangle)]
pub static RELR_ARR: [&u8; 2] = [&C, &A];

fn exit(code: i32) -> ! {
    // SAFETY: Linux `exit_group` never returns.
    unsafe {
        core::arch::asm!("syscall", in("rax") 231usize, in("rdi") code as usize, options(noreturn));
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_libc_start(
    _sp: *const usize,
    rtld_fini: usize,
    iplt_start: usize,
    iplt_end: usize,
) -> ! {
    if rtld_fini != 0 || iplt_start != 0 || iplt_end != 0 {
        exit(1);
    }
    // SAFETY: `TABLE` is a live static of this image and `read_volatile` forces a memory read.
    let p0 = unsafe { core::ptr::read_volatile(TABLE.as_ptr()) } as *const u8;
    // SAFETY: see above.
    let p1 = unsafe { core::ptr::read_volatile(TABLE.as_ptr().add(1)) } as *const u8;
    // SAFETY: see above.
    let p2 = unsafe { core::ptr::read_volatile(TABLE.as_ptr().add(2)) } as *const u8;
    if p0 != core::ptr::addr_of!(A) || p1 != core::ptr::addr_of!(B) || p2 != core::ptr::addr_of!(C)
    {
        exit(2);
    }
    // SAFETY: `p0`/`p2` name the statics just compared.
    if unsafe { core::ptr::read_volatile(p0) } != 11
        || unsafe { core::ptr::read_volatile(p2) } != 33
    {
        exit(3);
    }
    // SAFETY: `RELR_ARR` is a live static of this image.
    let r0 = unsafe { core::ptr::read_volatile(RELR_ARR.as_ptr()) } as *const u8;
    // SAFETY: see above.
    let r1 = unsafe { core::ptr::read_volatile(RELR_ARR.as_ptr().add(1)) } as *const u8;
    if r0 != core::ptr::addr_of!(C) || r1 != core::ptr::addr_of!(A) {
        exit(4);
    }
    exit(0)
}
