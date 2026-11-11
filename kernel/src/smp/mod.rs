// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Symmetric multiprocessing: per-CPU state, AP bring-up and cross-CPU services.
//!
//! The module is declared from `crate::sched` (the kernel binary has no `lib.rs`); it is a
//! top-level concept otherwise. CPU 0 is the boot processor. CPUs 1..N are application
//! processors started from the MADT through the real-mode trampoline in
//! `arch::x86_64::ap_start`.
//!
//! # Locking
//!
//! Per-CPU scheduler state is protected by one `spin::Mutex` per CPU (see `sched`). Cross-CPU
//! senders take exactly one remote CPU lock at a time, so there is no lock-order cycle. The TLB
//! shootdown mailbox and the cross-CPU probe mailbox are serialised by their own interrupt-safe
//! locks.

pub mod bringup;
pub mod call;
pub mod percpu;
pub mod selftest;
pub mod shootdown;

use core::sync::atomic::{AtomicUsize, Ordering};

/// Maximum number of CPUs kept in the per-CPU tables (matches the MADT parser).
pub const MAX_CPUS: usize = 64;

static ONLINE: AtomicUsize = AtomicUsize::new(1);

/// Number of CPUs currently online, including the boot processor.
pub fn online_cpus() -> usize {
    ONLINE.load(Ordering::Acquire).clamp(1, MAX_CPUS)
}

/// Publish the number of CPUs that completed bring-up.
pub(crate) fn set_online(count: usize) {
    ONLINE.store(count.clamp(1, MAX_CPUS), Ordering::Release);
}

/// Bring up the application processors and install the IPI gates.
///
/// Called once from the architecture interrupt-enable hook, after the BSP local APIC and timer
/// are initialised and before `sti`. A failure leaves the system running on the boot processor.
pub fn platform_init() {
    bringup::platform_init();
}

/// Run the optional `selftest=smp` workload after the BSP has enabled interrupts.
pub fn after_enable_interrupts() {
    let cmdline = crate::cmdline::CmdLine::parse(crate::memory::paging::cmdline());
    shootdown::set_failure_injection(cmdline.flag("s34-tlb-fail"));
    if cmdline.get("selftest") == Some("smp") {
        selftest::run();
    }
}

/// Called by the scheduler on an AP once its architecture state is ready.
pub(crate) fn ap_main(cpu: usize) -> ! {
    crate::sched::ap_main(cpu)
}

/// Return from an IPI that asked this CPU to reschedule.
pub(crate) fn reschedule_ipi(from_user: bool) {
    crate::sched::ipi_return(from_user);
}
