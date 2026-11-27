// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![cfg(target_arch = "x86_64")]

//! K449 test-only resource metrics.
//!
//! Enabled by `k449-metrics=<seconds>` on the kernel command line. A kernel thread prints one
//! `K449-METRICS ...` line to the serial every interval with the existing allocator, frame,
//! process, thread and handle counters, plus page-table frames, kernel-stack frames and the
//! present user pages of the init/vfsd/netd/sshd processes. This is a diagnostic path for the
//! SSH soak harness, not a product service: with the command-line key absent nothing is spawned
//! and no line is printed.

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
/// Print the leaked-thread holder census with each sample (`s36-holders`, test only).
static HOLDER_SCAN: AtomicBool = AtomicBool::new(false);

/// Start the test-only metric sampler when the command line asks for it.
pub fn start(cmdline: &CmdLine) {
    let Some(value) = cmdline.get("k449-metrics") else {
        return;
    };
    let seconds = value.parse::<u64>().unwrap_or(DEFAULT_SECONDS).max(1);
    INTERVAL_NS.store(seconds.saturating_mul(NS_PER_SECOND), Ordering::Relaxed);
    HOLDER_SCAN.store(cmdline.get("s36-holders").is_some(), Ordering::Relaxed);
    kprintln!(
        "K449-SIZES process={} thread={} pending_request={}",
        core::mem::size_of::<crate::process::Process>(),
        core::mem::size_of::<crate::sched::thread::Thread>(),
        core::mem::size_of::<crate::ipc::PendingRequest>(),
    );
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
        let class = memory::heap::class_stats(16);
        let turnstile = crate::sync::turnstile_counts();
        let zombies = process::zombie_count();
        let pending = crate::ipc::pending_counts();
        let leaked = process::leaked_process_stats();
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
                leaked[0],
                leaked[1],
                leaked[2],
                leaked[3],
                leaked[4],
                leaked[5],
                class.map_or(0, |stats| stats.free as u64),
                class.map_or(0, |stats| stats.in_use as u64),
                class.map_or(0, |stats| stats.slabs as u64),
                pending.0,
                pending.1,
                pending.2,
                usize::try_from(leaked[6]).unwrap_or(usize::MAX),
                leaked[7],
                leaked[8],
                leaked[12],
                turnstile.0,
                u32::try_from(turnstile.1).unwrap_or(u32::MAX),
                u64::try_from(zombies).unwrap_or(u64::MAX),
                crate::sync::registered_count(),
            )
        );
        let (graph_threads, weak_threads) = crate::sync::turnstile_counts();
        let class3072 = raven_kmem::class_for(3072).and_then(memory::heap::class_stats);
        let class1024 = raven_kmem::class_for(1024).and_then(memory::heap::class_stats);
        let class1536 = raven_kmem::class_for(1536).and_then(memory::heap::class_stats);
        let class192 = raven_kmem::class_for(192).and_then(memory::heap::class_stats);
        let (queued, active, client) = crate::ipc::pending_counts();
        kprintln!(
            "K449-GRAPH graph_threads={graph_threads} weak_threads={weak_threads} \
             class3072_in_use={} class3072_slabs={} class3072_fail={} \
             class1024_in_use={} class1536_in_use={} class192_in_use={} \
             queued={queued} active={active} client={client} \
             zombies={} deadlive={} clsenotices={}",
            class3072.map_or(0, |stats| stats.in_use),
            class3072.map_or(0, |stats| stats.slabs),
            class3072.map_or(0, |stats| stats.alloc_fail),
            class1024.map_or(0, |stats| stats.in_use),
            class1536.map_or(0, |stats| stats.in_use),
            class192.map_or(0, |stats| stats.in_use),
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
            let mut names = alloc::string::String::new();
            for scheme in entry.schemes.lock().iter() {
                if !names.is_empty() {
                    names.push(',');
                }
                names.push_str(&scheme.name);
            }
            let command = entry.command.lock().clone();
            kprintln!(
                "K449-PENDING pid={} queued={queued} path={} command={} schemes={}",
                entry.id,
                entry.path,
                command,
                names
            );
            shown = shown.saturating_add(1);
            if shown >= 8 {
                break;
            }
        }
        if HOLDER_SCAN.load(Ordering::Relaxed) {
            let holders = sched::thread::thread_holder_stats();
            kprintln!(
                "s36: hold thr={} cur={} rq={} sleep={} reap={} live={} pwait={} futex={} handle={} multi={} unexp={} reqwait={}",
                holders[0],
                holders[1],
                holders[2],
                holders[3],
                holders[4],
                holders[5],
                holders[6],
                holders[7],
                holders[8],
                holders[9],
                holders[10],
                holders[11]
            );
        }
    }
}

