// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure svcd supervision state machine.
//!
//! The state machine owns the manifest list, the start plan and the per-service lifecycle. It has
//! no syscalls, clock or threads: callers pass the current time in milliseconds and execute the
//! [`Notice`]s and [`StartRequest`]s it returns. The real `svcd` glue in `boot.rs` uses a futex
//! mutex and one worker thread per service; host tests drive the same transitions with an
//! explicit clock and a fake spawner.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_proto_svc::{State, Status};
use raven_svc_manifest::{
    GraphError, Manifest, Overrides, Plan, Readiness, Restart, apply_overrides, start_order,
};

/// Per-service runtime state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Runtime {
    /// Current lifecycle state.
    pub state: State,
    /// Live child process handle.
    pub handle: Option<Handle>,
    /// Restart attempts in the current window.
    pub restarts: u32,
    /// True once this service has been started at least once.
    pub started: bool,
    /// True once this service has been ready at least once in the current boot.
    pub ever_ready: bool,
    /// Monotonic deadline for readiness; `None` when not waiting.
    pub ready_deadline_ms: Option<u64>,
    /// Monotonic time of the pending restart.
    pub restart_at_ms: Option<u64>,
    /// Start of the restart-counting window.
    pub window_start_ms: u64,
    /// Generation counter; stale worker events are ignored.
    pub generation: u64,
    /// Last observed exit code.
    pub last_exit: Option<i32>,
    /// A STOP request is waiting for the child to reach terminal state.
    pub stop_requested: bool,
    /// Start the service again after the current stop reaches terminal state.
    pub restart_after_stop: bool,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            state: State::Stopped,
            handle: None,
            restarts: 0,
            started: false,
            ever_ready: false,
            ready_deadline_ms: None,
            restart_at_ms: None,
            window_start_ms: 0,
            generation: 0,
            last_exit: None,
            stop_requested: false,
            restart_after_stop: false,
        }
    }
}

/// A message the caller should print on the console.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// `svcd: <name> up`
    Up(usize),
    /// `svcd: <name> failed: <reason>`
    Failed(usize, String),
    /// `svcd: <name> crash loop, giving up`
    CrashLoop(usize),
}

/// A service the caller should start on a new worker thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StartRequest {
    /// Index into the supervisor's service list.
    pub index: usize,
    /// Generation the worker must present with later events.
    pub generation: u64,
}

/// The reason a restart is due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RestartDecision {
    /// Generation for the restarted service.
    pub generation: u64,
    /// Delay already elapsed (the caller may sleep again if it has not).
    pub delay_ms: u32,
}

/// The result of one exit event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExitOutcome {
    /// Set when the caller should start the service again after the delay.
    pub restart: Option<RestartDecision>,
    /// Set when an explicit restart should spawn the new generation immediately.
    pub start: Option<StartRequest>,
    /// Messages to print.
    pub notices: Vec<Notice>,
}

/// Why a root-manifest merge failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MergeError {
    /// A root `rc.conf` override names a service that does not exist.
    UnknownService(String),
    /// The merged dependency graph is invalid.
    Graph(GraphError),
}

/// The service supervisor.
#[derive(Debug)]
pub struct Supervisor {
    services: Vec<Manifest>,
    runtime: Vec<Runtime>,
    plan: Plan,
    boot_complete: bool,
}

impl Supervisor {
    /// Build a supervisor from parsed manifests.
    ///
    /// A cycle, a missing dependency or an ambiguous provider is returned unchanged so the caller
    /// can print it as the fatal `svcd: <reason>` line.
    pub fn new(services: Vec<Manifest>) -> Result<Self, GraphError> {
        let plan = start_order(&services)?;
        let runtime = services.iter().map(|_| Runtime::default()).collect();
        Ok(Self {
            services,
            runtime,
            plan,
            boot_complete: false,
        })
    }

