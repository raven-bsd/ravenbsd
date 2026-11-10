// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `selftest=smp`: four-thread counter, per-CPU work stealing and a TLB shootdown check.
//!
//! The TLB test uses the architectural Accessed bit: every CPU reads its own test page (setting
//! A), the boot processor clears A, broadcasts a shootdown and then has every CPU read again. If
//! the invalidation reached a CPU, its second read walks the page table and sets A in memory; a
//! stale TLB entry would leave A clear.

use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use alloc::vec::Vec;

use raven_rmm::{Entry, MapError, PageFlags, PageSize, PageTable, PhysAddr, VirtAddr};

use crate::boot;
use crate::kprintln;
use crate::memory::{frame, paging};
use crate::sched::{self, sleep};

use super::{MAX_CPUS, call, online_cpus, percpu, shootdown};

/// Test pages are mapped in the low canonical half, away from the heap and kernel image.
const TLB_TEST_BASE: u64 = 0x0000_0052_0000_0000;
/// Accessed bit in a page-table entry.
const ACCESSED: u64 = 1 << 5;
/// Worker threads in the counter test.
const WORKERS: usize = 4;
/// Iterations per worker.
const ITERATIONS: u32 = 200;
/// Counter-test timeout.
const TIMEOUT_NS: u64 = 5_000_000_000;

static COUNTER: AtomicU64 = AtomicU64::new(0);
static SEEN: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
static DONE: AtomicU32 = AtomicU32::new(0);

