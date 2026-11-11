// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S32 two-thread capture client: 1 MiB round trips, unmap-during-request, capture-window
//! collision and server death.

#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, AtomicUsize, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_s32_capture::{
    O_RDWR, PAGE, all_equal, fill, handle_close, map_anonymous, map_fixed, map_hint, nanosleep,
    open, protect, read, thread_create, thread_exit, unmap, write,
};

entry!(main);

/// Capture size and the size of every fixture buffer.
const LEN: usize = 1024 * 1024;
/// Worker stack size.
const STACK: usize = 16 * 1024;
/// Kernel capture window (mirrors `memory::uspace::capture_window`; test-only).
const CAPTURE_BASE: usize = 0x0000_6000_0000_0000;
/// Kernel capture window size.
const CAPTURE_SIZE: usize = 32 * 1024 * 1024;

/// Client buffer the worker unmaps.
static WORK_ADDR: AtomicUsize = AtomicUsize::new(0);
/// Worker result: 0 pending, 1 intact, 2 unmap failed, 3 map failed, 4 sentinel corrupted.
static WORK_RESULT: AtomicU32 = AtomicU32::new(0);
/// Set when the worker finished its check.
static WORK_DONE: AtomicU32 = AtomicU32::new(0);

extern "C" fn worker(_arg: usize) -> ! {
    let _ = nanosleep(30_000_000);
    let address = WORK_ADDR.load(Ordering::Acquire);
    if unmap(address, LEN).is_err() {
        WORK_RESULT.store(2, Ordering::Release);
        WORK_DONE.store(1, Ordering::Release);
        thread_exit(0);
    }
    let reuse = match map_anonymous(LEN) {
        Ok(buffer) => buffer as usize,
        Err(_) => {
            WORK_RESULT.store(3, Ordering::Release);
            WORK_DONE.store(1, Ordering::Release);
            thread_exit(0);
        }
    };
    // SAFETY: `reuse` is a fresh private 1 MiB mapping.
    unsafe {
        fill(reuse, LEN, 0xaa);
    }
    // The server writes its pattern around 150 ms; check the reused mapping afterwards.
    let _ = nanosleep(250_000_000);
    // SAFETY: `reuse` is still a private readable 1 MiB mapping.
    let intact = unsafe { all_equal(reuse, LEN, 0xaa) };
    WORK_RESULT.store(if intact { 1 } else { 4 }, Ordering::Release);
    WORK_DONE.store(1, Ordering::Release);
    thread_exit(0)
}

fn start_worker() -> bool {
    let stack = match map_anonymous(STACK) {
        Ok(stack) => stack as usize,
        Err(_) => return false,
    };
    let entry: extern "C" fn(usize) -> ! = worker;
    match thread_create(entry as usize, stack.saturating_add(STACK), 0) {
        Ok(thread) => {
            let _ = handle_close(thread);
            true
        }
        Err(_) => false,
    }
}

fn wait_worker() -> u32 {
    let mut tries = 0usize;
    while WORK_DONE.load(Ordering::Acquire) == 0 && tries < 4000 {
        let _ = nanosleep(1_000_000);
        tries = tries.saturating_add(1);
    }
    WORK_RESULT.load(Ordering::Acquire)
}

/// Fill a 1 MiB buffer with `(index as u8) ^ seed`.
///
/// # Safety
///
/// `address` must point at a writable 1 MiB mapping of this process.
unsafe fn fill_pattern(address: *mut u8, seed: u8) {
    let mut index = 0usize;
    while index < LEN {
        // SAFETY: the caller passes a writable 1 MiB mapping.
        unsafe {
            address.add(index).write((index as u8) ^ seed);
        }
        index = index.saturating_add(1);
    }
}

/// Check a 1 MiB buffer against `(index as u8) ^ seed`.
///
/// # Safety
///
/// `address` must point at a readable 1 MiB mapping of this process.
unsafe fn check_pattern(address: *const u8, seed: u8) -> bool {
    let mut index = 0usize;
    while index < LEN {
        // SAFETY: the caller passes a readable 1 MiB mapping.
        if unsafe { address.add(index).read() } != ((index as u8) ^ seed) {
            return false;
        }
        index = index.saturating_add(1);
    }
    true
}

