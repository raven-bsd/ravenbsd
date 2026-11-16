// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S57 `dtrace-ring` fixture.
//!
//! Exercises the kernel DTrace provider without libc:
//!
//! 1. `DTRACE_PROBE_LIST` returns the static `syscall::entry` probe.
//! 2. `DTRACE_ENABLE` with an event handle delivers a consumer wakeup through `EVQ_WAIT`.
//! 3. A no-consumer burst wraps a per-CPU ring; the retained records and the drop count agree.
//! 4. Four producer threads generate 100k `THREAD_SELF` records under `-smp 4`; the consumer
//!    drains them with `DTRACE_READ` and checks the per-CPU statistics.

#![no_std]
#![no_main]

use core::arch::asm;
use core::sync::atomic::{AtomicU32, Ordering};

use raven_abi::errno::Errno;
use raven_abi::syscall::{Syscall, THREAD_CREATE_EX, THREAD_SELF, ThreadCreateEx};
use raven_abi::types::{Handle, futex, map, prot};
use raven_rt::entry;
use raven_rt::println;

entry!(main);

/// Raw DTrace control numbers (the ticket does not own the ABI contract crate).
const DTRACE_PROBE_LIST: usize = 0x90;
const DTRACE_ENABLE: usize = 0x91;
const DTRACE_DISABLE: usize = 0x92;
const DTRACE_READ: usize = 0x93;
const DTRACE_STATS: usize = 0x94;
const DTRACE_CLEAR: usize = 0x95;

const PAGE: usize = 4096;
const STACK_BASE: usize = 0x6000_0000;
const STACK_STRIDE: usize = 0x8000;
const STACK_LEN: usize = 0x8000;
const READ_BUF: usize = 0x7000_0000;
const READ_PAGES: usize = 4;
const READ_RECORDS: usize = 256;

const MAX_CPUS: usize = 8;
const PRODUCERS: u32 = 4;
const PRODUCER_RECORDS: u32 = 25_000;
const WRAP_RECORDS: u32 = 3 * 4096;
const POLL_LIMIT: u32 = 200_000_000;
const WAKE_TIMEOUT_NS: u64 = 100_000_000;
const NANOSLEEP_1_MS: u64 = 1_000_000;

const JOB_WRAP: usize = 5;

const SIGNALED: usize = 8;
const EVQ_ADD: usize = 1;

static START: AtomicU32 = AtomicU32::new(0);
static READY: AtomicU32 = AtomicU32::new(0);
static DONE: AtomicU32 = AtomicU32::new(0);
static WRAP_DONE: AtomicU32 = AtomicU32::new(0);

static WRAP_CLEAR: AtomicU32 = AtomicU32::new(1);
static PRODUCER_CLEAR: [AtomicU32; 4] = [
    AtomicU32::new(1),
    AtomicU32::new(1),
    AtomicU32::new(1),
    AtomicU32::new(1),
];

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Record {
    sequence: u64,
    timestamp_ns: u64,
    arg0: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    syscall: u32,
    tid: u32,
    cpu: u16,
    probe: u16,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct CpuStats {
    produced: u64,
    consumed: u64,
    dropped: u64,
    capacity: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ProbeInfo {
    id: u32,
    provider: [u8; 16],
    name: [u8; 32],
}

const _: () = assert!(core::mem::size_of::<Record>() == 64);
const _: () = assert!(core::mem::size_of::<CpuStats>() == 32);
const _: () = assert!(core::mem::size_of::<ProbeInfo>() == 52);

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global dtrace_thread_entry
    .type dtrace_thread_entry, @function
dtrace_thread_entry:
    call dtrace_thread_body
    mov edi, eax
    mov eax, 0x21
    syscall
    ud2
    .size dtrace_thread_entry, . - dtrace_thread_entry
"#
);

unsafe extern "C" {
    fn dtrace_thread_entry();
}

#[cfg(target_arch = "x86_64")]
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
    // SAFETY: scalar syscall arguments; the kernel validates handles and pointers.
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

fn sys(
    number: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
) -> Result<usize, Errno> {
    let raw = unsafe { syscall6(number, a0, a1, a2, a3, a4, a5) };
    if raw < 0 {
        Err(Errno(raw.unsigned_abs() as u16))
    } else {
        Ok(raw as usize)
    }
}

fn map_region(address: usize, len: usize) -> Result<(), Errno> {
    // SAFETY: scalar syscall arguments; the kernel validates the user range.
    sys(
        Syscall::MAP as usize,
        Handle::SELF_PROC.0 as usize,
        Handle::NONE.0 as usize,
        0,
        address,
        len,
        map::pack(prot::READ | prot::WRITE, map::FIXED),
    )
    .map(|_| ())
}

fn create_thread(
    stack_top: usize,
    arg: usize,
    clear_tid: &AtomicU32,
) -> Result<usize, Errno> {
    let extended = ThreadCreateEx {
        tp: 0,
        tid_out: 0,
        clear_tid_addr: clear_tid as *const AtomicU32 as usize,
    };
    // SAFETY: `extended` is a live payload; the kernel validates the addresses and the entry.
    sys(
        Syscall::THREAD_CREATE as usize,
        Handle::SELF_PROC.0 as usize,
        dtrace_thread_entry as *const () as usize,
        stack_top,
        arg,
        THREAD_CREATE_EX,
        &extended as *const ThreadCreateEx as usize,
    )
}

fn futex_wait_word(word: &AtomicU32, expected: u32) -> Result<(), Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    sys(
        Syscall::FUTEX as usize,
        word.as_ptr() as usize,
        (futex::WAIT | futex::PRIVATE) as usize,
        expected as usize,
        0,
        0,
        0,
    )
    .map(|_| ())
}

