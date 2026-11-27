// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S168b C8 probe: a custom `_start` reads the argument register before touching the stack, then
//! reports it against the stack pointer the kernel installed. The init process execs this image,
//! so the values come from `apply_exec_context`.
//!
//! This binary deliberately does not link `raven-rt`: its global `_start` would collide with the
//! probe start below. It talks to the kernel with the raw syscall ABI from `raven-abi`.

#![no_std]
#![no_main]

use core::fmt::{self, Write};
use core::panic::PanicInfo;

use raven_abi::syscall::Syscall;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".global _start",
    "_start:",
    "mov %rsp, %rsi",
    "and $-16, %rsp",
    "xor %ebp, %ebp",
    "call arg_main",
    "ud2",
    options(att_syntax),
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    ".global _start",
    "_start:",
    "mov x1, sp",
    "mov x2, sp",
    "and x2, x2, #0xfffffffffffffff0",
    "mov sp, x2",
    "bl arg_main",
    "brk #0",
);

#[cfg(target_arch = "x86_64")]
unsafe fn syscall(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: isize;
    // SAFETY: the Raven syscall convention is number in `rax`, arguments in
    // `rdi`/`rsi`/`rdx`, result in `rax`.
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") nr as isize => ret,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            clobber_abi("C"),
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "aarch64")]
unsafe fn syscall(nr: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let ret: usize;
    // SAFETY: the Raven syscall convention is number in `x8`, arguments in `x0..x2`, result in
    // `x0`.
    unsafe {
        core::arch::asm!(
            "svc 0",
            in("x8") nr,
            in("x0") a0,
            in("x1") a1,
            in("x2") a2,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret as isize
}

/// A fixed stack buffer for the one report line; no allocator is linked.
struct Line {
    bytes: [u8; 160],
    len: usize,
}

impl Write for Line {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.as_bytes() {
            let Some(slot) = self.bytes.get_mut(self.len) else {
                return Err(fmt::Error);
            };
            *slot = *byte;
            self.len += 1;
        }
        Ok(())
    }
}

/// Write one report line through `DEBUG_WRITE`.
fn write_line(line: &Line) {
    // SAFETY: `line.bytes[..line.len]` is initialized, in this stack frame, for the duration of
    // the syscall. `DEBUG_WRITE` reads exactly that many bytes and copies them to the console.
    let _ = unsafe {
        syscall(
            Syscall::DEBUG_WRITE as usize,
            line.bytes.as_ptr() as usize,
            line.len,
            0,
        )
    };
}

fn exit(code: i32) -> ! {
    // SAFETY: `EXIT` terminates the calling process; it does not return.
    let _ = unsafe { syscall(Syscall::EXIT as usize, code as usize, 0, 0) };
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    let mut line = Line {
        bytes: [0; 160],
        len: 0,
    };
    let _ = line.write_str("proc-exec-arg: child panic\n");
    write_line(&line);
    exit(101)
}

/// Report the C8 argument and stack registers and exit non-zero when they disagree.
#[unsafe(no_mangle)]
pub extern "C" fn arg_main(arg: u64, sp: u64) -> ! {
    // SAFETY: C8 puts the stack block at `sp0`; its first word is `argc`, inside the user stack
    // mapping the kernel built. The read is volatile so the compiler cannot fold it away.
    let argc = unsafe { core::ptr::read_volatile(sp as *const usize) };
    let aligned = sp % 16 == 0;
    let matched = arg == sp;

    let mut line = Line {
        bytes: [0; 160],
        len: 0,
    };
    let _ = write!(
        line,
        "proc-exec-arg: child arg={arg:#x} sp={sp:#x} match={} aligned={} argc={argc}\n",
        u8::from(matched),
        u8::from(aligned)
    );
    write_line(&line);

    exit(i32::from(!(matched && aligned && argc == 1)))
}
