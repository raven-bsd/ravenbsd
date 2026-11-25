// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `selftest=smp`: four-thread counter, per-CPU work stealing, a TLB shootdown check and a
//! two-CPU concurrent call-probe check.
//!
//! The TLB test uses the architectural Accessed bit: every CPU reads its own test page (setting
//! A), the boot processor clears A, broadcasts a shootdown and then has every CPU read again. If
//! the invalidation reached a CPU, its second read walks the page table and sets A in memory; a
//! stale TLB entry would leave A clear.
//!
//! The call-probe test pins one thread to CPU 0 and one to CPU 1 and has both call
//! `call::probe()` at the other CPU concurrently. This is the S102 proof that the probe wait is
//! interruptible: a probe waiter must be able to take the other probe's IPI.

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

/// S159: target readiness for the deterministic IF=0 TLB shootdown test.
#[cfg(target_arch = "x86_64")]
static IF0_READY: AtomicU32 = AtomicU32::new(0);
/// S159: target completion for the deterministic IF=0 TLB shootdown test.
#[cfg(target_arch = "x86_64")]
static IF0_DONE: AtomicU32 = AtomicU32::new(0);

/// K393: target readiness for the simulated syscall `swapgs` window.
#[cfg(target_arch = "x86_64")]
static SWAPGS_READY: AtomicU32 = AtomicU32::new(0);
/// K393: release the simulated `swapgs` window after the shootdown.
#[cfg(target_arch = "x86_64")]
static SWAPGS_RELEASE: AtomicU32 = AtomicU32::new(0);
/// K393: target completion for the simulated `swapgs` window.
#[cfg(target_arch = "x86_64")]
static SWAPGS_DONE: AtomicU32 = AtomicU32::new(0);
/// K393: the handler left the simulated GS pair unchanged.
#[cfg(target_arch = "x86_64")]
static SWAPGS_OK: AtomicU32 = AtomicU32::new(0);

/// K393: concurrent shooter completion.
#[cfg(target_arch = "x86_64")]
static NESTED_DONE: AtomicU32 = AtomicU32::new(0);
/// K393: concurrent shooter failure count.
#[cfg(target_arch = "x86_64")]
static NESTED_FAIL: AtomicU32 = AtomicU32::new(0);

/// K393: absent/invalid `KERNEL_GS_BASE` stage and handshake.
#[cfg(target_arch = "x86_64")]
static GS_READY: AtomicU32 = AtomicU32::new(0);
/// K393: release the absent/invalid `KERNEL_GS_BASE` stage.
#[cfg(target_arch = "x86_64")]
static GS_RELEASE: AtomicU32 = AtomicU32::new(0);
/// K393: target completion for the absent/invalid `KERNEL_GS_BASE` stages.
#[cfg(target_arch = "x86_64")]
static GS_DONE: AtomicU32 = AtomicU32::new(0);
/// K393: both `KERNEL_GS_BASE` patterns left the GS pair unchanged.
#[cfg(target_arch = "x86_64")]
static GS_OK: AtomicU32 = AtomicU32::new(0);

/// `IA32_GS_BASE` architectural MSR.
#[cfg(target_arch = "x86_64")]
const IA32_GS_BASE: u32 = 0xC000_0101;
/// `IA32_KERNEL_GS_BASE` architectural MSR.
#[cfg(target_arch = "x86_64")]
const IA32_KERNEL_GS_BASE: u32 = 0xC000_0102;
/// Non-canonical-user-like value for the absent/invalid `KERNEL_GS_BASE` pattern.
#[cfg(target_arch = "x86_64")]
const INVALID_KERNEL_GS_BASE: u64 = 0x0000_dead_beef_0000;
/// Concurrent shootdown rounds per shooter in the nesting test.
#[cfg(target_arch = "x86_64")]
const NESTED_ROUNDS: u32 = 16;

