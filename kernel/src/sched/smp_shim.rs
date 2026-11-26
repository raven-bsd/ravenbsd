// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! aarch64 SMP: per-CPU state, PSCI bring-up, cross-CPU reschedule IPIs and TLB shootdown.
//!
//! This replaces the single-CPU `smp_shim` stand-in. Kernel and user threads may run on
//! application processors; the scheduler spreads user threads by default (`smp_user=off` is the
//! emergency disable), and every user mapping change is broadcast to the other CPUs through the
//! SGI mailbox below.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum CPUs in the per-CPU tables.
pub const MAX_CPUS: usize = 64;

/// SGI used to request a reschedule.
pub const SGI_RESCHEDULE: u32 = 0;
/// SGI used for a TLB shootdown.
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
    use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use super::MAX_CPUS;

    static PREEMPT: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
    /// One while this CPU holds an address-space mutex that may need a TLB shootdown.
    static UNMAP_LOCK_HELD: [AtomicBool; MAX_CPUS] = [const { AtomicBool::new(false) }; MAX_CPUS];

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

    /// Record a page-fault diagnostic stage (x86-only in this tree).
    #[inline]
    pub fn note_page_fault_phase(_phase: u64) {}

    /// Read the page-fault diagnostic stage (x86-only in this tree).
    #[allow(dead_code)]
    #[inline]
    pub fn page_fault_phase(_cpu: usize) -> u64 {
        0
    }

    /// Mark whether this CPU holds an address-space mutex across a potential TLB shootdown.
    #[inline]
    pub fn note_unmap_lock_held(active: bool) {
        if let Some(slot) = UNMAP_LOCK_HELD.get(this_cpu()) {
            slot.store(active, Ordering::Relaxed);
        }
    }

    /// True when this CPU holds an address-space mutex across a potential TLB shootdown.
    #[inline]
    pub fn unmap_lock_held() -> bool {
        UNMAP_LOCK_HELD
            .get(this_cpu())
            .is_some_and(|slot| slot.load(Ordering::Relaxed))
    }

    /// Record a wait-queue diagnostic stage.
    #[inline]
    pub fn note_wait_phase(_phase: u64) {}

    /// Read the wait-queue diagnostic stage.
    #[allow(dead_code)]
    #[inline]
    pub fn wait_phase(_cpu: usize) -> u64 {
        0
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

/// Accept IRQ delivery inside a polling wait.
///
/// The aarch64 TLB shootdown acknowledgement is a maskable IRQ SGI, while x86-64 delivers its
/// invalidation as an NMI that the target cannot mask. A CPU that waits for another CPU's thread
/// to leave (process quiesce) therefore must accept IRQ delivery even though the caller's
/// critical section keeps the preemption count raised; otherwise the target CPU's shootdown can
/// never be acknowledged and both CPUs spin forever (S755 cross-CPU teardown stall).
pub mod irq_window {
    /// Saved DAIF, restored when the window closes.
    pub struct Guard {
        flags: u64,
    }

    impl Guard {
        /// Unmask IRQ delivery on this CPU.
        pub fn new() -> Self {
            let flags: u64;
            // SAFETY: DAIF is an EL1 system register; the paired restore writes the saved value.
            unsafe {
                core::arch::asm!(
                    "mrs {flags}, daif",
                    "msr daifclr, #2",
                    flags = out(reg) flags,
                    options(nomem, nostack, preserves_flags),
                );
            }
            Self { flags }
        }
    }

    impl Drop for Guard {
        fn drop(&mut self) {
            // SAFETY: the value was captured by `new` on this CPU.
            unsafe {
                core::arch::asm!(
                    "msr daif, {flags}",
                    flags = in(reg) self.flags,
                    options(nomem, nostack, preserves_flags)
                );
            }
        }
    }
}

/// Cross-CPU TLB shootdown.
///
/// A CPU that changes a user mapping publishes the page-aligned range, sends [`super::SGI_TLB`]
/// to every other online CPU and waits for each acknowledgement before the caller may reuse the
/// old frames. The local flush is the fast path and runs without taking the mailbox lock. The
/// protocol mirrors the x86-64 `smp::shootdown` mailbox, adapted to aarch64's SGI and `tlbi`
/// instructions.
///
/// # Locking and interrupt contract
///
/// * The caller may hold any address-space lock, but must not hold the scheduler's per-CPU slot
///   lock: the SGI handler only touches this module's atomics and executes `tlbi`/`dsb`.
/// * [`ACTIVE`] serialises shootdowns system-wide, so the single shared range is unambiguous.
///   A second shooter waits with interrupts enabled (preemption stays disabled), so it can take
///   the active shooter's TLB SGI instead of timing out. The acknowledgement wait also enables
///   interrupts; the guard restores the caller's original DAIF on every path.
/// * The 100 ms deadline is diagnostic only. A late acknowledgement is still an acknowledgement:
///   the pending sequence stays published and the shooter waits for it. `false` is returned only
///   when a target could not be signalled at all.
///
/// Memory ordering: the shooter stores the range and the per-CPU pending sequence with `SeqCst`
/// before sending the SGI; the handler loads them with `SeqCst`, invalidates the range, then
/// stores the acknowledgement with `SeqCst`. The shooter's `SeqCst` acknowledgement load
/// therefore observes the flush before the caller reuses the frames.
pub mod shootdown {
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use crate::arch::aarch64::gic;
    use crate::kprintln;
    use crate::sched::smp::percpu;
    use crate::time;

    use super::MAX_CPUS;

    /// Diagnostic acknowledgement-delay bound, not a give-up bound.
    const ACK_DELAY_NS: u64 = 100_000_000;

    /// Save DAIF and mask all interrupts, mirroring `sync::disable_interrupts` without changing
    /// the shared lock module. The guard restores the exact caller state.
    fn save_and_disable_interrupts() -> u64 {
        let flags: u64;
        // SAFETY: DAIF is an EL1 system register; the paired restore writes the saved value.
        unsafe {
            core::arch::asm!(
                "mrs {flags}, daif",
                "msr daifset, #0xf",
                flags = out(reg) flags,
                options(nomem, nostack, preserves_flags),
            );
        }
        flags
    }

    fn restore_interrupts(flags: u64) {
        // SAFETY: the value was captured by `save_and_disable_interrupts` on this CPU.
        unsafe {
            core::arch::asm!(
                "msr daif, {flags}",
                flags = in(reg) flags,
                options(nomem, nostack, preserves_flags),
            );
        }
    }

    fn enable_local_interrupts() {
        // SAFETY: the vectors and GIC are installed before SMP shootdowns run.
        unsafe {
            core::arch::asm!("msr daifclr, #2", options(nomem, nostack, preserves_flags));
        }
    }

    fn disable_local_interrupts() {
        // SAFETY: the caller restores the saved DAIF when the active guard drops.
        unsafe {
            core::arch::asm!("msr daifset, #2", options(nomem, nostack, preserves_flags));
        }
    }

    static ACTIVE: AtomicBool = AtomicBool::new(false);

    /// Serialises the active request. A waiter spins with interrupts enabled so the current
    /// shooter's SGI can be taken; preemption stays disabled so the request cannot migrate.
    struct ActiveGuard {
        flags: u64,
    }

    impl ActiveGuard {
        fn acquire() -> Self {
            let flags = save_and_disable_interrupts();
            percpu::preempt_inc();
            loop {
                if !ACTIVE.swap(true, Ordering::AcqRel) {
                    break;
                }
                enable_local_interrupts();
                while ACTIVE.load(Ordering::Acquire) {
                    core::hint::spin_loop();
                }
                disable_local_interrupts();
            }
            Self { flags }
        }
    }

    impl Drop for ActiveGuard {
        fn drop(&mut self) {
            ACTIVE.store(false, Ordering::Release);
            percpu::preempt_dec();
            restore_interrupts(self.flags);
        }
    }

    static ADDRESS: AtomicU64 = AtomicU64::new(0);
    static LENGTH: AtomicU64 = AtomicU64::new(0);
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    static PENDING: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
    static ACKED: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
    static LAST: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
    static FLUSHES: [AtomicU64; MAX_CPUS] = [const { AtomicU64::new(0) }; MAX_CPUS];
    static SHOOTDOWNS: AtomicU64 = AtomicU64::new(0);
    static IPIS: AtomicU64 = AtomicU64::new(0);
    static DELIVERY_FAILURES: AtomicU64 = AtomicU64::new(0);

    /// Invalidate `start..start + len` on every online CPU.
    ///
    /// The local CPU is always flushed. Remote CPUs receive one SGI and acknowledge after
    /// executing the invalidation; a missing acknowledgement is counted, logged and reported as
    /// `false` without hanging the system.
    pub fn flush_range(start: u64, len: u64) -> bool {
        debug_assert!(
            !crate::sched::smp::percpu::unmap_lock_held(),
            "TLB shootdown while holding the address-space lock"
        );
        if len == 0 {
            return true;
        }
        let me = crate::sched::smp::percpu::this_cpu();
        let online = super::online_cpus();
        crate::arch::mmu::tlb_flush_range(start, len);
        if online <= 1 {
            return true;
        }

        let _guard = ActiveGuard::acquire();
        let sequence = SEQUENCE.fetch_add(1, Ordering::SeqCst).wrapping_add(1);
        ADDRESS.store(start, Ordering::SeqCst);
        LENGTH.store(len, Ordering::SeqCst);

        let mut cpu = 0usize;
        while cpu < online {
            if cpu != me {
                if let Some(acked) = ACKED.get(cpu) {
                    acked.store(0, Ordering::SeqCst);
                }
                if let Some(pending) = PENDING.get(cpu) {
                    pending.store(sequence, Ordering::SeqCst);
                }
            }
            cpu = cpu.saturating_add(1);
        }

        let mut delivered = [false; MAX_CPUS];
        let mut sent = 0u64;
        cpu = 0;
        while cpu < online {
            if cpu != me {
                let ok = gic::send_sgi(cpu, super::SGI_TLB).is_ok();
                if let Some(slot) = delivered.get_mut(cpu) {
                    *slot = ok;
                }
                if ok {
                    sent = sent.saturating_add(1);
                }
            }
            cpu = cpu.saturating_add(1);
        }
        IPIS.fetch_add(sent, Ordering::SeqCst);
        SHOOTDOWNS.fetch_add(1, Ordering::SeqCst);

        // A target that is inside this module waiting for the active slot must be able to take
        // the SGI. The guard restores the caller's original DAIF when this function returns.
        enable_local_interrupts();
        let deadline = time::now_ns().saturating_add(ACK_DELAY_NS);
        let mut all_acked = true;
        let mut warned = false;
        cpu = 0;
        while cpu < online {
            if cpu != me {
                if !delivered.get(cpu).copied().unwrap_or(false) {
                    // A target that could not be signalled never acknowledges. Clear its
                    // mailbox and report the failed delivery without waiting for an ack.
                    if let Some(pending) = PENDING.get(cpu) {
                        pending.store(0, Ordering::SeqCst);
                    }
                    DELIVERY_FAILURES.fetch_add(1, Ordering::SeqCst);
                    all_acked = false;
                    cpu = cpu.saturating_add(1);
                    continue;
                }
                let Some(acked) = ACKED.get(cpu) else {
                    cpu = cpu.saturating_add(1);
                    continue;
                };
                while acked.load(Ordering::SeqCst) != sequence {
                    if time::now_ns() > deadline && !warned {
                        // The target is late, not lost. Keep the request published and wait for
                        // the acknowledgement; the SGI remains pending in the GIC. The one-shot
                        // line names the target's state so a masked-interrupt stall points at
                        // its exact path instead of reporting only the CPU index.
                        warned = true;
                        let raw = crate::arch::percpu::block(cpu)
                            .current_thread
                            .load(Ordering::Acquire);
                        if raw == 0 {
                            kprintln!("aarch64-smp: tlb shootdown ack delay on cpu {cpu} idle");
                        } else {
                            // SAFETY: the per-CPU block publishes the running thread, which the
                            // scheduler keeps alive until it is switched out; the fields read
                            // here are atomics.
                            let thread = unsafe { &*(raw as *const crate::sched::thread::Thread) };
                            kprintln!(
                                "aarch64-smp: tlb shootdown ack delay on cpu {cpu} tid={} state={:?} user={} syscall={} pc={:#x}",
                                thread.tid,
                                thread.state(),
                                thread.user,
                                thread.process.last_syscall.load(Ordering::Relaxed),
                                thread.process.last_pc.load(Ordering::Relaxed)
                            );
                        }
                    }
                    core::hint::spin_loop();
                }
            }
            cpu = cpu.saturating_add(1);
        }
        disable_local_interrupts();
        all_acked
    }

    /// Handle the TLB SGI on this CPU: invalidate the published range and acknowledge it.
    pub fn handle_ipi() {
        let cpu = crate::sched::smp::percpu::this_cpu();
        let Some(pending) = PENDING.get(cpu) else {
            return;
        };
        let sequence = pending.load(Ordering::SeqCst);
        let last = LAST.get(cpu);
        if sequence == 0 || last.is_some_and(|value| value.load(Ordering::SeqCst) == sequence) {
            return;
        }
        let start = ADDRESS.load(Ordering::SeqCst);
        let len = LENGTH.load(Ordering::SeqCst);
        crate::arch::mmu::tlb_flush_range(start, len);
        if let Some(counter) = FLUSHES.get(cpu) {
            counter.fetch_add(1, Ordering::SeqCst);
        }
        if let Some(slot) = last {
            slot.store(sequence, Ordering::SeqCst);
        }
        if let Some(acked) = ACKED.get(cpu) {
            acked.store(sequence, Ordering::SeqCst);
        }
    }

    /// Number of TLB SGIs handled by `cpu` (diagnostics and the SMP selftest).
    pub fn flush_count(cpu: usize) -> u64 {
        FLUSHES
            .get(cpu)
            .map(|counter| counter.load(Ordering::Relaxed))
            .unwrap_or(0)
    }

    /// Number of shootdowns started by any CPU.
    pub fn shootdown_count() -> u64 {
        SHOOTDOWNS.load(Ordering::Relaxed)
    }

    /// Number of TLB SGIs sent to remote CPUs.
    pub fn ipi_count() -> u64 {
        IPIS.load(Ordering::Relaxed)
    }

    /// Number of remote CPUs that could not be signalled. A late acknowledgement is not a
    /// failure; only a failed SGI delivery is counted here.
    pub fn timeout_count() -> u64 {
        DELIVERY_FAILURES.load(Ordering::Relaxed)
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

    /// Prove the remote TLB shootdown: every online AP must acknowledge every published range.
    ///
    /// Each round publishes a fresh sequence and waits for all acknowledgements; the summary line
    /// quotes the real counters (rounds, SGIs sent, SGIs handled, timeouts) so the report never
    /// relies on an assumed delivery.
    fn tlb_shootdown_proof(online: usize) {
        const ROUNDS: u64 = 8;
        if online <= 1 {
            kprintln!("aarch64-smp: tlb skipped cpus=1");
            return;
        }
        let mut before = [0u64; MAX_CPUS];
        let mut cpu = 0usize;
        while cpu < online {
            if let Some(slot) = before.get_mut(cpu) {
                *slot = super::shootdown::flush_count(cpu);
            }
            cpu = cpu.saturating_add(1);
        }
        let mut rounds_ok = 0u64;
        let mut round = 0u64;
        while round < ROUNDS {
            // The address does not have to be mapped: it only feeds `tlbi vaae1is`.
            let start = 0x1000u64.saturating_add(round.saturating_mul(0x1000));
            if super::shootdown::flush_range(start, 0x1000) {
                rounds_ok = rounds_ok.saturating_add(1);
            }
            round = round.saturating_add(1);
        }
        let mut acked = 0u64;
        cpu = 0usize;
        while cpu < online {
            let previous = before.get(cpu).copied().unwrap_or(0);
            let handled = super::shootdown::flush_count(cpu).saturating_sub(previous);
            acked = acked.saturating_add(handled);
            kprintln!("aarch64-smp: tlb cpu={cpu} handled={handled}");
            cpu = cpu.saturating_add(1);
        }
        let expected_acks =
            ROUNDS.saturating_mul(u64::try_from(online.saturating_sub(1)).unwrap_or(0));
        kprintln!(
            "aarch64-smp: tlb shootdowns={} ipis={} acked={} timeouts={}",
            super::shootdown::shootdown_count(),
            super::shootdown::ipi_count(),
            acked,
            super::shootdown::timeout_count()
        );
        if rounds_ok != ROUNDS || acked != expected_acks || super::shootdown::timeout_count() != 0 {
            kprintln!(
                "aarch64-smp: tlb failed rounds={rounds_ok} expected={ROUNDS} acks={acked} expected_acks={expected_acks}"
            );
            crate::boot::exit_qemu(0x11);
        }
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
        tlb_shootdown_proof(online);
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
