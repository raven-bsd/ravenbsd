// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K524 `/init` fixture: `AT_RANDOM`, `GETRANDOM`, fork and exec.
//!
//! 1. `AT_RANDOM` and `GETRANDOM` are non-zero and different.
//! 2. `GETRANDOM` with length 0 succeeds; an unknown flag is `EINVAL`.
//! 3. A `PROC_FORK` child inherits the parent's `AT_RANDOM` bytes and draws a different
//!    `GETRANDOM` stream from the shared pool.
//! 4. Eight self-execs each receive a fresh `AT_RANDOM` and `GETRANDOM`; the history is carried
//!    in argv so any repeated value is detected and fails the run.
//!
//! The qtest negative case boots with `entropy=off`: `AT_RANDOM`/`GETRANDOM` must fail rather
//! than fall back to xorshift or jitter.

#![no_std]
#![no_main]

extern crate alloc;

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::syscall::Syscall;
use raven_abi::types::{Handle, map, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::exec::exec_image;
use raven_rt::println;
use raven_rt::sys::{
    map_initfs, syscall1, syscall2, syscall3, syscall6, thread_create, thread_exit,
};

entry!(main);

/// `GETRANDOM(buf, len, flags)` syscall number.
const GETRANDOM: usize = 0x08;
/// `PROC_FORK(source, 0) -> child handle` raw number.
const PROC_FORK: usize = 0x39;
/// `PROC_HANDLE_INFO(handle) -> pid` raw number.
const PROC_HANDLE_INFO: usize = 0x3E;
/// `PROC_WAIT_EX(idtype, id, options, status) -> pid` raw number.
const PROC_WAIT_EX: usize = 0x48;
/// Wait for a pid.
const P_PID: usize = 1;
/// `AT_RANDOM` auxiliary-vector key.
const AT_RANDOM: usize = 25;
/// One page.
const PAGE: usize = 4096;
/// Shared parent/child control page.
const CONTROL: usize = 0x4000_0000;
/// Parent `AT_RANDOM` bytes (16 bytes, two words).
const SLOT_PARENT_AT: usize = 0;
/// Parent `GETRANDOM` bytes (16 bytes, two words).
const SLOT_PARENT_GR: usize = 2;
/// Pointer to the parent's `AT_RANDOM` bytes in the cloned stack.
const SLOT_AT_PTR: usize = 4;
/// Child `AT_RANDOM` bytes (16 bytes, two words).
const SLOT_CHILD_AT: usize = 5;
/// Child `GETRANDOM` bytes (16 bytes, two words).
const SLOT_CHILD_GR: usize = 7;
/// Child has published its bytes.
const SLOT_READY: usize = 9;
/// Self-exec chain length.
const RUNS: usize = 8;

fn main(args: raven_rt::Args) -> i32 {
    let run = args
        .get(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let at_pointer = args.auxv(AT_RANDOM).unwrap_or(0);
    let at_random = match read_at_random(at_pointer) {
        Some(bytes) => bytes,
        None => {
            println!("random: FAIL at-random missing");
            return 1;
        }
    };
    let mut getrandom = [0u8; 16];
    if getrandom_bytes(&mut getrandom, 0) != Ok(16) {
        println!("random: FAIL getrandom");
        return 1;
    }
    if at_random.iter().all(|byte| *byte == 0) || getrandom.iter().all(|byte| *byte == 0) {
        println!("random: FAIL all-zero");
        return 1;
    }

    if run == 0 {
        println!("random: at-random nonzero=1");
        if at_random == getrandom {
            println!("random: FAIL same-value");
            return 1;
        }
        println!("random: getrandom nonzero=1 at-random-differs=1");

        let mut empty = [0u8; 0];
        if getrandom_bytes(&mut empty, 0) != Ok(0) {
            println!("random: FAIL zero-length");
            return 1;
        }
        let mut invalid = [0u8; 16];
        if getrandom_bytes(&mut invalid, u32::MAX) != Err(Errno::EINVAL) {
            println!("random: FAIL invalid-flags");
            return 1;
        }
        println!("random: getrandom zero-len=0 invalid-flags=1");

        if !fork_check(&at_random, &getrandom, at_pointer) {
            println!("random: FAIL fork");
            return 1;
        }
        println!("random: fork at-random-inherited=1 getrandom-differs=1");
        println!("random: exec-start");
    }

    let at_hex = to_hex(&at_random);
    let gr_hex = to_hex(&getrandom);
    let previous_at = args.get(2).unwrap_or("");
    let previous_gr = args.get(3).unwrap_or("");
    if previous_at.contains(at_hex.as_str()) || previous_gr.contains(gr_hex.as_str()) {
        println!("random: FAIL repeated value run={run}");
        return 1;
    }
    let mut next_at = String::from(previous_at);
    next_at.push_str(&at_hex);
    let mut next_gr = String::from(previous_gr);
    next_gr.push_str(&gr_hex);

    if run + 1 < RUNS {
        if exec_self(run + 1, &next_at, &next_gr).is_err() {
            println!("random: FAIL exec");
            return 1;
        }
        println!("random: FAIL exec returned");
        return 1;
    }
    println!("random: exec at-random-distinct={RUNS} getrandom-distinct={RUNS}");
    println!("random: pass");
    0
}

/// Read 16 bytes from the `AT_RANDOM` pointer.
fn read_at_random(pointer: usize) -> Option<[u8; 16]> {
    if pointer == 0 {
        return None;
    }
    let mut bytes = [0u8; 16];
    // SAFETY: `AT_RANDOM` names 16 readable bytes on the process's initial stack.
    unsafe {
        core::ptr::copy_nonoverlapping(pointer as *const u8, bytes.as_mut_ptr(), bytes.len());
    }
    Some(bytes)
}

/// One `GETRANDOM` call with the kernel return convention.
fn getrandom_bytes(buffer: &mut [u8], flags: u32) -> Result<usize, Errno> {
    // SAFETY: the kernel validates the pointer and length; the buffer is a live mutable slice.
    let raw = unsafe {
        syscall3(
            GETRANDOM,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            flags as usize,
        )
    };
    Errno::from_ret(raw)
}

/// The forked child's thread entry: read the cloned `AT_RANDOM`, draw `GETRANDOM`, publish both.
extern "C" fn fork_child_entry() -> ! {
    let pointer = read64(CONTROL + SLOT_AT_PTR * 8) as usize;
    let mut at_random = [0u8; 16];
    // SAFETY: the pointer came from the parent's `AT_RANDOM`, which the fork cloned at the same
    // virtual address; the parent still owns that readable stack page.
    unsafe {
        core::ptr::copy_nonoverlapping(
            pointer as *const u8,
            at_random.as_mut_ptr(),
            at_random.len(),
        );
    }
    let mut getrandom = [0u8; 16];
    let _ = getrandom_bytes(&mut getrandom, 0);
    write_bytes(CONTROL + SLOT_CHILD_AT * 8, &at_random);
    write_bytes(CONTROL + SLOT_CHILD_GR * 8, &getrandom);
    write64(CONTROL + SLOT_READY * 8, 1);
    thread_exit(0)
}

/// Create a shared control page and prove the fork child does not repeat the parent stream.
fn fork_check(parent_at: &[u8; 16], parent_getrandom: &[u8; 16], at_pointer: usize) -> bool {
    if map_control().is_err() {
        return false;
    }
    write_bytes(CONTROL + SLOT_PARENT_AT * 8, parent_at);
    write_bytes(CONTROL + SLOT_PARENT_GR * 8, parent_getrandom);
    write64(CONTROL + SLOT_AT_PTR * 8, at_pointer as u64);
    write64(CONTROL + SLOT_READY * 8, 0);

    // SAFETY: `PROC_FORK` takes a self handle and a zero flags word; the kernel validates both.
    let raw_child = unsafe { syscall6(PROC_FORK, Handle::SELF_PROC.0 as usize, 0, 0, 0, 0, 0) };
    let Ok(value) = Errno::from_ret(raw_child) else {
        return false;
    };
    let Ok(raw) = u32::try_from(value) else {
        return false;
    };
    let child = Handle(raw);
    // SAFETY: `PROC_HANDLE_INFO` reads one live child handle and returns its pid.
    let Ok(pid) = Errno::from_ret(unsafe { syscall1(PROC_HANDLE_INFO, child.0 as usize) }) else {
        return false;
    };
    let anchor = 0u64;
    let stack_top = (&anchor as *const u64 as usize) & !0xf;
    if thread_create(child, fork_child_entry as *const () as usize, stack_top, 0).is_err() {
        return false;
    }
    let mut status = 0i32;
    // SAFETY: `PROC_WAIT_EX` blocks on the child pid and writes one status word to `status`.
    let waited = unsafe {
        syscall6(
            PROC_WAIT_EX,
            P_PID,
            pid,
            0,
            (&mut status as *mut i32) as usize,
            0,
            0,
        )
    };
    if Errno::from_ret(waited).is_err() || read64(CONTROL + SLOT_READY * 8) != 1 {
        return false;
    }

    let mut child_at = [0u8; 16];
    let mut child_getrandom = [0u8; 16];
    read_bytes(CONTROL + SLOT_CHILD_AT * 8, &mut child_at);
    read_bytes(CONTROL + SLOT_CHILD_GR * 8, &mut child_getrandom);
    let mut parent_next = [0u8; 16];
    if getrandom_bytes(&mut parent_next, 0) != Ok(16) {
        return false;
    }

    child_at == *parent_at
        && child_getrandom != *parent_getrandom
        && child_getrandom != parent_next
        && parent_next != *parent_getrandom
}

/// Map the shared 4 KiB control page.
fn map_control() -> Result<usize, Errno> {
    // SAFETY: `MEMOBJ_CREATE` takes scalar length and flags.
    let raw = unsafe { syscall2(Syscall::MEMOBJ_CREATE as usize, PAGE, 0) };
    let value = Errno::from_ret(raw)?;
    let handle = Handle(u32::try_from(value).map_err(|_| Errno::EINVAL)?);
    raven_rt::sys::map_object_in(
        Handle::SELF_PROC,
        handle,
        0,
        CONTROL,
        PAGE,
        prot::READ | prot::WRITE,
        map::FIXED | map::SHARED,
    )
}

/// Execute the same `/init` image with the next run number and the accumulated history.
fn exec_self(run: usize, at_history: &str, getrandom_history: &str) -> Result<(), Errno> {
    let image = map_initfs()?;
    let initfs = Initfs::parse(image).map_err(|_| Errno::ENOEXEC)?;
    let init = initfs.get("/init").ok_or(Errno::ENOENT)?;
    let run_text = format!("{run}");
    let owned = [
        String::from("/init"),
        run_text,
        at_history.to_string(),
        getrandom_history.to_string(),
    ];
    let argv: Vec<&[u8]> = owned.iter().map(String::as_bytes).collect();
    exec_image(init.data, b"/init", &argv, &[], &[])
}

/// Encode bytes as lowercase hexadecimal.
fn to_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        text.push_str(&format!("{byte:02x}"));
    }
    text
}

fn read64(address: usize) -> u64 {
    // SAFETY: the control page is mapped read/write at `CONTROL`; `address` stays inside it.
    unsafe { core::ptr::read_volatile(address as *const u64) }
}

fn write64(address: usize, value: u64) {
    // SAFETY: the control page is mapped read/write at `CONTROL`; `address` stays inside it.
    unsafe {
        core::ptr::write_volatile(address as *mut u64, value);
    }
}

fn read_bytes(address: usize, out: &mut [u8]) {
    // SAFETY: the control page is mapped read/write; the slot is 16 bytes and `out` is 16 bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(address as *const u8, out.as_mut_ptr(), out.len());
    }
}

fn write_bytes(address: usize, bytes: &[u8]) {
    // SAFETY: the control page is mapped read/write; the slot is 16 bytes and `bytes` is 16 bytes.
    unsafe {
        core::ptr::copy_nonoverlapping(bytes.as_ptr(), address as *mut u8, bytes.len());
    }
}
