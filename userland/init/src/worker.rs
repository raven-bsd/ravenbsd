// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One supervision worker per running service.
//!
//! A worker owns the whole lifecycle of one service: spawn, readiness polling, `PROC_WAIT`,
//! restart delay and the next spawn. The supervisor state machine decides every transition; the
//! worker only performs the syscalls and reports events with the generation it was given, so a
//! late wait or a cancelled restart cannot resurrect a stopped service.

use alloc::string::ToString;
use alloc::sync::Arc;
use core::sync::atomic::{AtomicI32, AtomicU32, AtomicU64, Ordering};

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_svc::State;
use raven_rt::spawn::RtSys;
use raven_rt::sync::Mutex;
use raven_svc_manifest::{Manifest, Readiness};

use crate::boot::{self, Core};

/// One scheme-readiness waiter per service index (the manifest parser caps services at 256).
const WAIT_SLOT_COUNT: usize = 256;
/// Scheme-readiness poll interval.
const READINESS_POLL_MS: u64 = 10;
/// A poll gap above this is a stall of the worker itself, not service slowness.
const READINESS_STALL_MS: u64 = 500;
/// Status published for a genuine wait error. A child exit is 0..=255, so this cannot be
/// confused with a real exit or a signal-derived 128+signal status.
pub const WAIT_ERROR_CODE: i32 = -1;

/// Wait for `handle` and return the child's real exit status.
///
/// `THREAD_KICK` may interrupt `PROC_WAIT` with `EINTR`; that is not an exit and must be
/// retried. A genuine wait error is returned so the caller can publish [`WAIT_ERROR_CODE`]
/// instead of inventing a child exit.
pub fn wait_child_status(handle: Handle) -> Result<i32, Errno> {
    let mut sys = RtSys;
    loop {
        let mut code = 0i32;
        match sys.proc_wait(handle, &mut code) {
            Ok(()) => return Ok(code),
            Err(Errno::EINTR) => continue,
            Err(error) => return Err(error),
        }
    }
}

struct WaitSlot {
    handle: AtomicU32,
    generation: AtomicU64,
    /// Futex word: 0 while the child runs, 1 after `PROC_WAIT` records the code.
    finished: AtomicU32,
    code: AtomicI32,
}

impl WaitSlot {
    const fn new() -> Self {
        Self {
            handle: AtomicU32::new(u32::MAX),
            generation: AtomicU64::new(0),
            finished: AtomicU32::new(1),
            code: AtomicI32::new(1),
        }
    }
}

static WAIT_SLOTS: [WaitSlot; WAIT_SLOT_COUNT] = [const { WaitSlot::new() }; WAIT_SLOT_COUNT];

/// Start the helper that observes `PROC_WAIT` while the worker polls scheme readiness.
///
/// A child that exits before it registers its scheme (for example netd without a link) must not
/// keep the start plan blocked until the 60 s readiness deadline. The helper records the exit;
/// the worker still owns the supervisor transition and the handle close.
fn start_waiter(index: usize, generation: u64, handle: Handle) -> bool {
    let Some(slot) = WAIT_SLOTS.get(index) else {
        return false;
    };
    slot.finished.store(0, Ordering::Release);
    slot.code.store(1, Ordering::Release);
    slot.handle.store(handle.0, Ordering::Release);
    slot.generation.store(generation, Ordering::Release);
    raven_rt::thread::spawn(waiter_thread, index, 32 * 1024).is_ok()
}

/// Helper thread entry point: block in `PROC_WAIT` and publish the exit code.
fn waiter_thread(index: usize) {
    let Some(slot) = WAIT_SLOTS.get(index) else {
        return;
    };
    let generation = slot.generation.load(Ordering::Acquire);
    if generation == 0 {
        return;
    }
    let handle = Handle(slot.handle.load(Ordering::Acquire));
    let code = wait_child_status(handle).unwrap_or(WAIT_ERROR_CODE);
    if slot.generation.load(Ordering::Acquire) == generation {
        slot.code.store(code, Ordering::Release);
        slot.finished.store(1, Ordering::Release);
        let _ = raven_rt::futex_wake(&slot.finished, u32::MAX);
    }
}

