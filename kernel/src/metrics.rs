// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

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
    match sched::spawn_kernel("k449-metrics", sampler) {
        Ok(_) => kprintln!("K449-METRICS: sampler every {seconds}s"),
        Err(error) => kprintln!("K449-METRICS: spawn failed: {error:?}"),
    }
}

/// Sample and print one line per interval.
fn sampler() {
    loop {
        sleep_ns(INTERVAL_NS.load(Ordering::Relaxed));
        let (heap_used, heap_free) = memory::heap::stats();
        let frames = memory::frame::stats();
        let (procs, threads) = process::live_totals();
        let class = memory::heap::class_stats(16);
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
                memory::paging::page_table_frames(),
                sched::thread::stack_frames(),
                process_pages("init"),
                process_pages("vfsd"),
                process_pages("netd"),
                process_pages("sshd"),
                class.map_or(0, |stats| stats.free as u64),
                class.map_or(0, |stats| stats.in_use as u64),
                class.map_or(0, |stats| stats.slabs as u64),
            )
        );
        if crate::memory::paging::cmdline()
            .split_ascii_whitespace()
            .any(|token| token == "k449-pending")
        {
            for process in process::processes() {
                let queued = process.scheme_queue.lock().len();
                let active = process.active_requests.lock().len();
                let client = process.client_requests.lock().len();
                if queued == 0 && active == 0 && client == 0 {
                    continue;
                }
                let schemes = process
                    .schemes
                    .lock()
                    .iter()
                    .map(|server| server.name.clone())
                    .collect::<alloc::vec::Vec<_>>()
                    .join(",");
                let state = match *process.state.lock() {
                    process::ProcessState::Created => 0u64,
                    process::ProcessState::Running => 1,
                    process::ProcessState::Exited => 2,
                };
                let last_syscall = process.last_syscall.load(Ordering::Relaxed);
                let pc = process.last_pc.load(Ordering::Relaxed);
                let inside = u64::from(process.in_syscall.load(Ordering::Relaxed));
                let wait = process.wait_object.load(Ordering::Relaxed);
                let threads = u64::from(process.threads.load(Ordering::Relaxed));
                kprintln!(
                    "K449-PENDING t={} pid={} queued={} active={} client={} schemes={} state={} last_syscall={} pc=0x{:x} in_syscall={} wait={} threads={}",
                    crate::time::now_ns(),
                    process.id,
                    queued,
                    active,
                    client,
                    schemes,
                    state,
                    last_syscall,
                    pc,
                    inside,
                    wait,
                    threads
                );
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
fn serves_scheme(target: &process::Process, name: &str) -> bool {
    target
        .schemes
        .lock()
        .iter()
        .any(|scheme| scheme.name == name)
}

/// True when `target` registered any scheme.
fn serves_any(target: &process::Process) -> bool {
    !target.schemes.lock().is_empty()
}

/// True when any of `target`'s scheme-file handles points at a server of `name`.
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
) -> String {
    format!(
        "K449-METRICS heap_used={heap_used} heap_free={heap_free} \
         frames_free={frames_free} frames_alloc={frames_allocated} \
         procs={procs} threads={threads} ofds={ofds} \
         pt_frames={pt_frames} stack_frames={stack_frames} init_pages={init_pages} \
         vfsd_pages={vfsd_pages} netd_pages={netd_pages} sshd_pages={sshd_pages} \
         c3072_free={c3072_free} c3072_used={c3072_used} c3072_slabs={c3072_slabs}"
    )
}

#[cfg(test)]
#[path = "metrics_ut.rs"]
mod tests;