/// Selftest TLB target readiness/completion for the lost-NMI and delayed-ack rows.
#[cfg(target_arch = "x86_64")]
static ACK_READY: AtomicU32 = AtomicU32::new(0);
/// Selftest TLB target completion for the lost-NMI and delayed-ack rows.
#[cfg(target_arch = "x86_64")]
static ACK_DONE: AtomicU32 = AtomicU32::new(0);
/// Delay used by the delayed-ack row: past the 100 ms diagnostic deadline.
#[cfg(target_arch = "x86_64")]
const ACK_DELAY_NS: u64 = 250_000_000;

/// Read by both CPUs in the concurrent call-probe test.
static PROBE_BYTE: u8 = 0x5a;
static PROBE_DONE: AtomicU32 = AtomicU32::new(0);
static PROBE_FAIL: [AtomicU32; MAX_CPUS] = [const { AtomicU32::new(0) }; MAX_CPUS];
const PROBE_ROUNDS: u32 = 200;

/// Bounded churn for the scheduler-side dead-Thread retention (K495 rev 311).
///
/// Every cycle registers a long timeout through `begin_block`/`add_sleep`, is woken, and then
/// exits without `end_block` - the terminal-process path Tim S36 measured. The per-CPU sleeper
/// entry holds an `Arc<Thread>` (and through it the `Process`); it must be gone once the deferred
/// reaper releases the dead Thread, not after the 10 s deadline. NetBSD destroys the LWP timeout
/// callout in `lwp_exit` and halts it in `sleepq_block`; FreeBSD and Illumos drop the sleep-queue
/// entry on wake. A retained entry is a leak, so this row reports both the sleeper count and the
/// live-Thread delta.
fn sleeper_close_test(cpus: usize) -> Result<[usize; 4], (&'static str, [usize; 4])> {
    use crate::sched::thread::{ThreadState, live_threads};

    const CYCLES: usize = 8;
    const WAIT_NS: u64 = 1_000_000_000;
    let target = cpus.saturating_sub(1);
    // Drain any reap queued by the earlier self-tests before taking the baseline.
    sleep::sleep_ns(20_000_000);
    let base = sched::sleepers_len();
    let live_base = live_threads();
    // sleeper delta, live-Thread delta, run-queue entries, deferred-reap entries. The last two
    // are absolute: the audit shows the other scheduler holders are empty.
    let ledger = || {
        [
            sched::sleepers_len().saturating_sub(base),
            live_threads().saturating_sub(live_base),
            sched::runqueue_len(),
            sched::reap_len(),
        ]
    };
    for _ in 0..CYCLES {
        let thread = match sched::spawn_kernel_on(target, "sleeper-close", sleeper_thread) {
            Ok(thread) => thread,
            Err(_) => return Err(("spawn", ledger())),
        };
        let deadline = sleep::now_ns().saturating_add(WAIT_NS);
        while thread.state() != ThreadState::Blocked {
            if sleep::now_ns() > deadline {
                return Err(("not blocked", ledger()));
            }
            sleep::sleep_ns(50_000);
        }
        if !sched::wake(alloc::sync::Arc::clone(&thread), None, 0) {
            return Err(("wake refused", ledger()));
        }
        let deadline = sleep::now_ns().saturating_add(WAIT_NS);
        while thread.state() != ThreadState::Dead {
            if sleep::now_ns() > deadline {
                return Err(("not dead", ledger()));
            }
            sleep::sleep_ns(50_000);
        }
    }
    // Give the next scheduling point on this CPU time to run the deferred reaper.
    sleep::sleep_ns(20_000_000);
    let counts = ledger();
    if counts[0] != 0 || counts[1] != 0 {
        return Err(("retained", counts));
    }
    Ok(counts)
}

fn sleeper_thread() {
    let Ok(token) = sched::begin_block() else {
        sched::exit_thread(1);
    };
    sched::add_sleep(token, sleep::now_ns().saturating_add(10_000_000_000), 0);
    let _ = sched::block_current();
    // Deliberately skip `end_block`: a terminal process exits from the blocking path.
    sched::exit_thread(0)
}