    /// Merge root-filesystem manifests into the bootstrap set.
    ///
    /// A manifest with the same name replaces the initfs configuration in place, preserving that
    /// service's runtime and index (so a restart picks up the new file); a new name is appended.
    /// Optional root `rc.conf` overrides apply to the merged set. The plan is recomputed before
    /// any change is committed, so a bad merged graph leaves the supervisor untouched.
    pub fn merge(
        &mut self,
        root_services: Vec<Manifest>,
        overrides: Option<&Overrides>,
    ) -> Result<usize, MergeError> {
        let mut services = self.services.clone();
        let mut added = 0usize;
        for manifest in root_services {
            if let Some(index) = services
                .iter()
                .position(|service| service.name == manifest.name)
            {
                services[index] = manifest;
            } else {
                services.push(manifest);
                added = added.saturating_add(1);
            }
        }
        if let Some(overrides) = overrides {
            apply_overrides(&mut services, overrides)
                .map_err(|error| MergeError::UnknownService(error.service))?;
        }
        let plan = start_order(&services).map_err(MergeError::Graph)?;
        self.runtime.resize(services.len(), Runtime::default());
        self.services = services;
        self.plan = plan;
        Ok(added)
    }

    /// Parsed manifests in input order.
    pub fn services(&self) -> &[Manifest] {
        &self.services
    }

    /// Runtime state in manifest order.
    pub fn runtime(&self) -> &[Runtime] {
        &self.runtime
    }

    /// The deterministic start plan.
    pub const fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Number of manifests (enabled and disabled).
    pub fn len(&self) -> usize {
        self.services.len()
    }

    /// True when there are no manifests.
    pub fn is_empty(&self) -> bool {
        self.services.is_empty()
    }

