// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 SMP: per-CPU state, PSCI bring-up and cross-CPU reschedule IPIs.
//!
//! This replaces the single-CPU `smp_shim` stand-in. Kernel threads may run on application
//! processors; user threads still stay on the boot processor until the per-CPU EL0/syscall and
//! FP/SIMD slice lands (`sched::enqueue`/`target_cpu` gate that explicitly).

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum CPUs in the per-CPU tables.
pub const MAX_CPUS: usize = 64;

/// SGI used to request a reschedule.
pub const SGI_RESCHEDULE: u32 = 0;
/// SGI used for a TLB shootdown.
#[allow(dead_code)] // Wired in `irq::dispatch`; the range broadcast lands with user SMP.
pub const SGI_TLB: u32 = 1;

static ONLINE: AtomicUsize = AtomicUsize::new(1);

/// Number of CPUs currently online, including the boot processor.
pub fn online_cpus() -> usize {
    ONLINE.load(Ordering::Acquire).clamp(1, MAX_CPUS)
}

/// Publish the number of CPUs that completed bring-up.
pub(crate) fn set_online(count: usize) {
    ONLINE.store(count.clamp(1, MAX_CPUS), Ordering::Release);
}

/// Bring up the application processors and install the SGI gates.
pub fn platform_init() {
    bringup::platform_init();
}

/// Per-CPU accessors shared by the scheduler, `sync` and the architecture code.
pub mod percpu {
    use core::sync::atomic::{AtomicU32, Ordering};

    use super::MAX_CPUS;

    static PREEMPT: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

    /// Index of the CPU executing this code.
    #[inline]
    pub fn this_cpu() -> usize {
        crate::arch::percpu::this_cpu()
    }

    /// Current preemption nesting depth of this CPU.
    #[inline]
    pub fn preempt_count() -> u32 {
        PREEMPT
            .get(this_cpu())
            .map(|counter| counter.load(Ordering::SeqCst))
            .unwrap_or(0)
    }

    /// Install a thread's saved preemption depth on this CPU.
    #[inline]
    pub fn set_preempt_count(value: u32) {
        if let Some(counter) = PREEMPT.get(this_cpu()) {
            counter.store(value, Ordering::SeqCst);
        }
    }

    /// Enter one nesting level.
    #[inline]
    pub fn preempt_inc() {
        if let Some(counter) = PREEMPT.get(this_cpu()) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Leave one nesting level.
    #[inline]
    pub fn preempt_dec() {
        if let Some(counter) = PREEMPT.get(this_cpu()) {
            counter.fetch_sub(1, Ordering::SeqCst);
        }
    }

    /// Record the current thread pointer on this CPU.
    #[inline]
    pub fn set_current_thread(value: u64) {
        crate::arch::percpu::set_current_thread(value);
    }

    /// Record the kernel stack top on this CPU.
    #[inline]
    pub fn set_kernel_rsp(value: u64) {
        crate::arch::percpu::set_kernel_sp(value);
    }

    /// Mark one CPU online after its scheduler slot is installed.
    #[inline]
    pub fn mark_online(cpu: usize) {
        crate::arch::percpu::mark_online(cpu);
    }
}

/// Application-processor bring-up and cross-CPU wakeups.
pub mod bringup {
    /// Start the secondary CPUs and publish how many completed bring-up.
    pub fn platform_init() {
        let _ = crate::arch::aarch64::gic::init_cpu();
        let online = crate::arch::aarch64::ap_start::bring_up();
        super::set_online(online);
    }

    /// Ask `cpu` to reschedule.
    pub fn send_reschedule(cpu: usize) {
        let _ = crate::arch::aarch64::gic::send_sgi(cpu, super::SGI_RESCHEDULE);
    }
}

/// Cross-CPU TLB shootdown.
///
/// The first SMP slice keeps the local flush only: no user address space is installed on an
/// application processor yet, so every AP's TTBR0 is the shared empty root. The SGI 1 handler is
/// wired so the range broadcast can be completed with the user-SMP slice without another IRQ
/// change.
pub mod shootdown {
    use core::sync::atomic::{AtomicU64, Ordering};

    use super::MAX_CPUS;

    static FLUSHES: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];