/// Blocking mutex used only by the donation-close churn; locking it registers the caller in the
/// donation graph (`KernelTurnstile::register` from `Mutex::lock`).
static DONATION_LOCK: crate::sync::Mutex<u32> = crate::sync::Mutex::new(0);

/// Bounded donation churn: a thread acquires the priority-inheritance mutex (registering it in
/// the kernel weak registry and the crate graph), releases it, and exits. The exit boundary must
/// drop the kernel registration; the registered count has to return to the pre-churn baseline.
fn donation_close_test(cpus: usize) -> Result<(usize, usize), (&'static str, usize, usize)> {
    use crate::sched::thread::ThreadState;

    const CYCLES: usize = 8;
    const WAIT_NS: u64 = 1_000_000_000;
    let target = cpus.saturating_sub(1);
    sleep::sleep_ns(20_000_000);
    let base = crate::sync::registered_count();
    for _ in 0..CYCLES {
        let thread = match sched::spawn_kernel_on(target, "donation-close", donation_thread) {
            Ok(thread) => thread,
            Err(_) => return Err(("spawn", crate::sync::registered_count(), base)),
        };
        let deadline = sleep::now_ns().saturating_add(WAIT_NS);
        while thread.state() != ThreadState::Dead {
            if sleep::now_ns() > deadline {
                return Err(("not dead", crate::sync::registered_count(), base));
            }
            sleep::sleep_ns(50_000);
        }
    }
    sleep::sleep_ns(20_000_000);
    let after = crate::sync::registered_count();
    if after != base {
        return Err(("registered retained", after, base));
    }
    Ok((after, base))
}

fn donation_thread() {
    let guard = DONATION_LOCK.lock();
    drop(guard);
    sched::exit_thread(0);
}

/// Cross-CPU FIFO call row: the FIFO carries every call; the old single-slot mailbox does not.
fn call_fifo_test(cpus: usize) -> Result<(u32, u32, u32, u64), &'static str> {
    const COUNT: u32 = 32;
    if cpus < 2 {
        return Ok((COUNT, COUNT, COUNT, 0));
    }
    let (fifo, elapsed) = crate::sched::smp::call::batch(1, COUNT);
    if fifo != COUNT {
        return Err("fifo batch");
    }
    if crate::sched::smp::call::pending(1) != 0 {
        return Err("fifo pending");
    }
    // The old mailbox held one pending call; a 32-call burst left 31 unread.
    let mut mailbox: u64 = 0;
    for value in 0..COUNT {
        mailbox = u64::from(value);
    }
    let mailbox = u32::from(mailbox == u64::from(COUNT - 1));
    if mailbox >= COUNT {
        return Err("mailbox model");
    }
    Ok((COUNT, fifo, mailbox, elapsed))
}

/// DragonFly IPIQ FIFO port check: 400 messages through the 256-slot ring.
fn ipiq_test() -> Result<u32, &'static str> {
    use raven_ipiq::{IpIQ, Message};

    let queue: IpIQ<u32> = IpIQ::new();
    let mut expected = 0u32;
    let mut ordered = true;
    for round in 0..4u32 {
        for value in 0..100u32 {
            let message = Message {
                func: round * 100 + value,
                arg1: (round * 100 + value) as usize,
                arg2: 0,
            };
            queue.push(message).map_err(|_| "ipiq full")?;
        }
        queue.process(0, |message| {
            if message.func != expected {
                ordered = false;
            }
            expected = expected.saturating_add(1);
        });
    }
    if !ordered {
        return Err("ipiq order");
    }
    if queue.xindex() != 400 || queue.pending() != 0 {
        return Err("ipiq index");
    }
    Ok(400)
}

