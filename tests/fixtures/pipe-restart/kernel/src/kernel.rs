// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S781 kernel-level pipe-restart fixture.
//!
//! Two rows on raw blocking pipe syscalls, with no libc in the path:
//!
//! * `read`: a blocked read on an empty pipe is interrupted by a caught kernel `SIGUSR1`; it
//!   returns `EINTR`, the handler runs, and a following write/read delivers the bytes.
//! * `write`: a blocking write larger than the 64 KiB ring is interrupted after transferring the
//!   bytes that fitted; FreeBSD `dofilewrite` returns that partial count instead of `EINTR`, so
//!   the caller never loses accepted bytes. The fixture then reads those bytes back.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{PROC_SIGQUEUE, SIGACTION};
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::pipe::pipe;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const SIGUSR1: usize = 10;
const SA_RESTART: u32 = 0x0002;
const SIGNALLER_DELAY_NS: u64 = 200_000_000;
const BULK: usize = 80 * 1024;
const RING: usize = 64 * 1024;

static HANDLED: AtomicU32 = AtomicU32::new(0);
static ARMED: AtomicU32 = AtomicU32::new(0);

/// The Raven C19 40-byte `SIGACTION` request (`kernel/src/syscall/thread.rs`).
#[repr(C)]
struct SigAction {
    handler: u64,
    mask: [u64; 2],
    flags: u32,
    _pad: u32,
    restorer: u64,
}

const _: () = assert!(core::mem::size_of::<SigAction>() == 40);

#[repr(C, align(16))]
struct ThreadStack([u8; 64 * 1024]);

static mut READ_SIGNALLER_STACK: ThreadStack = ThreadStack([0; 64 * 1024]);
static mut WRITE_SIGNALLER_STACK: ThreadStack = ThreadStack([0; 64 * 1024]);

extern "C" fn handler(signo: i32) {
    if signo as usize == SIGUSR1 {
        HANDLED.fetch_add(1, Ordering::SeqCst);
    }
}

fn stack_top(stack: *const ThreadStack) -> usize {
    stack as usize + core::mem::size_of::<ThreadStack>()
}

/// Queue SIGUSR1 once the main thread is inside its blocking call.
extern "C" fn signaller(_arg: usize) -> ! {
    while ARMED.load(Ordering::Acquire) == 0 {
        core::hint::spin_loop();
    }
    let _ = sys::nanosleep(SIGNALLER_DELAY_NS);
    // SAFETY: scalar syscall arguments; target 0 is this process.
    let sent = unsafe { sys::syscall3(PROC_SIGQUEUE, 0, SIGUSR1, 0) };
    sys::thread_exit(if sent < 0 { 1 } else { 0 })
}

fn fail(row: &str, value: isize) -> i32 {
    println!("pipe-restart-kernel: fail {row} value={value}");
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let action = SigAction {
        handler: handler as *const () as u64,
        mask: [0; 2],
        flags: SA_RESTART,
        _pad: 0,
        restorer: 0,
    };
    // SAFETY: the kernel copies the 40-byte request from the live stack object.
    let installed =
        unsafe { sys::syscall3(SIGACTION, SIGUSR1, &action as *const SigAction as usize, 0) };
    if installed < 0 {
        return fail("sigaction", installed);
    }

    let row_read = run_read_row();
    if row_read != 0 {
        return row_read;
    }
    let row_write = run_write_row();
    if row_write != 0 {
        return row_write;
    }
    println!("pipe-restart-kernel: ok");
    0
}

/// A blocked read returns `EINTR` with the handler run, then the bytes still arrive.
fn run_read_row() -> i32 {
    let (read_handle, write_handle) = match pipe() {
        Ok(handles) => handles,
        Err(error) => return fail("read-pipe", error.0 as isize),
    };
    let thread = match sys::thread_create(
        Handle::SELF_PROC,
        signaller as *const () as usize,
        stack_top(core::ptr::addr_of!(READ_SIGNALLER_STACK)),
        0,
    ) {
        Ok(thread) => thread,
        Err(error) => return fail("read-thread", error.0 as isize),
    };
    HANDLED.store(0, Ordering::SeqCst);
    let mut buffer = [0u8; 16];
    ARMED.store(1, Ordering::Release);
    let blocked = sys::read(read_handle, &mut buffer);
    let _ = sys::handle_close(thread);
    ARMED.store(0, Ordering::Release);
    match blocked {
        Err(Errno::EINTR) => {}
        Err(error) => return fail("read", error.0 as isize),
        Ok(count) => return fail("read-returned", count as isize),
    }
    let handled = HANDLED.load(Ordering::SeqCst);
    if handled != 1 {
        return fail("read-handler", handled as isize);
    }
    if sys::write(write_handle, b"hello") != Ok(5) {
        return fail("read-write", -1);
    }
    let count = match sys::read(read_handle, &mut buffer[..5]) {
        Ok(count) => count,
        Err(error) => return fail("read-after", error.0 as isize),
    };
    if count != 5 || &buffer[..5] != b"hello" {
        return fail("read-data", count as isize);
    }
    let _ = sys::handle_close(read_handle);
    let _ = sys::handle_close(write_handle);
    println!("pipe-restart-kernel: read interrupt ok");
    0
}

/// A blocked write interrupted after `free()` bytes returns that partial count.
fn run_write_row() -> i32 {
    let (read_handle, write_handle) = match pipe() {
        Ok(handles) => handles,
        Err(error) => return fail("write-pipe", error.0 as isize),
    };
    let thread = match sys::thread_create(
        Handle::SELF_PROC,
        signaller as *const () as usize,
        stack_top(core::ptr::addr_of!(WRITE_SIGNALLER_STACK)),
        0,
    ) {
        Ok(thread) => thread,
        Err(error) => return fail("write-thread", error.0 as isize),
    };
    let mut bulk = [0u8; BULK];
    for (index, byte) in bulk.iter_mut().enumerate() {
        *byte = (index & 0xFF) as u8;
    }
    HANDLED.store(0, Ordering::SeqCst);
    ARMED.store(1, Ordering::Release);
    let written = sys::write(write_handle, &bulk);
    let _ = sys::handle_close(thread);
    ARMED.store(0, Ordering::Release);
    let count = match written {
        Ok(count) => count,
        Err(error) => return fail("write", error.0 as isize),
    };
    if count != RING {
        return fail("write-partial", count as isize);
    }
    let handled = HANDLED.load(Ordering::SeqCst);
    if handled != 1 {
        return fail("write-handler", handled as isize);
    }
    let mut back = [0u8; RING];
    let got = match sys::read(read_handle, &mut back) {
        Ok(count) => count,
        Err(error) => return fail("write-read-back", error.0 as isize),
    };
    if got != RING
        || back
            .iter()
            .enumerate()
            .any(|(index, byte)| *byte != (index & 0xFF) as u8)
    {
        return fail("write-data", got as isize);
    }
    let _ = sys::handle_close(read_handle);
    let _ = sys::handle_close(write_handle);
    println!("pipe-restart-kernel: write partial ok");
    0
}