/// Present user pages of the named process, or zero when it is not running.
#[allow(dead_code)] // Kept for the next diagnostic image; the correlation sampler zeroes it.
fn process_pages(role: &str) -> u64 {
    for target in process::processes() {
        let matched = match role {
            "init" => target.id == 1,
            "vfsd" => serves_scheme(&target, "file"),
            "netd" => serves_scheme(&target, "net"),
            // sshd is the listener that holds a `net:` client file but serves no scheme itself.
            "sshd" => !serves_any(&target) && uses_server_scheme(&target, "net"),
            _ => false,
        };
        if !matched {
            continue;
        }
        let mut slot = target.address_space.lock();
        if let Some(space) = slot.as_mut() {
            return space.mapped_pages();
        }
    }
    0
}

/// True when `target` registered a scheme with this name.
#[allow(dead_code)] // Helper for `process_pages` in the next diagnostic image.
fn serves_scheme(target: &process::Process, name: &str) -> bool {
    target
        .schemes
        .lock()
        .iter()
        .any(|scheme| scheme.name == name)
}

/// True when `target` registered any scheme.
#[allow(dead_code)] // Helper for `process_pages` in the next diagnostic image.
fn serves_any(target: &process::Process) -> bool {
    !target.schemes.lock().is_empty()
}

/// True when any of `target`'s scheme-file handles points at a server of `name`.
#[allow(dead_code)] // Helper for `process_pages` in the next diagnostic image.
fn uses_server_scheme(target: &process::Process, name: &str) -> bool {
    let mut found = false;
    target.handles.lock().for_each_live(|_, object, _, _| {
        if let crate::process::handle::Object::SchemeFile(file) = object
            && file
                .server
                .schemes
                .lock()
                .iter()
                .any(|scheme| scheme.name == name)
        {
            found = true;
        }
    });
    found
}

/// Format one machine-readable metric line.
#[allow(clippy::too_many_arguments)] // one machine-readable row; the fields are the interface
pub fn format_line(
    heap_used: usize,
    heap_free: usize,
    frames_free: u64,
    frames_allocated: u64,
    procs: usize,
    threads: u32,
    ofds: u64,
    pt_frames: u64,
    stack_frames: u64,
    init_pages: u64,
    vfsd_pages: u64,
    netd_pages: u64,
    sshd_pages: u64,
    c3072_free: u64,
    c3072_used: u64,
    c3072_slabs: u64,
    live_threads: usize,
    reap: usize,
    slot_drops: usize,
    slot_denies: usize,
    stacks_fresh: u64,
    stacks_cached: u64,
    stacks_dropped: u64,
    sshd_procs: usize,
    sshd_threads: u32,
    dma_used: u64,
    donations: usize,
) -> String {
    format!(
        "K449-METRICS heap_used={heap_used} heap_free={heap_free} \
         frames_free={frames_free} frames_alloc={frames_allocated} \
         procs={procs} threads={threads} ofds={ofds} \
         pt_frames={pt_frames} stack_frames={stack_frames} init_pages={init_pages} \
         vfsd_pages={vfsd_pages} netd_pages={netd_pages} sshd_pages={sshd_pages} \
         c3072_free={c3072_free} c3072_used={c3072_used} c3072_slabs={c3072_slabs} \
         live_threads={live_threads} reap={reap} slot_drops={slot_drops} \
         slot_denies={slot_denies} stacks_fresh={stacks_fresh} \
         stacks_cached={stacks_cached} stacks_dropped={stacks_dropped} \
         sshd_procs={sshd_procs} sshd_threads={sshd_threads} dma_used={dma_used} \
         donations={donations}"
    )
}

#[cfg(test)]
#[path = "metrics_ut.rs"]
mod tests;
