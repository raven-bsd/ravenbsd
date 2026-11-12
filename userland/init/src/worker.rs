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

use raven_proto_svc::State;
use raven_rt::spawn::RtSys;
use raven_rt::sync::Mutex;
use raven_svc_manifest::{Manifest, Readiness};

use crate::boot::{self, Core};

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
        let handle = match boot::spawn_service(&manifest) {
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
        if manifest.readiness == Readiness::Scheme
            && !wait_for_scheme(&core, index, generation, &manifest)
        {
            let _ = raven_rt::handle_close(handle);
            return;
        }

        let mut code = 0i32;
        let mut sys = RtSys;
        if sys.proc_wait(handle, &mut code).is_err() {
            // STOP closed the handle before the wait started, or the child vanished.
            return;
        }
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
) -> bool {
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
        return false;
    };
    let path = alloc::format!("{scheme}:");
    let client = raven_scheme::Client::new();
    loop {
        if let Ok(handle) = client.open(&path, 1) {
            let _ = client.close(handle);
            let notices = {
                let mut guard = core.lock();
                guard.supervisor.ready(index, generation)
            };
            let ready = !notices.is_empty();
            boot::emit(core, &notices);
            return ready;
        }
        let now = boot::now_ms();
        let timed_out = {
            let mut guard = core.lock();
            guard.supervisor.readiness_timed_out(index, generation, now)
        };
        if !timed_out.is_empty() {
            boot::emit(core, &timed_out);
            return false;
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
            return false;
        }
        let _ = raven_rt::nanosleep(10_000_000);
    }
}