    /// Flush the local TLB after a mapping change.
    pub fn flush_range(_start: u64, _len: u64) -> bool {
        crate::arch::mmu::tlb_flush_all();
        true
    }

    /// Handle the TLB SGI on this CPU.
    pub fn handle_ipi() {
        crate::arch::mmu::tlb_flush_all();
        let cpu = crate::sched::smp::percpu::this_cpu();
        if let Some(counter) = FLUSHES.get(cpu) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }

    /// Number of TLB SGIs handled by `cpu` (diagnostics).
    #[allow(dead_code)]
    #[must_use]
    pub fn flush_count(cpu: usize) -> u64 {
        FLUSHES
            .get(cpu)
            .map(|counter| counter.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

/// The kernel-thread SMP proof used by `selftest=smp`.
///
/// Four pinned kernel workers count increments on the online CPUs and yield on every iteration,
/// so an application processor can only finish its worker if it keeps resuming after timer
/// interrupts. Each worker records the CPU it ran on without touching the console; the boot
/// processor's monitor reads those counters and proves that every AP ran one before exiting
/// QEMU with success.
///
/// A second pair of kernel threads is pinned to the first AP and alternates between two known
/// `Q31` values across `yield_now()`. Both run on the same CPU, so each switch must park the
/// outgoing thread's FP/SIMD area and restore the incoming one; without that the resumed thread
/// would observe its peer's pattern.
pub mod selftest {
    use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

    use crate::kprintln;
    use crate::sched;

    use super::{MAX_CPUS, percpu};

    const WORKERS: usize = 4;
    const ITERATIONS: u32 = 2_000;
    /// Rounds each FP/SIMD ping-pong thread runs.
    const FPU_ROUNDS: u32 = 200;

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    static DONE: AtomicU32 = AtomicU32::new(0);
    static FPU_DONE: AtomicU32 = AtomicU32::new(0);
    static FPU_FAILURES: AtomicU32 = AtomicU32::new(0);
    /// Per-CPU worker entries. APs only bump their own slot; the BSP monitor prints it.
    static SEEN: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];

    /// Run the kernel-thread SMP proof; on success this exits QEMU.
    pub fn run() -> ! {
        let cpus = super::online_cpus();
        kprintln!("aarch64-smp: online cpus={cpus}");
        let workers: [(&str, fn()); WORKERS] = [
            ("aarch64-smp-worker-0", worker0),
            ("aarch64-smp-worker-1", worker1),
            ("aarch64-smp-worker-2", worker2),
            ("aarch64-smp-worker-3", worker3),
        ];
        for (index, (name, entry)) in workers.iter().enumerate() {
            let target = index.min(cpus.saturating_sub(1));
            if let Err(error) = sched::spawn_kernel_on(target, name, *entry) {
                kprintln!("aarch64-smp: spawn failed: {error:?}");
                crate::boot::exit_qemu(0x11);
            }
        }
        // Both FP/SIMD threads share the first application processor so their Q31 writes
        // interleave through real context switches.
        let fpu_cpu = 1.min(cpus.saturating_sub(1));
        for (name, entry) in [
            ("aarch64-smp-fpu-a", fpu_a as fn()),
            ("aarch64-smp-fpu-b", fpu_b as fn()),
        ] {
            if let Err(error) = sched::spawn_kernel_on(fpu_cpu, name, entry) {
                kprintln!("aarch64-smp: fpu spawn failed: {error:?}");
                crate::boot::exit_qemu(0x11);
            }
        }
        if let Err(error) = sched::spawn_kernel("aarch64-smp-monitor", monitor) {
            kprintln!("aarch64-smp: monitor spawn failed: {error:?}");
            crate::boot::exit_qemu(0x11);
        }
        sched::start()
    }

    fn worker0() {
        worker();
    }

    fn worker1() {
        worker();
    }

    fn worker2() {
        worker();
    }

    fn worker3() {
        worker();
    }

    fn worker() {
        let cpu = percpu::this_cpu();
        if let Some(seen) = SEEN.get(cpu) {
            seen.fetch_add(1, Ordering::SeqCst);
        }
        let mut iteration = 0u32;
        while iteration < ITERATIONS {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            sched::yield_now();
            iteration = iteration.saturating_add(1);
        }
        DONE.fetch_add(1, Ordering::SeqCst);
        sched::exit_thread(0)
    }

    fn fpu_a() {
        fpu_ping_pong(0xa5a5_0000_0000_0000);
    }

    fn fpu_b() {
        fpu_ping_pong(0x5a5a_0000_0000_0000);
    }

    /// Write a private pattern into `Q31`, yield, and require it back after the switch.
    ///
    /// The kernel is soft-float, so only the scheduler's per-thread save/restore can explain a
    /// preserved value: the sibling thread on this CPU overwrites `Q31` on every round.
    fn fpu_ping_pong(tag: u64) {
        let mut round = 0u32;
        while round < FPU_ROUNDS {
            let expected = tag | u64::from(round);
            set_q31(expected);
            sched::yield_now();
            if read_q31() != expected {
                FPU_FAILURES.fetch_add(1, Ordering::SeqCst);
            }
            round = round.saturating_add(1);
        }
        FPU_DONE.fetch_add(1, Ordering::SeqCst);
        sched::exit_thread(0)
    }

    fn set_q31(value: u64) {
        // SAFETY: `Q31` is not an ABI register the soft-float compiler allocates, and the `.arch`
        // directive enables the FP/SIMD encodings this target does not assemble by default.
        unsafe {
            core::arch::asm!(
                ".arch armv8-a+fp+simd",
                "fmov d31, {value}",
                value = in(reg) value,
                options(nostack, preserves_flags),
            );
        }
    }

    fn read_q31() -> u64 {
        let value: u64;
        // SAFETY: as in `set_q31`; the read has no side effects.
        unsafe {
            core::arch::asm!(
                ".arch armv8-a+fp+simd",
                "fmov {value}, d31",
                value = out(reg) value,
                options(nostack, preserves_flags),
            );
        }
        value
    }

    fn monitor() {
        let start = crate::time::now_ns();
        while DONE.load(Ordering::SeqCst) < WORKERS as u32 {
            if crate::time::now_ns().saturating_sub(start) > 10_000_000_000 {
                kprintln!("aarch64-smp: timeout done={}", DONE.load(Ordering::SeqCst));
                crate::boot::exit_qemu(0x11);
            }
            crate::sched::sleep::sleep_ns(1_000_000);
        }
        let expected = u64::try_from(WORKERS)
            .unwrap_or(0)
            .saturating_mul(u64::from(ITERATIONS));
        let total = COUNTER.load(Ordering::SeqCst);
        if total != expected {
            kprintln!("aarch64-smp: counter={total} expected={expected}");
            crate::boot::exit_qemu(0x11);
        }
        let deadline = crate::time::now_ns().saturating_add(10_000_000_000);
        while FPU_DONE.load(Ordering::SeqCst) < 2 {
            if crate::time::now_ns() > deadline {
                kprintln!(
                    "aarch64-smp: fpu timeout done={}",
                    FPU_DONE.load(Ordering::SeqCst)
                );
                crate::boot::exit_qemu(0x11);
            }
            crate::sched::sleep::sleep_ns(1_000_000);
        }
        if FPU_FAILURES.load(Ordering::SeqCst) != 0 {
            kprintln!(
                "aarch64-smp: fpu state lost failures={}",
                FPU_FAILURES.load(Ordering::SeqCst)
            );
            crate::boot::exit_qemu(0x11);
        }
        kprintln!("aarch64-smp: fpu ok rounds={FPU_ROUNDS}");
        let online = super::online_cpus();
        let mut cpu = 1usize;
        while cpu < online {
            let seen = SEEN
                .get(cpu)
                .map_or(0, |value| value.load(Ordering::SeqCst));
            if seen == 0 {
                kprintln!("aarch64-smp: worker cpu={cpu} did not run");
                crate::boot::exit_qemu(0x11);
            }
            // Printed by the BSP: AP workers never touch the shared console lock.
            kprintln!("aarch64-smp: worker cpu={cpu} runs={seen}");
            cpu = cpu.saturating_add(1);
        }
        kprintln!("aarch64-smp: ok cpus={online} counter={total}");
        crate::boot::exit_qemu(0x10)
    }
}
