// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Host-run h5: `fnmatch`, the shell's `regex_t` argument/error glue and `qsort`/`bsearch`
//! (C60c section 5).
//!
//! The program is `x86_64-unknown-none` machine code run natively on a Linux x86-64 host.
//! `fnmatch`, `qsort` and `bsearch` are pure. The regex checks stay on the shell glue that does
//! not allocate (`REG_INVARG` on an invalid object and on the `EXTENDED|NOSPEC` flag pair, the
//! `regerror` text, `regfree` on a zeroed object): a successful `regcomp` allocates through the
//! Raven page provider, which a Linux host cannot serve. The C45 crate's 412 host tests cover
//! the allocating path; the report records this split. The exit code is the first failed check.
#![no_std]
#![no_main]

use core::arch::asm;

const FNM_NOMATCH: i32 = 1;
const FNM_CASEFOLD: i32 = 0x10;
const REG_EXTENDED: i32 = 0x1;
const REG_NOSPEC: i32 = 0x20;
const REG_INVARG: i32 = 16;

/// The C `regex_t` (eight opaque words).
#[repr(C)]
struct RegexT {
    opaque: [usize; 8],
}

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
    let mut text = *b"H5 FAIL 0\n";
    if let Some(byte) = text.get_mut(8) {
        *byte = b'0' + u8::try_from(index).unwrap_or(0);
    }
    // SAFETY: fixed local buffer.
    unsafe { write(&text) };
    // SAFETY: exit_group never returns.
    unsafe { exit(index) }
}

extern "C" fn compare(left: *const u8, right: *const u8) -> i32 {
    // SAFETY: qsort calls this with two `i32` elements.
    let a = unsafe { *(left as *const i32) };
    // SAFETY: as above.
    let b = unsafe { *(right as *const i32) };
    match a.cmp(&b) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    let mut regex = RegexT { opaque: [0; 8] };
    // SAFETY: the C string, regex_t and buffer contracts below.
    unsafe {
        if fnmatch(b"*.c\0".as_ptr(), b"foo.c\0".as_ptr(), 0) != 0 {
            fail(1);
        }
        if fnmatch(b"*.c\0".as_ptr(), b"foo.h\0".as_ptr(), 0) != FNM_NOMATCH {
            fail(2);
        }
        if fnmatch(b"foo\0".as_ptr(), b"FOO\0".as_ptr(), FNM_CASEFOLD) != 0 {
            fail(3);
        }
        if regexec(&regex, b"x\0".as_ptr(), 0, core::ptr::null_mut(), 0) != REG_INVARG {
            fail(4);
        }
        if regcomp(core::ptr::null_mut(), b"x\0".as_ptr(), 0) != REG_INVARG {
            fail(5);
        }
        if regcomp(&mut regex, b"x\0".as_ptr(), REG_EXTENDED | REG_NOSPEC) != REG_INVARG {
            fail(6);
        }
        let mut message = [0u8; 64];
        let length = regerror(2, core::ptr::null(), message.as_mut_ptr().cast(), message.len());
        if length <= 1 || message.first().copied().unwrap_or(0) == 0 {
            fail(7);
        }
        regfree(&mut regex);
        let mut values = [5i32, 1, 9, 3, 7, 2, 8];
        qsort(
            values.as_mut_ptr().cast(),
            values.len(),
            core::mem::size_of::<i32>(),
            Some(compare),
        );
        if values != [1, 2, 3, 5, 7, 8, 9] {
            fail(8);
        }
        let key = 7i32;
        let found = bsearch(
            (&key as *const i32).cast(),
            values.as_ptr().cast(),
            values.len(),
            core::mem::size_of::<i32>(),
            Some(compare),
        );
        if found.is_null() || *(found as *const i32) != 7 {
            fail(9);
        }
        let missing = 99i32;
        let absent = bsearch(
            (&missing as *const i32).cast(),
            values.as_ptr().cast(),
            values.len(),
            core::mem::size_of::<i32>(),
            Some(compare),
        );
        if !absent.is_null() {
            fail(10);
        }
    }
    // SAFETY: fixed success text.
    unsafe { write(b"H5 OK\n") };
    // SAFETY: all checks passed.
    unsafe { exit(0) }
}

unsafe extern "C" {
    fn fnmatch(pattern: *const u8, string: *const u8, flags: i32) -> i32;
    fn regcomp(regex: *mut RegexT, pattern: *const u8, cflags: i32) -> i32;
    fn regexec(
        regex: *const RegexT,
        string: *const u8,
        nmatch: usize,
        matches: *mut core::ffi::c_void,
        eflags: i32,
    ) -> i32;
    fn regerror(
        code: i32,
        regex: *const RegexT,
        buf: *mut core::ffi::c_char,
        size: usize,
    ) -> usize;
    fn regfree(regex: *mut RegexT);
    fn qsort(
        base: *mut u8,
        nmemb: usize,
        size: usize,
        compar: Option<extern "C" fn(*const u8, *const u8) -> i32>,
    );
    fn bsearch(
        key: *const u8,
        base: *const u8,
        nmemb: usize,
        size: usize,
        compar: Option<extern "C" fn(*const u8, *const u8) -> i32>,
    ) -> *mut u8;
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    // SAFETY: exit_group never returns.
    unsafe { exit(127) }
}