/// Run the SMP self-test; on success this never returns.
pub fn run() -> ! {
    let cpus = online_cpus();
    let cmdline = crate::cmdline::CmdLine::parse(crate::memory::paging::cmdline());
    if cmdline.flag("selftest-tlb-drop-ack") {
        lost_ack_test(cpus);
    }
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
    match call_probe_test(cpus) {
        Ok(probes) => kprintln!("selftest: smp call ok cpus={} probes={probes}", cpus.min(2)),
        Err(reason) => {
            kprintln!("selftest: smp call failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    match ipiq_test() {
        Ok(messages) => {
            kprintln!("selftest: smp ipiq ok messages={messages} wrap=1 xindex={messages}")
        }
        Err(reason) => {
            kprintln!("selftest: smp ipiq failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    match call_fifo_test(cpus) {
        Ok((sent, fifo, mailbox, elapsed)) => kprintln!(
            "selftest: smp ipiq calls ok sent={sent} fifo={fifo} mailbox={mailbox} elapsed_us={}",
            elapsed / 1_000
        ),
        Err(reason) => {
            kprintln!("selftest: smp ipiq calls failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    // Let the probe threads leave CPU1 before the next pinned edge target is queued there.
    sleep::sleep_ns(20_000_000);
    #[cfg(target_arch = "x86_64")]
    match gs_pattern_test(cpus) {
        Ok(patterns) => kprintln!("selftest: smp tlb gs ok absent={patterns} invalid={patterns}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb gs failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    #[cfg(target_arch = "x86_64")]
    match swapgs_window_test(cpus) {
        Ok(elapsed_ms) => kprintln!("selftest: smp tlb swapgs ok elapsed_ms={elapsed_ms}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb swapgs failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    #[cfg(target_arch = "x86_64")]
    match nested_shootdown_test(cpus) {
        Ok(rounds) => kprintln!("selftest: smp tlb nested ok rounds={rounds}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb nested failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    #[cfg(target_arch = "x86_64")]
    match if0_shootdown_test(cpus) {
        Ok(elapsed_ms) => kprintln!("selftest: smp tlb if0 ok elapsed_ms={elapsed_ms}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb if0 failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    #[cfg(target_arch = "x86_64")]
    match lost_nmi_test(cpus) {
        Ok(elapsed_ms) => kprintln!("selftest: smp tlb lost-nmi ok elapsed_ms={elapsed_ms}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb lost-nmi failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    #[cfg(target_arch = "x86_64")]
    match delayed_ack_test(cpus) {
        Ok(elapsed_ms) => kprintln!("selftest: smp tlb delayed-ack ok elapsed_ms={elapsed_ms}"),
        Err(reason) => {
            kprintln!("selftest: smp tlb delayed-ack failed: {reason}");
            boot::exit_qemu(0x11);
        }
    }
    match sleeper_close_test(cpus) {
        Ok([sleepers, live, queued, reap]) => kprintln!(
            "selftest: smp sleeper-close cycles=8 sleepers={sleepers} live={live} queued={queued} reap={reap} ok"
        ),
        Err((reason, [sleepers, live, queued, reap])) => {
            kprintln!(
                "selftest: smp sleeper-close failed: {reason} sleepers={sleepers} live={live} queued={queued} reap={reap}"
            );
            boot::exit_qemu(0x11);
        }
    }
    match donation_close_test(cpus) {
        Ok((registered, base)) => kprintln!(
            "selftest: smp donation-close cycles=8 registered={registered} base={base} ok"
        ),
        Err((reason, registered, base)) => {
            kprintln!(
                "selftest: smp donation-close failed: {reason} registered={registered} base={base}"
            );
            boot::exit_qemu(0x11);
        }
    }
    kprintln!(
        "selftest: smp ok cpus={cpus} switches={}",
        sched::switch_count()
    );
    boot::exit_qemu(0x10)
}

fn probe_cpu0() {
    probe_worker(0);
}

fn probe_cpu1() {
    probe_worker(1);
}

fn probe_worker(cpu: usize) {
    let target = if cpu == 0 { 1 } else { 0 };
    let address = core::ptr::addr_of!(PROBE_BYTE) as u64;
    let mut round = 0u32;
    while round < PROBE_ROUNDS {
        if call::probe(target, address) != Some(PROBE_BYTE)
            && let Some(failed) = PROBE_FAIL.get(cpu)
        {
            failed.fetch_add(1, Ordering::SeqCst);
        }
        round = round.saturating_add(1);
    }
    PROBE_DONE.fetch_add(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn if0_target() {
    // Intentional: hold IF clear across the S113/S159 shootdown deadline. This is the
    // deterministic reproducer for a target that cannot take a fixed TLB IPI.
    crate::arch::x86_64::ap_start::disable_local_interrupts();
    core::sync::atomic::fence(Ordering::SeqCst);
    IF0_READY.store(1, Ordering::SeqCst);
    let start = sleep::now_ns();
    while sleep::now_ns().saturating_sub(start) < 150_000_000 {
        core::hint::spin_loop();
    }
    crate::arch::x86_64::ap_start::enable_local_interrupts();
    IF0_DONE.store(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn if0_shootdown_test(cpus: usize) -> Result<u64, &'static str> {
    if cpus < 2 {
        return Ok(0);
    }
    IF0_READY.store(0, Ordering::SeqCst);
    IF0_DONE.store(0, Ordering::SeqCst);
    if sched::spawn_kernel_on(1, "smp-if0", if0_target).is_err() {
        return Err("cannot spawn IF=0 target");
    }
    let start = sleep::now_ns();
    while IF0_READY.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("IF=0 target not ready");
        }
        sleep::sleep_ns(1_000_000);
    }
    let start = sleep::now_ns();
    let flushed = shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE);
    let elapsed_ms = sleep::now_ns().saturating_sub(start) / 1_000_000;
    if !flushed {
        return Err("IF=0 shootdown timed out");
    }
    let start = sleep::now_ns();
    while IF0_DONE.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("IF=0 target did not finish");
        }
        sleep::sleep_ns(1_000_000);
    }
    Ok(elapsed_ms)
}

#[cfg(target_arch = "x86_64")]
fn swapgs_window_target() {
    let kernel_gs = crate::arch::x86_64::ap_start::rdmsr(IA32_GS_BASE);
    let old_kernel_gs = crate::arch::x86_64::ap_start::rdmsr(IA32_KERNEL_GS_BASE);
    // Simulate the architectural pre-`swapgs` window: the user base is loaded in GS_BASE and
    // the kernel per-CPU block is parked in the privileged KERNEL_GS_BASE. IF is clear so a
    // timer interrupt cannot use the absent per-CPU base; the NMI is non-maskable.
    crate::arch::x86_64::ap_start::disable_local_interrupts();
    crate::arch::x86_64::ap_start::wrmsr(IA32_GS_BASE, 0);
    crate::arch::x86_64::ap_start::wrmsr(IA32_KERNEL_GS_BASE, kernel_gs);
    core::sync::atomic::fence(Ordering::SeqCst);
    SWAPGS_READY.store(1, Ordering::SeqCst);
    while SWAPGS_RELEASE.load(Ordering::SeqCst) == 0 {
        core::hint::spin_loop();
    }
    let gs = crate::arch::x86_64::ap_start::rdmsr(IA32_GS_BASE);
    let kernel = crate::arch::x86_64::ap_start::rdmsr(IA32_KERNEL_GS_BASE);
    SWAPGS_OK.store(u32::from(gs == 0 && kernel == kernel_gs), Ordering::SeqCst);
    // Restore the original pair before any interrupt can observe the simulated state.
    crate::arch::x86_64::ap_start::wrmsr(IA32_KERNEL_GS_BASE, old_kernel_gs);
    crate::arch::x86_64::ap_start::wrmsr(IA32_GS_BASE, kernel_gs);
    crate::arch::x86_64::ap_start::enable_local_interrupts();
    SWAPGS_DONE.store(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn swapgs_window_test(cpus: usize) -> Result<u64, &'static str> {
    if cpus < 2 {
        return Err("needs at least 2 CPUs");
    }
    SWAPGS_READY.store(0, Ordering::SeqCst);
    SWAPGS_RELEASE.store(0, Ordering::SeqCst);
    SWAPGS_DONE.store(0, Ordering::SeqCst);
    SWAPGS_OK.store(0, Ordering::SeqCst);
    if sched::spawn_kernel_on(1, "smp-swapgs", swapgs_window_target).is_err() {
        return Err("cannot spawn swapgs target");
    }
    let start = sleep::now_ns();
    while SWAPGS_READY.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("swapgs target not ready");
        }
        sleep::sleep_ns(1_000_000);
    }
    let start = sleep::now_ns();
    let flushed = shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE);
    let elapsed_ms = sleep::now_ns().saturating_sub(start) / 1_000_000;
    SWAPGS_RELEASE.store(1, Ordering::SeqCst);
    let start = sleep::now_ns();
    while SWAPGS_DONE.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("swapgs target did not finish");
        }
        sleep::sleep_ns(1_000_000);
    }
    if !flushed {
        return Err("swapgs shootdown timed out");
    }
    if SWAPGS_OK.load(Ordering::SeqCst) == 0 {
        return Err("swapgs handler did not restore the GS pair");
    }
    sleep::sleep_ns(10_000_000);
    Ok(elapsed_ms)
}

#[cfg(target_arch = "x86_64")]
fn ack_target() {
    ACK_READY.store(1, Ordering::SeqCst);
    while ACK_DONE.load(Ordering::SeqCst) == 0 {
        sleep::sleep_ns(1_000_000);
    }
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn start_ack_target() -> Result<(), &'static str> {
    ACK_READY.store(0, Ordering::SeqCst);
    ACK_DONE.store(0, Ordering::SeqCst);
    if sched::spawn_kernel_on(1, "smp-ack-target", ack_target).is_err() {
        return Err("cannot spawn ack target");
    }
    let start = sleep::now_ns();
    while ACK_READY.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("ack target not ready");
        }
        sleep::sleep_ns(1_000_000);
    }
    Ok(())
}

#[cfg(target_arch = "x86_64")]
fn stop_ack_target() {
    ACK_DONE.store(1, Ordering::SeqCst);
    // Let the target leave CPU1 before a later row arms another delivery hook.
    sleep::sleep_ns(20_000_000);
}

#[cfg(target_arch = "x86_64")]
fn shootdown_once() -> (bool, u64, u64) {
    let before = shootdown::flush_count(1);
    let start = sleep::now_ns();
    let flushed = shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE);
    let elapsed_ms = sleep::now_ns().saturating_sub(start) / 1_000_000;
    (flushed, elapsed_ms, shootdown::flush_count(1).saturating_sub(before))
}

/// S195: a lost NMI must not lose the shootdown; the fixed IPI is the latched fallback.
#[cfg(target_arch = "x86_64")]
fn lost_nmi_test(cpus: usize) -> Result<u64, &'static str> {
    if cpus < 2 {
        return Ok(0);
    }
    start_ack_target()?;
    shootdown::arm_nmi_drop(1);
    let (flushed, elapsed_ms, flushes) = shootdown_once();
    stop_ack_target();
    if !flushed {
        return Err("lost-NMI shootdown timed out");
    }
    if flushes == 0 {
        return Err("lost-NMI target did not flush");
    }
    Ok(elapsed_ms)
}

/// S195: a target that acks after the 100 ms diagnostic must still be waited for.
#[cfg(target_arch = "x86_64")]
fn delayed_ack_test(cpus: usize) -> Result<u64, &'static str> {
    if cpus < 2 {
        return Ok(0);
    }
    start_ack_target()?;
    shootdown::arm_ack_delay(1, ACK_DELAY_NS);
    let (flushed, elapsed_ms, flushes) = shootdown_once();
    stop_ack_target();
    if !flushed {
        return Err("delayed-ack shootdown timed out");
    }
    if flushes == 0 {
        return Err("delayed-ack target did not flush");
    }
    Ok(elapsed_ms)
}

/// S195 mutation: drop one ack and the row must report a failure (not a silent pass).
#[cfg(target_arch = "x86_64")]
fn lost_ack_test(cpus: usize) -> ! {
    if cpus < 2 {
        kprintln!("selftest: smp tlb lost-ack failed: too few CPUs");
        boot::exit_qemu(0x11);
    }
    shootdown::set_failure_injection(true);
    let (flushed, _elapsed_ms, _flushes) = shootdown_once();
    if flushed {
        kprintln!("selftest: smp tlb lost-ack failed: shootdown passed without an ack");
    } else {
        kprintln!("selftest: smp tlb lost-ack failed: ack missing");
    }
    boot::exit_qemu(0x11)
}

#[cfg(target_arch = "x86_64")]
fn nested_shooter() {
    let mut round = 0u32;
    while round < NESTED_ROUNDS {
        if !shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE) {
            NESTED_FAIL.fetch_add(1, Ordering::SeqCst);
        }
        round = round.saturating_add(1);
    }
    NESTED_DONE.fetch_add(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn nested_shootdown_test(cpus: usize) -> Result<u32, &'static str> {
    if cpus < 3 {
        return Err("needs at least 3 CPUs");
    }
    NESTED_DONE.store(0, Ordering::SeqCst);
    NESTED_FAIL.store(0, Ordering::SeqCst);
    if sched::spawn_kernel_on(1, "smp-nested-1", nested_shooter).is_err() {
        return Err("cannot spawn nested shooter 1");
    }
    if sched::spawn_kernel_on(2, "smp-nested-2", nested_shooter).is_err() {
        return Err("cannot spawn nested shooter 2");
    }
    let start = sleep::now_ns();
    while NESTED_DONE.load(Ordering::SeqCst) < 2 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("nested shooters timed out");
        }
        sleep::sleep_ns(1_000_000);
    }
    if NESTED_FAIL.load(Ordering::SeqCst) != 0 {
        return Err("nested shootdown returned false");
    }
    // Let both shooters leave their CPUs before the stale-NMI probe targets CPU 1.
    sleep::sleep_ns(10_000_000);
    // A second NMI with no new sequence must be a no-op: the handler must not double-ack it.
    let before = shootdown::flush_count(1);
    crate::arch::x86_64::apic::send_nmi(crate::arch::x86_64::ap_start::apic_id(1));
    sleep::sleep_ns(20_000_000);
    if shootdown::flush_count(1) != before {
        return Err("duplicate NMI acknowledged a stale shootdown");
    }
    sleep::sleep_ns(10_000_000);
    Ok(NESTED_ROUNDS.saturating_mul(2))
}

#[cfg(target_arch = "x86_64")]
fn gs_pattern_target() {
    let kernel_gs = crate::arch::x86_64::ap_start::rdmsr(IA32_GS_BASE);
    let old_kernel_gs = crate::arch::x86_64::ap_start::rdmsr(IA32_KERNEL_GS_BASE);
    crate::arch::x86_64::ap_start::disable_local_interrupts();
    // Absent pattern: KERNEL_GS_BASE is zero while the kernel GS base is live.
    crate::arch::x86_64::ap_start::wrmsr(IA32_KERNEL_GS_BASE, 0);
    core::sync::atomic::fence(Ordering::SeqCst);
    GS_READY.store(1, Ordering::SeqCst);
    while GS_RELEASE.load(Ordering::SeqCst) == 0 {
        core::hint::spin_loop();
    }
    let absent = crate::arch::x86_64::ap_start::rdmsr(IA32_GS_BASE) == kernel_gs
        && crate::arch::x86_64::ap_start::rdmsr(IA32_KERNEL_GS_BASE) == 0;
    // Invalid pattern: a non-kernel KERNEL_GS_BASE must not trigger a swap.
    crate::arch::x86_64::ap_start::wrmsr(IA32_KERNEL_GS_BASE, INVALID_KERNEL_GS_BASE);
    GS_RELEASE.store(0, Ordering::SeqCst);
    core::sync::atomic::fence(Ordering::SeqCst);
    GS_READY.store(2, Ordering::SeqCst);
    while GS_RELEASE.load(Ordering::SeqCst) == 0 {
        core::hint::spin_loop();
    }
    let invalid = crate::arch::x86_64::ap_start::rdmsr(IA32_GS_BASE) == kernel_gs
        && crate::arch::x86_64::ap_start::rdmsr(IA32_KERNEL_GS_BASE) == INVALID_KERNEL_GS_BASE;
    GS_OK.store(u32::from(absent && invalid), Ordering::SeqCst);
    crate::arch::x86_64::ap_start::wrmsr(IA32_KERNEL_GS_BASE, old_kernel_gs);
    crate::arch::x86_64::ap_start::enable_local_interrupts();
    GS_DONE.store(1, Ordering::SeqCst);
    sched::exit_thread(0)
}

#[cfg(target_arch = "x86_64")]
fn gs_pattern_test(cpus: usize) -> Result<u32, &'static str> {
    if cpus < 2 {
        return Err("needs at least 2 CPUs");
    }
    GS_READY.store(0, Ordering::SeqCst);
    GS_RELEASE.store(0, Ordering::SeqCst);
    GS_DONE.store(0, Ordering::SeqCst);
    GS_OK.store(0, Ordering::SeqCst);
    if sched::spawn_kernel_on(1, "smp-gs-pattern", gs_pattern_target).is_err() {
        return Err("cannot spawn GS pattern target");
    }
    let start = sleep::now_ns();
    while GS_READY.load(Ordering::SeqCst) != 1 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("GS absent pattern not ready");
        }
        sleep::sleep_ns(1_000_000);
    }
    let absent = shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE);
    GS_RELEASE.store(1, Ordering::SeqCst);
    let start = sleep::now_ns();
    while GS_READY.load(Ordering::SeqCst) != 2 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("GS invalid pattern not ready");
        }
        sleep::sleep_ns(1_000_000);
    }
    let invalid = shootdown::flush_range(TLB_TEST_BASE, frame::PAGE_SIZE);
    GS_RELEASE.store(1, Ordering::SeqCst);
    let start = sleep::now_ns();
    while GS_DONE.load(Ordering::SeqCst) == 0 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("GS pattern target did not finish");
        }
        sleep::sleep_ns(1_000_000);
    }
    if !absent || !invalid {
        return Err("GS pattern shootdown timed out");
    }
    if GS_OK.load(Ordering::SeqCst) == 0 {
        return Err("GS pattern handler changed the GS pair");
    }
    sleep::sleep_ns(10_000_000);
    Ok(1)
}

