// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Application-processor bring-up from the MADT.
//!
//! The boot processor starts the APs one at a time: it writes that AP's stack, page table, GS
//! block and entry point into the shared low trampoline page, sends INIT-SIPI-SIPI and waits for
//! the AP to publish `online` before reusing the trampoline page for the next CPU.

use core::sync::atomic::{AtomicBool, Ordering};

use alloc::sync::Arc;

use crate::arch::x86_64::{acpi, ap_start, apic, ipi};
use crate::kprintln;
use crate::sched;
use crate::time;

use super::{MAX_CPUS, set_online};

static STARTED: AtomicBool = AtomicBool::new(false);

/// Bring up every enabled MADT CPU except the boot processor.
pub fn platform_init() {
    if STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    if ap_start::early_init().is_err() {
        kprintln!("smp: per-CPU setup failed; running on the boot processor");
        set_online(1);
        kprintln!("smp: 1 cpus online");
        return;
    }
    if !ipi::install() {
        kprintln!("smp: cannot install IPI gates; running on the boot processor");
        set_online(1);
        kprintln!("smp: 1 cpus online");
        return;
    }
    let info = acpi::info();
    let target = (info.cpus as usize).min(MAX_CPUS);
    let boot_id = apic::id();
    let root = match ap_start::ap_root() {
        Ok(root) => root,
        Err(()) => {
            kprintln!("smp: no AP page table; running on the boot processor");
            set_online(1);
            kprintln!("smp: 1 cpus online");
            return;
        }
    };

    let mut cpu = 1usize;
    let mut index = 0usize;
    while index < target && cpu < target {
        let apic_id = info.apic_ids.get(index).copied().unwrap_or(0);
        index = index.saturating_add(1);
        if apic_id == boot_id {
            continue;
        }
        let idle = match sched::new_ap_idle() {
            Ok(idle) => idle,
            Err(error) => {
                kprintln!("smp: cannot allocate idle stack: {error:?}");
                break;
            }
        };
        if sched::init_cpu(cpu, Arc::clone(&idle)).is_err() {
            kprintln!("smp: invalid CPU slot {cpu}");
            break;
        }
        if ap_start::prepare_ap(
            cpu,
            apic_id,
            idle.kernel_stack_top,
            ap_start::ap_entry as *const () as u64,
            root,
        )
        .is_err()
        {
            kprintln!("smp: cannot prepare CPU {cpu}");
            break;
        }
        let mut started = false;
        let mut attempt = 0u32;
        while attempt < 3 && !started {
            ap_start::start_ap(apic_id);
            let deadline = time::now_ns().saturating_add(200_000_000);
            while ap_start::block(cpu).online.load(Ordering::Acquire) == 0 {
                if time::now_ns() > deadline {
                    break;
                }
                core::hint::spin_loop();
            }
            started = ap_start::block(cpu).online.load(Ordering::Acquire) != 0;
            attempt = attempt.saturating_add(1);
        }
        if !started {
            kprintln!("smp: cpu {apic_id} did not start");
            // The APs that did start are spinning with interrupts off; let them enter idle.
            ap_start::start_ap_timers();
            set_online(cpu);
            kprintln!("smp: {cpu} cpus online");
            return;
        }
        cpu = cpu.saturating_add(1);
    }

    // All APs are spinning with interrupts off until this point, so the timer gates are
    // quiescent while the boot processor finishes initialisation.
    ap_start::start_ap_timers();
    set_online(cpu);
    kprintln!("smp: {cpu} cpus online");
}

/// Ask `cpu` to reschedule (used by cross-CPU wakeups).
pub fn send_reschedule(cpu: usize) {
    ipi::send_reschedule(cpu);
}