fn waiter_code(index: usize, generation: u64) -> Option<i32> {
    let slot = WAIT_SLOTS.get(index)?;
    if slot.generation.load(Ordering::Acquire) != generation
        || slot.finished.load(Ordering::Acquire) == 0
    {
        return None;
    }
    Some(slot.code.load(Ordering::Acquire))
}

fn wait_waiter(index: usize, generation: u64) -> i32 {
    let Some(slot) = WAIT_SLOTS.get(index) else {
        return WAIT_ERROR_CODE;
    };
    loop {
        if let Some(code) = waiter_code(index, generation) {
            return code;
        }
        let state = slot.finished.load(Ordering::Acquire);
        if state == 0 {
            let _ = raven_rt::futex_wait(&slot.finished, 0, None);
        }
    }
}

enum ReadinessOutcome {
    Ready,
    Exited(i32),
    Failed,
}

/// Worker thread entry point. The argument packs the index and generation.
pub fn thread(argument: usize) {
    run(argument);
}

fn run(argument: usize) {
    let index = argument & 0xFFFF_FFFF;
    let mut generation = (argument >> 32) as u64;
    let Some(core) = boot::core_handle() else {
        return;
    };
    loop {
        let Some(manifest) = ready_manifest(&core, index, generation) else {
            return;
        };
        let from_root = boot::service_root_exec(&core, index);
        let handle = match boot::spawn_service(&manifest, from_root) {
            Ok(handle) => handle,
            Err(reason) => {
                let notices = {
                    let mut guard = core.lock();
                    guard.supervisor.spawn_failed(index, generation, reason)
                };
                boot::emit(&core, &notices);
                return;
            }
        };
        let notices = {
            let mut guard = core.lock();
            guard
                .supervisor
                .spawned(index, generation, handle, boot::now_ms())
        };
        boot::emit(&core, &notices);
        if core.lock().supervisor.stop_pending(index, generation) {
            let _ = raven_rt::proc_terminate(handle, 0);
        }

        let waited = if manifest.readiness == Readiness::Scheme {
            let waiter = start_waiter(index, generation, handle);
            match wait_for_scheme(&core, index, generation, &manifest, waiter) {
                ReadinessOutcome::Ready => {
                    if waiter {
                        Some(wait_waiter(index, generation))
                    } else {
                        Some(wait_child_status(handle).unwrap_or(WAIT_ERROR_CODE))
                    }
                }
                ReadinessOutcome::Exited(code) => Some(code),
                ReadinessOutcome::Failed => {
                    if waiter {
                        let _ = raven_rt::proc_terminate(handle, 0);
                        let _ = wait_waiter(index, generation);
                    }
                    let _ = raven_rt::handle_close(handle);
                    return;
                }
            }
        } else if manifest.readiness == Readiness::File {
            let waiter = start_waiter(index, generation, handle);
            match wait_for_log(&core, index, generation, &manifest, waiter) {
                ReadinessOutcome::Ready => {
                    if waiter {
                        Some(wait_waiter(index, generation))
                    } else {
                        Some(wait_child_status(handle).unwrap_or(WAIT_ERROR_CODE))
                    }
                }
                ReadinessOutcome::Exited(code) => Some(code),
                ReadinessOutcome::Failed => {
                    if waiter {
                        let _ = raven_rt::proc_terminate(handle, 0);
                        let _ = wait_waiter(index, generation);
                    }
                    let _ = raven_rt::handle_close(handle);
                    return;
                }
            }
        } else {
            Some(wait_child_status(handle).unwrap_or(WAIT_ERROR_CODE))
        };

        let code = waited.unwrap_or(WAIT_ERROR_CODE);
        let _ = raven_rt::handle_close(handle);
        let now = boot::now_ms();
        let (outcome, exit_when_exits) = {
            let mut guard = core.lock();
            let outcome = guard.supervisor.exited(index, generation, code, now);
            let name = guard
                .supervisor
                .services()
                .get(index)
                .map(|service| service.name.as_str());
            let exit_when =
                name.is_some_and(|name| guard.config.exit_when_exits.as_deref() == Some(name));
            (outcome, exit_when)
        };
        boot::emit(&core, &outcome.notices);
        if exit_when_exits {
            core.lock().exit_code = Some(code);
        }
        if let Some(request) = outcome.start {
            boot::spawn_worker(&core, request.index, request.generation);
            return;
        }
        match outcome.restart {
            Some(restart) => {
                if restart.delay_ms > 0 {
                    let _ = raven_rt::nanosleep(u64::from(restart.delay_ms) * 1_000_000);
                }
                let now = boot::now_ms();
                let current = {
                    let mut guard = core.lock();
                    guard
                        .supervisor
                        .begin_restart(index, restart.generation, now)
                };
                if !current {
                    return;
                }
                generation = restart.generation;
            }
            None => return,
        }
    }
}