fn roundtrip(handle: Handle) -> bool {
    let input = match map_anonymous(LEN) {
        Ok(buffer) => buffer,
        Err(_) => return false,
    };
    let output = match map_anonymous(LEN) {
        Ok(buffer) => buffer,
        Err(_) => return false,
    };
    // SAFETY: `input` is a private 1 MiB mapping.
    unsafe {
        fill_pattern(input, 0x5a);
    }
    // SAFETY: both buffers are private writable/readable 1 MiB mappings.
    unsafe {
        fill(output as usize, LEN, 0);
    }
    // SAFETY: `input` and `output` are 1 MiB mappings of this process.
    let input_slice = unsafe { core::slice::from_raw_parts(input, LEN) };
    let output_slice = unsafe { core::slice::from_raw_parts_mut(output, LEN) };
    if write(handle, input_slice).ok() != Some(LEN) {
        return false;
    }
    if read(handle, output_slice).ok() != Some(LEN) {
        return false;
    }
    // SAFETY: `output` is a private readable 1 MiB mapping.
    unsafe { check_pattern(output, 0x5a) }
}

fn frame_loop(handle: Handle) -> bool {
    let mut round = 0usize;
    while round < 8 {
        if !roundtrip(handle) {
            return false;
        }
        round = round.saturating_add(1);
    }
    true
}

fn unmap_during_write(handle: Handle) -> bool {
    let buffer = match map_anonymous(LEN) {
        Ok(buffer) => buffer,
        Err(_) => return false,
    };
    // SAFETY: `buffer` is a private 1 MiB mapping.
    unsafe {
        fill_pattern(buffer, 0x3c);
    }
    WORK_ADDR.store(buffer as usize, Ordering::Release);
    WORK_RESULT.store(0, Ordering::Release);
    WORK_DONE.store(0, Ordering::Release);
    if !start_worker() {
        return false;
    }
    // SAFETY: the buffer is a 1 MiB mapping; the worker unmaps it while the syscall blocks, so
    // the slice is never read again after this call.
    let slice = unsafe { core::slice::from_raw_parts(buffer, LEN) };
    let result = write(handle, slice);
    let worker = wait_worker();
    result.ok() == Some(LEN) && worker == 1
}

fn unmap_during_read(handle: Handle) -> bool {
    let buffer = match map_anonymous(LEN) {
        Ok(buffer) => buffer,
        Err(_) => return false,
    };
    WORK_ADDR.store(buffer as usize, Ordering::Release);
    WORK_RESULT.store(0, Ordering::Release);
    WORK_DONE.store(0, Ordering::Release);
    if !start_worker() {
        return false;
    }
    // SAFETY: the buffer is a 1 MiB mapping; the worker unmaps it while the syscall blocks, so
    // the slice is never read again after this call.
    let slice = unsafe { core::slice::from_raw_parts_mut(buffer, LEN) };
    let result = read(handle, slice);
    let worker = wait_worker();
    result.ok() == Some(LEN) && worker == 1
}

fn window_test() -> bool {
    if map_fixed(CAPTURE_BASE, PAGE).is_ok() {
        return false;
    }
    if unmap(CAPTURE_BASE, PAGE).is_ok() {
        return false;
    }
    if protect(CAPTURE_BASE, PAGE, raven_abi::types::prot::READ).is_ok() {
        return false;
    }
    let mapped = match map_hint(CAPTURE_BASE, PAGE) {
        Ok(address) => address as usize,
        Err(_) => return false,
    };
    if mapped >= CAPTURE_BASE && mapped < CAPTURE_BASE.saturating_add(CAPTURE_SIZE) {
        return false;
    }
    unmap(mapped, PAGE).is_ok()
}

fn fail(line: &str) -> i32 {
    println!("s32: {}", line);
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let echo = match open("s32:echo", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => return fail("client open failed"),
    };
    let delay_write = match open("s32:delay-write", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => return fail("client delay-write open failed"),
    };
    let delay_read = match open("s32:delay-read", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => return fail("client delay-read open failed"),
    };
    let die = match open("s32:die", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => return fail("client die open failed"),
    };

    if !roundtrip(echo) {
        return fail("roundtrip failed");
    }
    println!("s32: roundtrip ok");

    if !frame_loop(echo) {
        return fail("frame loop failed");
    }
    println!("s32: frame loop ok");

    if !unmap_during_write(delay_write) {
        return fail("unmap during write failed");
    }
    println!("s32: unmap-write ok");

    if !unmap_during_read(delay_read) {
        return fail("unmap during read failed");
    }
    println!("s32: unmap-read ok");

    if !window_test() {
        return fail("capture window test failed");
    }
    println!("s32: window ok");

    let buffer = match map_anonymous(LEN) {
        Ok(buffer) => buffer,
        Err(_) => return fail("server-death buffer failed"),
    };
    // SAFETY: `buffer` is a private writable 1 MiB mapping.
    let slice = unsafe { core::slice::from_raw_parts_mut(buffer, LEN) };
    match read(die, slice) {
        Err(Errno::EIO) => {}
        _ => return fail("server death did not wake the client"),
    }
    println!("s32: server death ok");
    0
}
