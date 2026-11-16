// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2j `pipe-fork` fixture: inherited endpoints, cross-process blocking, EOF, EPIPE and SIGPIPE.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::pipe::pipe;
use raven_rt::println;
use raven_rt::sys::{handle_close, read, thread_create, write};

use raven_pipe_tests::{SIGPIPE, WNOHANG, fork, sigrecv, wait};

entry!(main);

/// Inherited handle slots, written before `PROC_FORK` and read from the child's COW copy.
const READY_R: usize = 0;
const READY_W: usize = 1;
const GO_R: usize = 2;
const GO_W: usize = 3;
const DONE_R: usize = 4;
const DONE_W: usize = 5;
const BULK_R: usize = 6;
const BULK_W: usize = 7;

/// More than the 64 KiB ring, so the writer must block and wait for the reader.
const BULK: usize = 72 * 1024;

static HANDLES: [AtomicU32; 8] = [const { AtomicU32::new(0) }; 8];

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global pipe_fork_child_entry
    .type pipe_fork_child_entry, @function
pipe_fork_child_entry:
    call pipe_fork_child_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size pipe_fork_child_entry, . - pipe_fork_child_entry
"#
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global pipe_fork_child_entry
    .type pipe_fork_child_entry, %function
pipe_fork_child_entry:
    bl pipe_fork_child_body
    mov x8, #0x21
    svc #0
1:
    b 1b
    .size pipe_fork_child_entry, . - pipe_fork_child_entry
"#
);

unsafe extern "C" {
    fn pipe_fork_child_entry();
}

#[inline]
fn store(index: usize, handle: Handle) {
    if let Some(slot) = HANDLES.get(index) {
        slot.store(handle.0, Ordering::Relaxed);
    }
}

#[inline]
fn load(index: usize) -> Handle {
    Handle(
        HANDLES
            .get(index)
            .map(|slot| slot.load(Ordering::Relaxed))
            .unwrap_or(u32::MAX),
    )
}

/// Child thread: handshake, then prove EOF and EPIPE on the inherited endpoints.
#[unsafe(no_mangle)]
pub extern "C" fn pipe_fork_child_body() -> i32 {
    let ready_write = load(READY_W);
    let go_read = load(GO_R);
    let go_write = load(GO_W);
    let done_write = load(DONE_W);
    let bulk_write = load(BULK_W);
    let mut buffer = [0u8; 8];

    if write(ready_write, b"R") != Ok(1) {
        return 1;
    }
    let read_ok = read(go_read, &mut buffer[..1]) == Ok(1) && buffer[0] == b'G';

    // 72 KiB into a 64 KiB ring: this write blocks until the parent drains it.
    let mut bulk = [0u8; BULK];
    for (index, byte) in bulk.iter_mut().enumerate() {
        *byte = (index & 0xFF) as u8;
    }
    let bulk_ok = write(bulk_write, &bulk) == Ok(BULK);
    let _ = handle_close(bulk_write);

    // After this close no read end remains, so the next write is a broken pipe.
    let _ = handle_close(go_read);
    // The parent's next read must observe EOF as soon as this copy disappears.
    let _ = handle_close(ready_write);

    let epipe = write(go_write, b"x") == Err(Errno::EPIPE);
    let mut signal = [0u64; 3];
    let sigpipe = sigrecv(&mut signal, WNOHANG) == Ok(SIGPIPE);

    let bits = u32::from(read_ok)
        | (u32::from(epipe) << 1)
        | (u32::from(sigpipe) << 2)
        | (u32::from(bulk_ok) << 3);
    let status = bits.to_le_bytes();
    if write(done_write, &status) != Ok(4) {
        return 2;
    }
    if read_ok && epipe && sigpipe && bulk_ok {
        0
    } else {
        3
    }
}

fn stack_top() -> usize {
    let anchor = 0u64;
    (&anchor as *const u64 as usize) & !0xf
}

fn run() -> Result<(), Errno> {
    let (ready_read, ready_write) = pipe()?;
    let (go_read, go_write) = pipe()?;
    let (done_read, done_write) = pipe()?;
    let (bulk_read, bulk_write) = pipe()?;
    store(READY_R, ready_read);
    store(READY_W, ready_write);
    store(GO_R, go_read);
    store(GO_W, go_write);
    store(DONE_R, done_read);
    store(DONE_W, done_write);
    store(BULK_R, bulk_read);
    store(BULK_W, bulk_write);

    let child = fork(Handle::SELF_PROC)?;
    let thread = thread_create(
        child,
        pipe_fork_child_entry as *const () as usize,
        stack_top(),
        0,
    )?;

    let mut buffer = [0u8; 8];
    let ready = read(ready_read, &mut buffer[..1]);
    let inherit = ready == Ok(1) && buffer[0] == b'R';
    // Close the parent's go-read copy before waking the child: the child still holds its own
    // copy, so the write succeeds, and once the child closes that copy the next write is EPIPE.
    let block = inherit && handle_close(go_read).is_ok() && write(go_write, b"G") == Ok(1);

    // Drain the 72 KiB blocking write before the child can reach its EOF close.
    let mut bulk = [0u8; BULK];
    let mut got = 0usize;
    while got < BULK {
        match read(bulk_read, &mut bulk[got..]) {
            Ok(0) => break,
            Ok(count) => got = got.saturating_add(count),
            Err(error) => return Err(error),
        }
    }
    let bulk_ok = got == BULK
        && bulk
            .iter()
            .enumerate()
            .all(|(index, byte)| *byte == (index & 0xFF) as u8);
    handle_close(bulk_read)?;
    handle_close(bulk_write)?;

    // Remove the parent's copies that must not keep the child's endpoints alive.
    handle_close(ready_write)?;
    handle_close(done_write)?;

    let eof = read(ready_read, &mut buffer) == Ok(0);
    let mut status = [0u8; 4];
    let done = read(done_read, &mut status) == Ok(4);
    let bits = u32::from_le_bytes(status);
    let read_ok = bits & 1 != 0;
    let epipe = bits & 2 != 0;
    let sigpipe = bits & 4 != 0;

    let mut code = 0i32;
    wait(child, &mut code)?;
    handle_close(thread)?;
    handle_close(child)?;
    handle_close(ready_read)?;
    handle_close(done_read)?;

    println!(
        "pipe-fork: inherit={} block={}",
        u8::from(inherit),
        u8::from(block)
    );
    println!(
        "pipe-fork: eof={} epipe={} sigpipe={}",
        u8::from(eof),
        u8::from(epipe),
        u8::from(sigpipe)
    );
    println!("pipe-fork: bulk={} child code={code}", u8::from(bulk_ok));
    if !(inherit && block && eof && done && bulk_ok && read_ok && epipe && sigpipe && code == 0) {
        println!("pipe-fork: FAIL checks");
        return Err(Errno::EINVAL);
    }
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("pipe-fork: ok");
            0
        }
        Err(error) => {
            println!("pipe-fork: error {error:?}");
            1
        }
    }
}