fn ready_manifest(core: &Arc<Mutex<Core>>, index: usize, generation: u64) -> Option<Manifest> {
    let guard = core.lock();
    let runtime = guard.supervisor.runtime().get(index)?;
    if runtime.generation != generation || runtime.state != State::Starting {
        return None;
    }
    guard.supervisor.services().get(index).cloned()
}

/// Every worker waits for its child. `svcd` exits process-wide, which terminates any helper still
/// blocked in `PROC_WAIT`; test profiles no longer need `restart = "never"` to avoid the wait.
fn wait_for_scheme(
    core: &Arc<Mutex<Core>>,
    index: usize,
    generation: u64,
    manifest: &Manifest,
    waiter: bool,
) -> ReadinessOutcome {
    let Some(scheme) = manifest
        .provides
        .iter()
        .find_map(|provide| provide.strip_prefix("scheme:"))
    else {
        let notices = {
            let mut guard = core.lock();
            guard.supervisor.spawn_failed(
                index,
                generation,
                "readiness is scheme but no scheme is provided".to_string(),
            )
        };
        boot::emit(core, &notices);
        return ReadinessOutcome::Failed;
    };
    let path = alloc::format!("{scheme}:");
    let client = raven_scheme::Client::new();
    // First poll timestamp for the diagnostic line; readiness rules are unchanged.
    let polled_at = boot::now_ms();
    let spawned_at = {
        let mut guard = core.lock();
        let spawned_at = guard
            .supervisor
            .runtime()
            .get(index)
            .and_then(|runtime| runtime.ready_timing.spawned_ms);
        guard.supervisor.polled(index, generation, polled_at);
        spawned_at
    };
    // Start the stall accounting at the spawn, so time spent setting up the waiter is credited
    // too instead of being charged silently before the first poll.
    let mut last_poll = spawned_at.unwrap_or(polled_at);
    loop {
        if waiter && let Some(code) = waiter_code(index, generation) {
            return ReadinessOutcome::Exited(code);
        }
        if let Ok(handle) = client.open(&path, 1) {
            let _ = client.close(handle);
            if waiter && let Some(code) = waiter_code(index, generation) {
                return ReadinessOutcome::Exited(code);
            }
            let notices = {
                let mut guard = core.lock();
                guard.supervisor.ready(index, generation, boot::now_ms())
            };
            boot::emit(core, &notices);
            return if notices.is_empty() {
                ReadinessOutcome::Failed
            } else {
                ReadinessOutcome::Ready
            };
        }
        let now = boot::now_ms();
        let gap = now.saturating_sub(last_poll);
        last_poll = now;
        // Only observed time counts: credit a gap in which this worker did not run, such as a
        // TCG guest descheduled by the host, back to the readiness window. A hung child leaves
        // the worker polling on time, so the deadline still fires.
        let stalled = gap.saturating_sub(READINESS_STALL_MS);
        let timed_out = {
            let mut guard = core.lock();
            if stalled > 0 {
                guard
                    .supervisor
                    .extend_readiness(index, generation, stalled);
            }
            guard.supervisor.readiness_timed_out(index, generation, now)
        };
        if !timed_out.is_empty() {
            boot::emit(core, &timed_out);
            return ReadinessOutcome::Failed;
        }
        let alive = {
            let guard = core.lock();
            guard
                .supervisor
                .runtime()
                .get(index)
                .is_some_and(|runtime| {
                    runtime.generation == generation && runtime.state == State::Starting
                })
        };
        if !alive {
            return ReadinessOutcome::Failed;
        }
        let _ = raven_rt::nanosleep(READINESS_POLL_MS * 1_000_000);
    }
}