fn futex_wake_word(word: &AtomicU32, count: u32) -> Result<usize, Errno> {
    // SAFETY: `word` is a live aligned AtomicU32; the kernel validates the user address.
    sys(
        Syscall::FUTEX as usize,
        word.as_ptr() as usize,
        (futex::WAKE | futex::PRIVATE) as usize,
        count as usize,
        0,
        0,
        0,
    )
}

fn nanosleep_ns(ns: u64) {
    // SAFETY: NANOSLEEP takes only the scalar duration.
    let _ = sys(Syscall::NANOSLEEP as usize, ns as usize, 0, 0, 0, 0, 0);
}

fn spin_until(word: &AtomicU32, value: u32) -> bool {
    let mut tries = 0;
    while word.load(Ordering::Acquire) != value {
        if tries == POLL_LIMIT {
            return false;
        }
        tries = tries.saturating_add(1);
        core::hint::spin_loop();
    }
    true
}

/// Spawn a producer: wait for START, generate records, then exit through the clear-tid path.
fn producer() {
    READY.fetch_add(1, Ordering::Release);
    while START.load(Ordering::Acquire) == 0 {
        let _ = futex_wait_word(&START, 0);
    }
    let mut count = 0;
    while count < PRODUCER_RECORDS {
        let _ = sys(THREAD_SELF, 0, 0, 0, 0, 0, 0);
        count = count.saturating_add(1);
        if count % 100 == 0 {
            nanosleep_ns(NANOSLEEP_1_MS);
        }
    }
    DONE.fetch_add(1, Ordering::Release);
}

/// Thread body; `arg` selects the job.
#[unsafe(no_mangle)]
pub extern "C" fn dtrace_thread_body(arg: usize) -> i32 {
    match arg {
        JOB_WRAP => {
            let mut count = 0;
            while count < WRAP_RECORDS {
                let _ = sys(THREAD_SELF, 0, 0, 0, 0, 0, 0);
                count = count.saturating_add(1);
            }
            WRAP_DONE.store(1, Ordering::Release);
            0
        }
        1..=4 => {
            producer();
            0
        }
        _ => 1,
    }
}

fn dtrace_probe_list(out: usize, max: usize) -> Result<usize, Errno> {
    sys(DTRACE_PROBE_LIST, out, max, 0, 0, 0, 0)
}

fn dtrace_enable(probe: u32, event: Handle) -> Result<usize, Errno> {
    sys(DTRACE_ENABLE, probe as usize, 0, event.0 as usize, 0, 0, 0)
}

fn dtrace_disable(probe: u32) -> Result<usize, Errno> {
    sys(DTRACE_DISABLE, probe as usize, 0, 0, 0, 0, 0)
}

fn dtrace_read(max: usize) -> Result<usize, Errno> {
    sys(DTRACE_READ, READ_BUF, max, 0, 0, 0, 0)
}

fn dtrace_stats(entries: &mut [CpuStats; MAX_CPUS]) -> Result<usize, Errno> {
    sys(
        DTRACE_STATS,
        entries.as_mut_ptr() as usize,
        MAX_CPUS,
        0,
        0,
        0,
        0,
    )
}