    /// Find a service by name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.services
            .iter()
            .position(|service| service.name == name)
    }

    /// Mark the boot phase complete; required before [`Supervisor::settled`] can be true.
    pub fn set_boot_complete(&mut self) {
        self.boot_complete = true;
    }

    /// True after [`Supervisor::set_boot_complete`].
    pub const fn boot_complete(&self) -> bool {
        self.boot_complete
    }

    /// Start `index` and return its generation, or `None` for an invalid index.
    pub fn start(&mut self, index: usize, now: u64) -> Option<u64> {
        let service = self.services.get(index)?;
        let runtime = self.runtime.get_mut(index)?;
        let preserve_ready = runtime.state == State::Restarting;
        runtime.state = State::Starting;
        runtime.stop_requested = false;
        runtime.restart_after_stop = false;
        runtime.handle = None;
        runtime.started = true;
        runtime.ready_deadline_ms = match service.readiness {
            Readiness::None => None,
            _ => Some(now.saturating_add(u64::from(service.ready_timeout_ms))),
        };
        runtime.restart_at_ms = None;
        runtime.last_exit = None;
        runtime.generation = runtime.generation.wrapping_add(1);
        if !preserve_ready {
            runtime.ever_ready = false;
        }
        Some(runtime.generation)
    }

    /// Record that the worker created the child.
    pub fn spawned(
        &mut self,
        index: usize,
        generation: u64,
        handle: Handle,
        _now: u64,
    ) -> Vec<Notice> {
        let Some(runtime) = self.runtime.get_mut(index) else {
            return Vec::new();
        };
        if runtime.generation != generation || runtime.state != State::Starting {
            return Vec::new();
        }
        runtime.handle = Some(handle);
        if self.services[index].readiness == Readiness::None {
            runtime.state = State::Running;
            runtime.ever_ready = true;
            return alloc::vec![Notice::Up(index)];
        }
        Vec::new()
    }

    /// Record that readiness succeeded.
    pub fn ready(&mut self, index: usize, generation: u64) -> Vec<Notice> {
        let Some(runtime) = self.runtime.get_mut(index) else {
            return Vec::new();
        };
        if runtime.generation != generation || runtime.state != State::Starting {
            return Vec::new();
        }
        runtime.state = State::Running;
        runtime.ever_ready = true;
        runtime.ready_deadline_ms = None;
        alloc::vec![Notice::Up(index)]
    }

    /// Report that the service could not be created at all.
    pub fn spawn_failed(&mut self, index: usize, generation: u64, reason: String) -> Vec<Notice> {
        let Some(runtime) = self.runtime.get_mut(index) else {
            return Vec::new();
        };
        if runtime.generation != generation || runtime.state != State::Starting {
            return Vec::new();
        }
        runtime.state = State::Failed;
        runtime.ready_deadline_ms = None;
        alloc::vec![Notice::Failed(index, reason)]
    }

    /// Check the readiness deadline. Returns a failure notice and cancels the wait when due.
    pub fn readiness_timed_out(&mut self, index: usize, generation: u64, now: u64) -> Vec<Notice> {
        let Some(runtime) = self.runtime.get_mut(index) else {
            return Vec::new();
        };
        if runtime.generation != generation || runtime.state != State::Starting {
            return Vec::new();
        }
        let Some(deadline) = runtime.ready_deadline_ms else {
            return Vec::new();
        };
        if now < deadline {
            return Vec::new();
        }
        runtime.state = State::Failed;
        runtime.ready_deadline_ms = None;
        alloc::vec![Notice::Failed(index, "readiness timeout".to_string())]
    }

    /// Record a child exit and apply the restart policy.
    pub fn exited(&mut self, index: usize, generation: u64, code: i32, now: u64) -> ExitOutcome {
        let Some(service) = self.services.get(index) else {
            return ExitOutcome::default();
        };
        let Some(runtime) = self.runtime.get_mut(index) else {
            return ExitOutcome::default();
        };
        if runtime.generation != generation
            || !matches!(runtime.state, State::Starting | State::Running)
        {
            return ExitOutcome::default();
        }
        if runtime.stop_requested {
            runtime.stop_requested = false;
            runtime.handle = None;
            runtime.last_exit = Some(code);
            runtime.state = State::Stopped;
            runtime.ready_deadline_ms = None;
            runtime.restart_at_ms = None;
            let restart_after = runtime.restart_after_stop;
            runtime.restart_after_stop = false;
            if restart_after {
                let start = self
                    .start(index, now)
                    .map(|generation| StartRequest { index, generation });
                return ExitOutcome {
                    restart: None,
                    start,
                    notices: Vec::new(),
                };
            }
            return ExitOutcome::default();
        }
        runtime.handle = None;
        runtime.last_exit = Some(code);
        if runtime.state == State::Starting && service.readiness == Readiness::Exit && code == 0 {
            runtime.state = State::Stopped;
            runtime.ever_ready = true;
            runtime.ready_deadline_ms = None;
            return ExitOutcome {
                restart: None,
                start: None,
                notices: alloc::vec![Notice::Up(index)],
            };
        }

        let restart = match service.restart {
            Restart::Never => false,
            Restart::OnFailure => code != 0,
            Restart::Always => true,
        };
        if !restart {
            if runtime.state == State::Starting && service.readiness != Readiness::None {
                runtime.state = State::Failed;
                runtime.ready_deadline_ms = None;
                return ExitOutcome {
                    restart: None,
                    start: None,
                    notices: alloc::vec![Notice::Failed(
                        index,
                        "exited before it became ready".to_string()
                    )],
                };
            }
            runtime.state = State::Stopped;
            runtime.ready_deadline_ms = None;
            return ExitOutcome::default();
        }

        if runtime.restarts == 0 {
            runtime.window_start_ms = now;
        } else if now.saturating_sub(runtime.window_start_ms) > u64::from(service.window_ms) {
            runtime.restarts = 0;
            runtime.window_start_ms = now;
        }
        if runtime.restarts >= service.max_restarts {
            runtime.state = State::GaveUp;
            runtime.ready_deadline_ms = None;
            return ExitOutcome {
                restart: None,
                start: None,
                notices: alloc::vec![Notice::CrashLoop(index)],
            };
        }
        runtime.restarts = runtime.restarts.saturating_add(1);
        runtime.state = State::Restarting;
        runtime.ready_deadline_ms = None;
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.restart_at_ms = Some(now.saturating_add(u64::from(service.restart_delay_ms)));
        ExitOutcome {
            restart: Some(RestartDecision {
                generation: runtime.generation,
                delay_ms: service.restart_delay_ms,
            }),
            start: None,
            notices: Vec::new(),
        }
    }

    /// Request that a service stop and return the child handle the caller must terminate.
    ///
    /// `restart` also schedules a new generation after the child reaches terminal state. The
    /// state changes to [`State::Stopped`] only from [`Supervisor::exited`] once the worker has
    /// observed the exit, so a client polling the status line cannot report success early.
    pub fn stop(&mut self, index: usize, restart: bool) -> Option<Handle> {
        if index >= self.services.len() {
            return None;
        }
        let runtime = self.runtime.get_mut(index)?;
        if let Some(handle) = runtime.handle {
            runtime.stop_requested = true;
            runtime.restart_after_stop = restart;
            runtime.ready_deadline_ms = None;
            runtime.restart_at_ms = None;
            return Some(handle);
        }
        match runtime.state {
            State::Starting | State::Running => {
                runtime.stop_requested = true;
                runtime.restart_after_stop = restart;
                runtime.ready_deadline_ms = None;
                runtime.restart_at_ms = None;
            }
            _ => {
                runtime.state = State::Stopped;
                runtime.ready_deadline_ms = None;
                runtime.restart_at_ms = None;
                runtime.stop_requested = false;
                runtime.restart_after_stop = false;
            }
        }
        None
    }

    /// True when the worker that just spawned should terminate its child immediately.
    pub fn stop_pending(&self, index: usize, generation: u64) -> bool {
        self.runtime
            .get(index)
            .is_some_and(|runtime| runtime.generation == generation && runtime.stop_requested)
    }

    /// Start a stopped, failed or given-up service.
    ///
    /// Running and starting services are left alone; the caller reports `EBUSY`.
    pub fn request_start(&mut self, index: usize, now: u64) -> Option<StartRequest> {
        let state = self.runtime.get(index)?.state;
        if !matches!(state, State::Stopped | State::Failed | State::GaveUp) {
            return None;
        }
        let generation = self.start(index, now)?;
        Some(StartRequest { index, generation })
    }

    /// True when a queued start request is still current and should be executed.
    pub fn should_start(&self, request: StartRequest) -> bool {
        self.runtime.get(request.index).is_some_and(|runtime| {
            runtime.generation == request.generation
                && runtime.state == State::Starting
                && !runtime.stop_requested
        })
    }

    /// True when `index` is still waiting for the restart scheduled by [`Supervisor::exited`].
    pub fn restart_current(&self, index: usize, generation: u64) -> bool {
        self.runtime.get(index).is_some_and(|runtime| {
            runtime.generation == generation && runtime.state == State::Restarting
        })
    }

    /// Move a scheduled restart back to `Starting` before the caller spawns it.
    ///
    /// Returns `false` when the generation is stale or a `STOP` cancelled the restart.
    pub fn begin_restart(&mut self, index: usize, generation: u64, now: u64) -> bool {
        let Some(service) = self.services.get(index) else {
            return false;
        };
        let Some(runtime) = self.runtime.get_mut(index) else {
            return false;
        };
        if runtime.generation != generation || runtime.state != State::Restarting {
            return false;
        }
        runtime.state = State::Starting;
        runtime.handle = None;
        runtime.ready_deadline_ms = match service.readiness {
            Readiness::None => None,
            _ => Some(now.saturating_add(u64::from(service.ready_timeout_ms))),
        };
        runtime.restart_at_ms = None;
        true
    }

    /// Status record for `svc:/<name>`.
    pub fn status(&self, index: usize) -> Option<Status<'_>> {
        let service = self.services.get(index)?;
        let runtime = self.runtime.get(index)?;
        Some(Status {
            name: service.name.as_str(),
            state: runtime.state,
            handle: runtime.handle.map_or(0, |handle| handle.0),
            restarts: runtime.restarts,
        })
    }

    /// True when every `restart != never` or required enabled service is running.
    pub fn all_up(&self) -> bool {
        self.services
            .iter()
            .zip(self.runtime.iter())
            .all(|(service, runtime)| {
                if !service.enable {
                    return true;
                }
                if service.restart == Restart::Never && !service.required {
                    return true;
                }
                runtime.state == State::Running
            })
    }

    /// True when no enabled service is starting or waiting to restart.
    ///
    /// Only meaningful after [`Supervisor::set_boot_complete`]; services that stopped cleanly,
    /// gave up or failed readiness all count as settled.
    pub fn settled(&self) -> bool {
        self.boot_complete
            && self
                .services
                .iter()
                .zip(self.runtime.iter())
                .all(|(service, runtime)| {
                    !service.enable
                        || (runtime.started
                            && !matches!(runtime.state, State::Starting | State::Restarting))
                })
    }

    /// Exit code for the settled state: 1 when a required service failed or gave up.
    pub fn settled_exit_code(&self) -> i32 {
        let failed = self
            .services
            .iter()
            .zip(self.runtime.iter())
            .any(|(service, runtime)| {
                service.enable
                    && service.required
                    && matches!(runtime.state, State::Failed | State::GaveUp)
            });
        if failed { 1 } else { 0 }
    }

    /// Names in manifest order, used by the `svc:` directory listing.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.services.iter().map(|service| service.name.as_str())
    }
}

#[cfg(test)]
#[path = "state_ut.rs"]
mod tests;
