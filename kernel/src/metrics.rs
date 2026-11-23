// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K449 test-only resource metrics.
//!
//! Enabled by `k449-metrics=<seconds>` on the kernel command line. A kernel thread prints one
//! `K449-METRICS ...` line to the serial every interval with the existing allocator, frame,
//! process, thread and handle counters. This is a diagnostic path for the SSH soak harness, not a
//! product service: with the command-line key absent nothing is spawned and no line is printed.

use core::sync::atomic::{AtomicU64, Ordering};

use alloc::format;
use alloc::string::String;

use crate::cmdline::CmdLine;
use crate::sched::sleep::sleep_ns;
use crate::{kprintln, memory, process, sched};

/// Default print interval when `k449-metrics` has no numeric value.
const DEFAULT_SECONDS: u64 = 5;
/// Nanoseconds per second.
const NS_PER_SECOND: u64 = 1_000_000_000;

/// Sampling interval in nanoseconds, published before the sampler thread starts.
static INTERVAL_NS: AtomicU64 = AtomicU64::new(DEFAULT_SECONDS * NS_PER_SECOND);

/// Start the test-only metric sampler when the command line asks for it.
pub fn start(cmdline: &CmdLine) {
    let Some(value) = cmdline.get("k449-metrics") else {
        return;
    };
    let seconds = value.parse::<u64>().unwrap_or(DEFAULT_SECONDS).max(1);
    INTERVAL_NS.store(seconds.saturating_mul(NS_PER_SECOND), Ordering::Relaxed);
    match sched::spawn_kernel("k449-metrics", sampler) {
        Ok(_) => kprintln!("K449-METRICS: sampler every {seconds}s"),
        Err(error) => kprintln!("K449-METRICS: spawn failed: {error:?}"),
    }
}

/// Print one-time object sizes for the 3072-class attribution.
fn print_sizes() {
    kprintln!(
        "K449-SIZES thread={} process={} uspace={} pending={} buffers={}",
        core::mem::size_of::<crate::sched::thread::Thread>(),
        core::mem::size_of::<crate::process::Process>(),
        core::mem::size_of::<crate::memory::uspace::UserAddressSpace>(),
        core::mem::size_of::<crate::ipc::PendingRequest>(),
        core::mem::size_of::<crate::ipc::RequestBuffers>(),
    );
}

/// Sample and print one line per interval.
fn sampler() {
    print_sizes();
    loop {
        sleep_ns(INTERVAL_NS.load(Ordering::Relaxed));
        let (heap_used, heap_free) = memory::heap::stats();
        let frames = memory::frame::stats();
        let (procs, threads) = process::live_totals();
        kprintln!(
            "{}",
            format_line(
                heap_used,
                heap_free,
                frames.free_bytes,
                frames.allocated_bytes,
                procs,
                threads,
                raven_handles::live_lineage_nodes(),
            )
        );
        let (graph_threads, weak_threads) = crate::sync::turnstile_counts();
        let (class_in_use, class_slabs, class_fail) =
            memory::heap::class_stats(3072).unwrap_or((0, 0, 0));
        let (class1024, _, _) = memory::heap::class_stats(1024).unwrap_or((0, 0, 0));
        let (class1536, _, _) = memory::heap::class_stats(1536).unwrap_or((0, 0, 0));
        let (class192, _, _) = memory::heap::class_stats(192).unwrap_or((0, 0, 0));
        let (queued, active, client) = crate::ipc::pending_counts();
        kprintln!(
            "K449-GRAPH graph_threads={graph_threads} weak_threads={weak_threads} \
             class3072_in_use={class_in_use} class3072_slabs={class_slabs} \
             class3072_fail={class_fail} class1024_in_use={class1024} \
             class1536_in_use={class1536} class192_in_use={class192} \
             queued={queued} active={active} client={client} \
             zombies={} deadlive={} clsenotices={}",
            process::zombie_count(),
            process::dead_live_threads(),
            crate::ipc::router::close_notice_count(),
        );
        let mut shown = 0usize;
        for entry in process::processes() {
            let queued = entry.scheme_queue.lock().len();
            if queued == 0 {
                continue;
            }
            kprintln!(
                "K449-PENDING pid={} queued={queued} path={}",
                entry.id,
                entry.path
            );
            shown = shown.saturating_add(1);
            if shown >= 8 {
                break;
            }
        }
    }
}

/// Format one machine-readable metric line.
pub fn format_line(
    heap_used: usize,
    heap_free: usize,
    frames_free: u64,
    frames_allocated: u64,
    procs: usize,
    threads: u32,
    ofds: u64,
) -> String {
    format!(
        "K449-METRICS heap_used={heap_used} heap_free={heap_free} \
         frames_free={frames_free} frames_alloc={frames_allocated} \
         procs={procs} threads={threads} ofds={ofds}"
    )
}

#[cfg(test)]
#[path = "metrics_ut.rs"]
mod tests;