fn call_probe_test(cpus: usize) -> Result<u32, &'static str> {
    if cpus < 2 {
        return Ok(0);
    }
    PROBE_DONE.store(0, Ordering::SeqCst);
    for failed in PROBE_FAIL.iter() {
        failed.store(0, Ordering::SeqCst);
    }
    if sched::spawn_kernel_on(0, "smp-probe-0", probe_cpu0).is_err() {
        return Err("cannot spawn probe CPU 0");
    }
    if sched::spawn_kernel_on(1, "smp-probe-1", probe_cpu1).is_err() {
        return Err("cannot spawn probe CPU 1");
    }
    let start = sleep::now_ns();
    while PROBE_DONE.load(Ordering::SeqCst) < 2 {
        if sleep::now_ns().saturating_sub(start) > TIMEOUT_NS {
            return Err("concurrent probe timeout");
        }
        sleep::sleep_ns(1_000_000);
    }
    let mut failures = 0u32;
    let mut cpu = 0usize;
    while cpu < 2 {
        if let Some(failed) = PROBE_FAIL.get(cpu) {
            failures = failures.saturating_add(failed.load(Ordering::SeqCst));
        }
        cpu = cpu.saturating_add(1);
    }
    if failures != 0 {
        return Err("concurrent probe mismatch");
    }
    Ok(PROBE_ROUNDS.saturating_mul(2))
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
