// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! B65 echo-dialogue fixture.
//!
//! Prints `ready> `, reads one console line with `DEBUG_READ` (S27), prints `got: <line>` and
//! exits on `quit`. The qtest case `dialogue-echo` types into it. S27 is not merged yet, so the
//! case is marked `skip` until the syscall exists; this program is the guest side of the
//! dialogue and is built exactly like the S11 fixture.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // Raw syscalls; every block has a SAFETY comment.

use core::arch::asm;

use raven_abi::syscall::Syscall;

/// Longest accepted input line.
const LINE: usize = 128;

#[inline(always)]
fn syscall3(number: usize, a0: usize, a1: usize, a2: usize) -> isize {
    let result: isize;
    // SAFETY: the ABI fixes the syscall register assignment; DEBUG_WRITE and DEBUG_READ
    // validate the slice before touching it, and the kernel validates every pointer.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") number => result,
            in("rdi") a0,
            in("rsi") a1,
            in("rdx") a2,
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
    result
}

fn write(bytes: &[u8]) {
    let _ = syscall3(
        Syscall::DEBUG_WRITE as usize,
        bytes.as_ptr() as usize,
        bytes.len(),
        0,
    );
}

/// Read console bytes into `buffer`; `0` means the timeout expired, a negative value is an
/// errno. S27 delivers raw bytes, so the fixture assembles the line itself.
fn read(buffer: &mut [u8], timeout_ns: u64) -> isize {
    syscall3(
        Syscall::DEBUG_READ as usize,
        buffer.as_mut_ptr() as usize,
        buffer.len(),
        timeout_ns as usize,
    )
}

/// Collect one line into `line`, returning its length without the terminator.
fn read_line(line: &mut [u8; LINE]) -> Result<usize, ()> {
    let mut used = 0usize;
    loop {
        if used == LINE {
            return Ok(used);
        }
        let count = read(&mut line[used..], 0);
        if count < 0 {
            return Err(());
        }
        if count == 0 {
            // Timeout with no bytes: keep waiting for the prompt's reader.
            continue;
        }
        let count = count as usize;
        let end = line[used..used + count]
            .iter()
            .position(|byte| *byte == b'\n' || *byte == b'\r');
        used += count;
        if let Some(offset) = end {
            return Ok(used - count + offset);
        }
    }
}

fn main() {
    let mut line = [0u8; LINE];
    loop {
        write(b"ready> ");
        let length = match read_line(&mut line) {
            Ok(length) => length,
            Err(()) => {
                write(b"\ndialogue: debug_read failed\n");
                exit(1);
            }
        };
        // A terminal would echo the typed newline; DEBUG_READ delivers bytes only.
        write(b"\n");
        if &line[..length] == b"quit" {
            write(b"bye\n");
            exit(0);
        }
        write(b"got: ");
        write(&line[..length]);
        write(b"\n");
    }
}

fn exit(code: i32) -> ! {
    let _ = syscall3(Syscall::EXIT as usize, code as usize, 0, 0);
    loop {
        core::hint::spin_loop();
    }
}

/// User entry point.
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    main();
    exit(0)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(2)
}