/// Run the SMP self-test; on success this never returns.
pub fn run() -> ! {
    let cpus = online_cpus();
    match tlb_test(cpus) {
        Ok(()) => kprintln!("selftest: smp tlb ok cpus={cpus}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }

    let workers: [(&str, fn()); WORKERS] = [
        ("smp-worker-0", worker0),
        ("smp-worker-1", worker1),
        ("smp-worker-2", worker2),
        ("smp-worker-3", worker3),
    ];
    for (index, (name, entry)) in workers.iter().enumerate() {
        let target = index.min(cpus.saturating_sub(1));
        if let Err(error) = sched::spawn_kernel_on(target, name, *entry) {
            kprintln!("selftest: smp spawn failed: {error:?}");
            boot::exit_qemu(0x11);
        }
    }
    if let Err(error) = sched::spawn_kernel("smp-monitor", monitor) {
        kprintln!("selftest: smp spawn failed: {error:?}");
        boot::exit_qemu(0x11);
    }
    sched::start()
}

fn worker0() {
    worker(0);
}

fn worker1() {
    worker(1);
}

fn worker2() {
    worker(2);
}

fn worker3() {
    worker(3);
}

fn worker(index: usize) {
    let _ = index;
    let mut iteration = 0u32;
    while iteration < ITERATIONS {
        COUNTER.fetch_add(1, Ordering::SeqCst);
        let cpu = percpu::this_cpu();
        if let Some(seen) = SEEN.get(cpu) {
            seen.fetch_add(1, Ordering::SeqCst);
        }
        sched::yield_now();
        iteration = iteration.saturating_add(1);
    }
    DONE.fetch_add(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

fn monitor() {
    let start = sleep::now_ns();
    while DONE.load(Ordering::SeqCst) < WORKERS as u32 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            kprintln!("selftest: smp counter timeout");
            boot::exit_qemu(0x11);
        }
        sleep::sleep_ns(1_000_000);
    }

    let cpus = online_cpus();
    let active = cpus.min(WORKERS);
    let total = COUNTER.load(Ordering::SeqCst);
    let expected = u64::try_from(WORKERS)
        .unwrap_or(0)
        .saturating_mul(u64::from(ITERATIONS));
    let mut used = 0usize;
    let mut cpu = 0usize;
    while cpu < active {
        if SEEN
            .get(cpu)
            .is_some_and(|seen| seen.load(Ordering::SeqCst) > 0)
        {
            used = used.saturating_add(1);
        }
        cpu = cpu.saturating_add(1);
    }
    if total != expected || used != active {
        kprintln!(
            "selftest: smp counter failed total={total} expected={expected} used={used} active={active} cpus={cpus}"
        );
        boot::exit_qemu(0x11);
    }
    kprintln!("selftest: smp counter ok cpus={cpus} counter={total}");
    kprintln!(
        "selftest: smp ok cpus={cpus} switches={}",
        sched::switch_count()
    );
    boot::exit_qemu(0x10)
}

fn tlb_test(cpus: usize) -> Result<(), &'static str> {
    if cpus == 0 || cpus > MAX_CPUS {
        return Err("invalid CPU count");
    }
    let mut frames: Vec<frame::PhysAddr> = Vec::with_capacity(cpus);
    let mut index = 0usize;
    while index < cpus {
        let physical = frame::alloc_zeroed_frame().ok_or("out of frames")?;
        frames.push(physical);
        index = index.saturating_add(1);
    }

    let mapped = paging::with_kernel_mapper(|mapper| {
        for (index, physical) in frames.iter().enumerate() {
            let address = TLB_TEST_BASE
                .checked_add((index as u64).saturating_mul(frame::PAGE_SIZE))
                .ok_or(MapError::NotCanonical)?;
            let virt = VirtAddr::new(address).ok_or(MapError::NotCanonical)?;
            // No GLOBAL bit: INVLPG must invalidate these entries.
            let flags = PageFlags::from_bits(0x8000_0000_0000_0003);
            mapper.map(virt, PhysAddr(physical.0), flags, PageSize::Size4K)?;
        }
        Ok::<(), MapError>(())
    });
    if !matches!(mapped, Ok(Ok(()))) {
        cleanup(&frames);
        return Err("mapping failed");
    }

    let mut result = Ok(());
    let mut cpu = 0usize;
    while cpu < cpus {
        let address = TLB_TEST_BASE.saturating_add((cpu as u64).saturating_mul(frame::PAGE_SIZE));
        if call::probe(cpu, address).is_none() {
            result = Err("first probe failed");
            break;
        }
        cpu = cpu.saturating_add(1);
    }
    if result.is_err() {
        cleanup(&frames);
        return result;
    }

    cpu = 0;
    while cpu < cpus {
        let address = TLB_TEST_BASE.saturating_add((cpu as u64).saturating_mul(frame::PAGE_SIZE));
        if set_accessed(address, false).is_err() {
            cleanup(&frames);
            return Err("cannot clear accessed bit");
        }
        cpu = cpu.saturating_add(1);
    }

    let length = (cpus as u64).saturating_mul(frame::PAGE_SIZE);
    let me = percpu::this_cpu();
    let mut flushes_before: Vec<u64> = Vec::with_capacity(cpus);
    let mut counter = 0usize;
    while counter < cpus {
        flushes_before.push(shootdown::flush_count(counter));
        counter = counter.saturating_add(1);
    }
    shootdown::flush_range(TLB_TEST_BASE, length);

    cpu = 0;
    while cpu < cpus {
        let address = TLB_TEST_BASE.saturating_add((cpu as u64).saturating_mul(frame::PAGE_SIZE));
        if call::probe(cpu, address).is_none() {
            result = Err("second probe failed");
            break;
        }
        cpu = cpu.saturating_add(1);
    }
    if result.is_err() {
        cleanup(&frames);
        return result;
    }

    cpu = 0;
    while cpu < cpus {
        if cpu != me && shootdown::flush_count(cpu) == flushes_before.get(cpu).copied().unwrap_or(0)
        {
            result = Err("CPU did not acknowledge the shootdown");
            break;
        }
        let address = TLB_TEST_BASE.saturating_add((cpu as u64).saturating_mul(frame::PAGE_SIZE));
        match leaf_entry(address) {
            Ok(entry) if entry.raw() & ACCESSED != 0 => {}
            Ok(_) => {
                result = Err("stale TLB entry");
                break;
            }
            Err(reason) => {
                result = Err(reason);
                break;
            }
        }
        cpu = cpu.saturating_add(1);
    }

    cleanup(&frames);
    result
}

fn leaf_entry(address: u64) -> Result<Entry, &'static str> {
    let root = paging::kernel_root().ok_or("no kernel root")?;
    let memory = paging::KernelPhysMemory;
    let virt = VirtAddr::new(address).ok_or("bad address")?;
    let mut table = PageTable::new(root, 3);
    while table.level() > 0 {
        let index = table.index_of(virt);
        table = table.next(&memory, index).ok_or("missing table")?;
    }
    let index = table.index_of(virt);
    table.entry(&memory, index).ok_or("missing entry")
}

fn set_accessed(address: u64, set: bool) -> Result<(), &'static str> {
    let root = paging::kernel_root().ok_or("no kernel root")?;
    let memory = paging::KernelPhysMemory;
    let virt = VirtAddr::new(address).ok_or("bad address")?;
    let mut table = PageTable::new(root, 3);
    while table.level() > 0 {
        let index = table.index_of(virt);
        table = table.next(&memory, index).ok_or("missing table")?;
    }
    let index = table.index_of(virt);
    let entry = table.entry(&memory, index).ok_or("missing entry")?;
    let raw = if set {
        entry.raw() | ACCESSED
    } else {
        entry.raw() & !ACCESSED
    };
    if table.set_entry(&memory, index, Entry::from_raw(raw)) {
        Ok(())
    } else {
        Err("cannot write entry")
    }
}

fn cleanup(frames: &[frame::PhysAddr]) {
    let _ = paging::with_kernel_mapper(|mapper| {
        for index in 0..frames.len() {
            let address =
                TLB_TEST_BASE.saturating_add((index as u64).saturating_mul(frame::PAGE_SIZE));
            if let Some(virt) = VirtAddr::new(address) {
                let _ = mapper.unmap(virt);
            }
        }
    });
    for physical in frames {
        let _ = frame::free_frame(*physical);
    }
}