fn dtrace_clear() -> Result<usize, Errno> {
    sys(DTRACE_CLEAR, 0, 0, 0, 0, 0, 0)
}

fn totals(entries: &[CpuStats; MAX_CPUS], count: usize) -> (u64, u64, u64) {
    let mut produced = 0u64;
    let mut consumed = 0u64;
    let mut dropped = 0u64;
    for entry in entries.iter().take(count.min(MAX_CPUS)) {
        produced = produced.saturating_add(entry.produced);
        consumed = consumed.saturating_add(entry.consumed);
        dropped = dropped.saturating_add(entry.dropped);
    }
    (produced, consumed, dropped)
}

fn main(_args: raven_rt::Args) -> i32 {
    if map_region(STACK_BASE, 5 * STACK_STRIDE).is_err()
        || map_region(READ_BUF, READ_PAGES * PAGE).is_err()
    {
        println!("dtrace: fail map");
        return 1;
    }

    // Probe registry.
    let probes = match dtrace_probe_list(0, 0) {
        Ok(count) => count,
        Err(error) => {
            println!("dtrace: fail probe-list count {}", error.0);
            return 1;
        }
    };
    let mut info = ProbeInfo::default();
    if probes != 1 || dtrace_probe_list(&mut info as *mut ProbeInfo as usize, 1) != Ok(1) {
        println!("dtrace: fail probe-list");
        return 1;
    }
    if info.id != 1 || &info.provider[..7] != b"syscall" || &info.name[..5] != b"entry" {
        println!("dtrace: fail probe-info");
        return 1;
    }
    println!("dtrace: probe id=1 provider=syscall");

    // Ring wrap: one producer, no consumer, then drain the retained records.
    if dtrace_enable(1, Handle::NONE).is_err()
        || create_thread(STACK_BASE + STACK_LEN, JOB_WRAP, &WRAP_CLEAR).is_err()
    {
        println!("dtrace: fail wrap setup");
        return 1;
    }
    if !spin_until(&WRAP_DONE, 1) {
        println!("dtrace: fail wrap timeout");
        return 1;
    }
    // The clear-on-exit word is written after the exit syscall's probe has fired.
    if !spin_until(&WRAP_CLEAR, 0) {
        println!("dtrace: fail wrap exit");
        return 1;
    }
    if dtrace_disable(1).is_err() {
        println!("dtrace: fail wrap disable");
        return 1;
    }
    let mut wrap_records = 0usize;
    loop {
        match dtrace_read(READ_RECORDS) {
            Ok(0) => break,
            Ok(count) => wrap_records = wrap_records.saturating_add(count),
            Err(error) => {
                println!("dtrace: fail wrap read {}", error.0);
                return 1;
            }
        }
    }
    let mut entries = [CpuStats::default(); MAX_CPUS];
    let wrap_cpus = match dtrace_stats(&mut entries) {
        Ok(count) => count,
        Err(error) => {
            println!("dtrace: fail wrap stats {}", error.0);
            return 1;
        }
    };
    let (wrap_produced, wrap_consumed, wrap_dropped) = totals(&entries, wrap_cpus);
    if wrap_dropped == 0
        || wrap_consumed != wrap_records as u64
        || wrap_produced != wrap_consumed
    {
        println!(
            "dtrace: fail wrap produced={} consumed={} dropped={} read={}",
            wrap_produced, wrap_consumed, wrap_dropped, wrap_records
        );
        return 1;
    }
    println!(
        "dtrace: wrap produced={} retained={} dropped={}",
        wrap_produced, wrap_records, wrap_dropped
    );
    if dtrace_clear().is_err() {
        println!("dtrace: fail clear after wrap");
        return 1;
    }

    // Consumer wakeup through the S26 event queue.
    let event = match sys(Syscall::EVENT_CREATE as usize, 0, 0, 0, 0, 0, 0) {
        Ok(handle) => Handle(handle as u32),
        Err(error) => {
            println!("dtrace: fail event-create {}", error.0);
            return 1;
        }
    };
    let evq = match sys(Syscall::EVQ_CREATE as usize, 0, 0, 0, 0, 0, 0) {
        Ok(handle) => Handle(handle as u32),
        Err(error) => {
            println!("dtrace: fail evq-create {}", error.0);
            return 1;
        }
    };
    if sys(
        Syscall::EVQ_CTL as usize,
        evq.0 as usize,
        EVQ_ADD,
        event.0 as usize,
        SIGNALED,
        0x57,
        0,
    )
    .is_err()
        || dtrace_enable(1, event).is_err()
    {
        println!("dtrace: fail wakeup setup");
        return 1;
    }
    let _ = sys(THREAD_SELF, 0, 0, 0, 0, 0, 0);
    let mut event_record = [0u64; 2];
    match sys(
        Syscall::EVQ_WAIT as usize,
        evq.0 as usize,
        event_record.as_mut_ptr() as usize,
        1,
        WAKE_TIMEOUT_NS as usize,
        0,
        0,
    ) {
        Ok(1) => {}
        Ok(other) => {
            println!("dtrace: fail wakeup count {other}");
            return 1;
        }
        Err(error) => {
            println!("dtrace: fail wakeup {}", error.0);
            return 1;
        }
    }
    if dtrace_disable(1).is_err() {
        println!("dtrace: fail wakeup disable");
        return 1;
    }
    let mut wake_records = 0usize;
    loop {
        match dtrace_read(READ_RECORDS) {
            Ok(0) => break,
            Ok(count) => wake_records = wake_records.saturating_add(count),
            Err(_) => break,
        }
    }
    if wake_records == 0 || dtrace_clear().is_err() {
        println!("dtrace: fail wakeup drain");
        return 1;
    }
    println!("dtrace: wakeup ok");

    // Four producers, 100k records, one consumer.
    for index in 0..4usize {
        if create_thread(
            STACK_BASE + (index + 1) * STACK_STRIDE + STACK_LEN,
            index + 1,
            &PRODUCER_CLEAR[index],
        )
        .is_err()
        {
            println!("dtrace: fail producer create");
            return 1;
        }
    }
    if !spin_until(&READY, PRODUCERS) {
        println!("dtrace: fail producer ready");
        return 1;
    }
    if dtrace_enable(1, Handle::NONE).is_err() {
        println!("dtrace: fail enable");
        return 1;
    }
    START.store(1, Ordering::Release);
    let _ = futex_wake_word(&START, PRODUCERS);

    let mut cpus_mask = 0u32;
    let mut saw_syscall = false;
    let mut stable = 0u32;
    loop {
        match dtrace_read(READ_RECORDS) {
            Ok(count) if count > 0 => {
                stable = 0;
                for index in 0..count {
                    // SAFETY: the kernel wrote `count` records into the mapped read buffer.
                    let record = unsafe { &*(READ_BUF as *const Record).add(index) };
                    if record.cpu < MAX_CPUS as u16 {
                        cpus_mask |= 1u32 << u32::from(record.cpu);
                    }
                    if record.probe == 1 && record.syscall == THREAD_SELF as u32 {
                        saw_syscall = true;
                    }
                }
            }
            Ok(_) => {
                stable = stable.saturating_add(1);
            }
            Err(error) => {
                println!("dtrace: fail read {}", error.0);
                return 1;
            }
        }
        let all_done = DONE.load(Ordering::Acquire) == PRODUCERS
            && PRODUCER_CLEAR.iter().all(|word| word.load(Ordering::Acquire) == 0);
        if all_done && stable >= 2 {
            break;
        }
    }
    if dtrace_disable(1).is_err() {
        println!("dtrace: fail disable");
        return 1;
    }
    let mut leftover = 0usize;
    loop {
        match dtrace_read(READ_RECORDS) {
            Ok(0) => break,
            Ok(count) => leftover = leftover.saturating_add(count),
            Err(_) => break,
        }
    }
    let production_cpus = match dtrace_stats(&mut entries) {
        Ok(count) => count,
        Err(error) => {
            println!("dtrace: fail stats {}", error.0);
            return 1;
        }
    };
    let (produced, consumed, dropped) = totals(&entries, production_cpus);
    let cpus = cpus_mask.count_ones();
    if produced < 100_000
        || dropped != 0
        || consumed != produced
        || cpus < 1
        || !saw_syscall
    {
        println!(
            "dtrace: fail records={} consumed={} dropped={} cpus={} left={} syscall={}",
            produced, consumed, dropped, cpus, leftover, saw_syscall
        );
        return 1;
    }
    println!(
        "dtrace: records={} dropped=0 cpus={}",
        produced, cpus
    );
    println!("dtrace-ring: ok");
    0
}