/// Wait until `ready_file` contains `ready_match`, or the child exits/times out.
///
/// sshd logs `Server listening on ...` only after `listen()` has succeeded, so the line is a
/// real listener-ready signal. The file is read through the existing `file:` scheme; a missing or
/// partially written file is retried, never treated as a sleep.
fn wait_for_log(
    core: &Arc<Mutex<Core>>,
    index: usize,
    generation: u64,
    manifest: &Manifest,
    waiter: bool,
) -> ReadinessOutcome {
    let path = alloc::format!("file:{}", manifest.ready_file);
    let needle = manifest.ready_match.as_bytes();
    let client = raven_scheme::Client::new();
    let polled_at = boot::now_ms();
    let spawned_at = {
        let mut guard = core.lock();
        let spawned_at = guard
            .supervisor
            .runtime()
            .get(index)
            .and_then(|runtime| runtime.ready_timing.spawned_ms);
        guard.supervisor.polled(index, generation, polled_at);
        spawned_at
    };
    let mut last_poll = spawned_at.unwrap_or(polled_at);
    loop {
        if waiter && let Some(code) = waiter_code(index, generation) {
            return ReadinessOutcome::Exited(code);
        }
        if let Ok(handle) = client.open(&path, 1) {
            let mut data = alloc::vec::Vec::new();
            let mut offset = 0u64;
            loop {
                let mut chunk = [0u8; 1024];
                match client.read(handle, offset, &mut chunk) {
                    Ok(0) => break,
                    Ok(count) => {
                        data.extend_from_slice(&chunk[..count]);
                        offset = offset.saturating_add(u64::try_from(count).unwrap_or(0));
                        if data.windows(needle.len()).any(|window| window == needle) {
                            break;
                        }
                        if data.len() >= 64 * 1024 {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let found = data.windows(needle.len()).any(|window| window == needle);
            let _ = client.close(handle);
            if waiter && let Some(code) = waiter_code(index, generation) {
                return ReadinessOutcome::Exited(code);
            }
            if found {
                let notices = {
                    let mut guard = core.lock();
                    guard.supervisor.ready(index, generation, boot::now_ms())
                };
                boot::emit(core, &notices);
                return if notices.is_empty() {
                    ReadinessOutcome::Failed
                } else {
                    ReadinessOutcome::Ready
                };
            }
        }
        let now = boot::now_ms();
        let gap = now.saturating_sub(last_poll);
        last_poll = now;
        let stalled = gap.saturating_sub(READINESS_STALL_MS);
        let timed_out = {
            let mut guard = core.lock();
            if stalled > 0 {
                guard
                    .supervisor
                    .extend_readiness(index, generation, stalled);
            }
            guard.supervisor.readiness_timed_out(index, generation, now)
        };
        if !timed_out.is_empty() {
            boot::emit(core, &timed_out);
            return ReadinessOutcome::Failed;
        }
        let alive = {
            let guard = core.lock();
            guard
                .supervisor
                .runtime()
                .get(index)
                .is_some_and(|runtime| {
                    runtime.generation == generation && runtime.state == State::Starting
                })
        };
        if !alive {
            return ReadinessOutcome::Failed;
        }
        let _ = raven_rt::nanosleep(READINESS_POLL_MS * 1_000_000);
    }
}
