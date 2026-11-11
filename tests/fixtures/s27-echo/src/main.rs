// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S27 console-input fixture.
//!
//! Loops `debug_read` on the console receiver and echoes every byte back through `debug_write`
//! upper-cased.  Exits after echoing a received `q`.

#![no_std]
#![no_main]

use core::arch::asm;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_rt::entry;

entry!(main);

const READ_SIZE: usize = 64;

#[inline(always)]
unsafe fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: the caller passes valid registers for the RavenBSD ABI; the kernel validates every
    // pointer and handle.  `r10` is caller-saved under the syscall convention (the runtime's
    // wrappers use `clobber_abi("C")`) and the kernel's return stub uses it as scratch, so it
    // must be declared clobbered here; `rcx` and `r11` are architectural syscall clobbers.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            lateout("rcx") _,
            lateout("r10") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn result(raw: isize) -> Result<usize, Errno> {
    Errno::from_ret(raw)
}

fn debug_read(buffer: &mut [u8], timeout_ns: u64) -> Result<usize, Errno> {
    result(unsafe {
        syscall3(
            Syscall::DEBUG_READ as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            timeout_ns as usize,
        )
    })
}

fn debug_write(bytes: &[u8]) -> Result<usize, Errno> {
    result(unsafe {
        syscall3(
            Syscall::DEBUG_WRITE as usize,
            bytes.as_ptr() as usize,
            bytes.len(),
            0,
        )
    })
}

fn exit(code: i32) -> ! {
    let _ = unsafe { syscall3(Syscall::EXIT as usize, code as usize, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

fn main(_args: raven_rt::Args) -> i32 {
    let mut buffer = [0u8; READ_SIZE];
    loop {
        let count = match debug_read(&mut buffer, 0) {
            Ok(count) => count,
            Err(_) => return 1,
        };
        let mut index = 0usize;
        while index < count {
            let Some(byte) = buffer.get(index).copied() else {
                break;
            };
            let upper = byte.to_ascii_uppercase();
            if debug_write(&[upper]).is_err() {
                return 2;
            }
            if byte == b'q' {
                return 0;
            }
            index = index.saturating_add(1);
        }
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
