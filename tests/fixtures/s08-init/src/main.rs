// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S08 `/init` fixture.
//!
//! Prints a greeting and the uname line through `DEBUG_WRITE`, runs the documented negative
//! syscall tests, then exits 0.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::layout::x86_64::USER_END;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, Rights};
use raven_abi::uname::Uname;

#[unsafe(link_section = ".rodata")]
static RO_PAGE: [u8; 512] = [0; 512];

#[inline(always)]
unsafe fn syscall6(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> isize {
    let result: isize;
    // SAFETY: the S08 ABI fixes the syscall register assignment; the caller passes valid args.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            in("r10") a3,
            in("r8") a4,
            in("r9") a5,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

#[inline(always)]
fn syscall1(number: usize, a0: usize) -> isize {
    // SAFETY: the register convention is the S08 ABI.
    unsafe { syscall6(number, a0, 0, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall2(number: usize, a0: usize, a1: usize) -> isize {
    // SAFETY: the register convention is the S08 ABI.
    unsafe { syscall6(number, a0, a1, 0, 0, 0, 0) }
}

#[inline(always)]
fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    // SAFETY: the register convention is the S08 ABI.
    unsafe { syscall6(number, a0, a1, a2, 0, 0, 0) }
}

fn write(bytes: &[u8]) {
    let _ = syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    );
}

fn write_field(field: &[u8; raven_abi::uname::UNAME_FIELD_LEN]) {
    if let Some(text) = Uname::field_str(field) {
        write(text.as_bytes());
    }
}

fn write_hex(value: isize) {
    let mut buffer = [0u8; 18];
    buffer[0] = b'0';
    buffer[1] = b'x';
    let mut index = 2;
    let mut shift = 60i32;
    while shift >= 0 {
        let digit = ((value as usize >> shift) & 0xF) as u8;
        buffer[index] = if digit < 10 {
            b'0' + digit
        } else {
            b'a' + digit - 10
        };
        index += 1;
        shift -= 4;
    }
    write(&buffer);
}

fn check(name: &str, result: isize, expected: Errno) -> bool {
    if result == expected.to_ret() {
        return true;
    }
    write(b"s08-init: negative failed: ");
    write(name.as_bytes());
    write(b" got ");
    write_hex(result);
    write(b" want ");
    write_hex(expected.to_ret());
    write(b"\n");
    false
}

fn main() {
    write(b"s08-init: hello\n");

    let mut uname = Uname::zeroed();
    let result = syscall2(
        Syscall::UNAME as usize,
        &mut uname as *mut Uname as usize,
        0,
    );
    if result != 0 {
        write(b"s08-init: uname failed\n");
        exit(1);
    }
    write_field(&uname.sysname);
    write(b" ");
    write_field(&uname.nodename);
    write(b" ");
    write_field(&uname.release);
    write(b" ");
    write_field(&uname.version);
    write(b" ");
    write_field(&uname.machine);
    write(b"\n");

    let mut ok = true;
    ok &= check(
        "debug null",
        syscall3(Syscall::DEBUG_WRITE as usize, 0, 1, 0),
        Errno::EFAULT,
    );
    ok &= check(
        "debug kernel",
        syscall3(Syscall::DEBUG_WRITE as usize, 0xFFFF_8000_0000_0000, 1, 0),
        Errno::EFAULT,
    );
    ok &= check(
        "debug last page",
        syscall3(
            Syscall::DEBUG_WRITE as usize,
            (USER_END - 0x1000 + 1) as usize,
            16,
            0,
        ),
        Errno::EFAULT,
    );
    ok &= check(
        "debug overflow",
        syscall3(Syscall::DEBUG_WRITE as usize, usize::MAX - 1, 2, 0),
        Errno::EFAULT,
    );
    ok &= check(
        "debug unmapped",
        syscall3(Syscall::DEBUG_WRITE as usize, 0x7000_0000_0000, 16, 0),
        Errno::EFAULT,
    );
    ok &= check(
        "debug too long",
        syscall3(
            Syscall::DEBUG_WRITE as usize,
            &RO_PAGE as *const _ as usize,
            5000,
            0,
        ),
        Errno::EINVAL,
    );
    ok &= check(
        "uname read-only",
        syscall2(Syscall::UNAME as usize, &RO_PAGE as *const _ as usize, 0),
        Errno::EFAULT,
    );
    ok &= check(
        "handle close",
        syscall1(Syscall::HANDLE_CLOSE as usize, 0x00FF_FFFF),
        Errno::EBADF,
    );
    ok &= check(
        "handle dup",
        syscall2(
            Syscall::HANDLE_DUP as usize,
            Handle::INITFS.0 as usize,
            Rights::ALL.bits() as usize,
        ),
        Errno::EACCES,
    );
    ok &= check("unknown syscall", syscall1(0x7777, 0), Errno::ENOSYS);
    ok &= check(
        "map invalid args",
        // SAFETY: the negative test only needs the ABI register convention with zero args.
        // S24 implemented MAP: zero args are now EINVAL, not ENOSYS.
        unsafe { syscall6(Syscall::MAP as usize, 0, 0, 0, 0, 0, 0) },
        Errno::EINVAL,
    );

    if ok {
        write(b"s08-init: negatives ok\n");
        exit(0);
    }
    exit(1);
}

fn exit(code: i32) -> ! {
    let _ = syscall1(Syscall::EXIT as usize, code as usize);
    loop {
        core::hint::spin_loop();
    }
}

/// User entry point.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main();
    exit(1)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
