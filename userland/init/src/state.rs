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
use raven_svc_manifest::{GraphError, Manifest, Plan, Readiness, Restart, start_order};

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
    /// Messages to print.
    pub notices: Vec<Notice>,
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
        runtime.handle = None;
        runtime.last_exit = Some(code);
        if runtime.state == State::Starting && service.readiness == Readiness::Exit && code == 0 {
            runtime.state = State::Stopped;
            runtime.ever_ready = true;
            runtime.ready_deadline_ms = None;
            return ExitOutcome {
                restart: None,
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
            notices: Vec::new(),
        }
    }

    /// Stop a service and return the child handle the caller must close.
    ///
    /// Closing a process handle only drops the capability; without signals the child keeps
    /// running until it exits. The worker ignores the later exit because the generation no longer
    /// matches and the state is [`State::Stopped`].
    pub fn stop(&mut self, index: usize) -> Option<Handle> {
        if index >= self.services.len() {
            return None;
        }
        let runtime = self.runtime.get_mut(index)?;
        let handle = runtime.handle.take();
        runtime.state = State::Stopped;
        runtime.ready_deadline_ms = None;
        runtime.restart_at_ms = None;
        handle
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
            runtime.generation == request.generation && runtime.state == State::Starting
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
mod tests {
    use super::*;
    use alloc::string::ToString;
    use alloc::vec;

    fn service(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            exec: alloc::format!("/sbin/{name}"),
            ..Manifest::default()
        }
    }

    fn with_needs(name: &str, needs: &[&str]) -> Manifest {
        let mut manifest = service(name);
        manifest.needs = needs.iter().map(|need| need.to_string()).collect();
        manifest
    }

    fn supervisor(services: Vec<Manifest>) -> Supervisor {
        Supervisor::new(services).expect("graph should be valid")
    }

    fn up_names(notices: &[Notice]) -> Vec<String> {
        notices
            .iter()
            .map(|notice| match notice {
                Notice::Up(index) => alloc::format!("up {index}"),
                Notice::Failed(index, reason) => alloc::format!("failed {index} {reason}"),
                Notice::CrashLoop(index) => alloc::format!("crash {index}"),
            })
            .collect()
    }

    fn running_with_restarts(name: &str, restarts: u32) -> Manifest {
        let mut manifest = service(name);
        manifest.restart = Restart::Always;
        manifest.max_restarts = restarts;
        manifest.restart_delay_ms = 10;
        manifest
    }

    #[test]
    fn rejects_missing_dependencies() {
        assert!(matches!(
            Supervisor::new(vec![with_needs("a", &["missing"])]).unwrap_err(),
            GraphError::Missing { .. }
        ));
    }

    #[test]
    fn rejects_cycles() {
        assert!(matches!(
            Supervisor::new(vec![with_needs("a", &["b"]), with_needs("b", &["a"])]).unwrap_err(),
            GraphError::Cycle(_)
        ));
    }

    #[test]
    fn plan_is_exposed() {
        let supervisor = supervisor(vec![with_needs("b", &["a"]), service("a")]);
        assert_eq!(supervisor.plan().waves.len(), 2);
        assert_eq!(supervisor.plan().order().len(), 2);
    }

    #[test]
    fn start_sets_deadline_and_generation() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 100).unwrap();
        assert_eq!(generation, 1);
        let runtime = &supervisor.runtime()[0];
        assert_eq!(runtime.state, State::Starting);
        assert_eq!(runtime.ready_deadline_ms, Some(5_100));
        assert_eq!(runtime.generation, 1);
    }

    #[test]
    fn readiness_none_marks_running() {
        let mut supervisor = supervisor(vec![service("a")]);
        let generation = supervisor.start(0, 0).unwrap();
        let notices = supervisor.spawned(0, generation, Handle(7), 0);
        assert_eq!(up_names(&notices), ["up 0"]);
        assert_eq!(supervisor.runtime()[0].state, State::Running);
        assert!(supervisor.runtime()[0].ever_ready);
    }

    #[test]
    fn scheme_readiness_waits_for_ready() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        assert!(supervisor.spawned(0, generation, Handle(7), 0).is_empty());
        assert_eq!(supervisor.runtime()[0].state, State::Starting);
        assert_eq!(up_names(&supervisor.ready(0, generation)), ["up 0"]);
        assert_eq!(supervisor.runtime()[0].state, State::Running);
    }

    #[test]
    fn readiness_timeout_is_not_premature() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        manifest.ready_timeout_ms = 100;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        assert!(supervisor.readiness_timed_out(0, generation, 99).is_empty());
        let notices = supervisor.readiness_timed_out(0, generation, 100);
        assert_eq!(up_names(&notices), ["failed 0 readiness timeout"]);
        assert_eq!(supervisor.runtime()[0].state, State::Failed);
    }

    #[test]
    fn scheme_readiness_without_a_provider_times_out() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        assert!(
            supervisor
                .readiness_timed_out(0, generation, 4_999)
                .is_empty()
        );
        let notices = supervisor.readiness_timed_out(0, generation, 5_000);
        assert_eq!(notices.len(), 1);
    }

    #[test]
    fn restart_never_stops_after_exit() {
        let mut manifest = service("a");
        manifest.restart = Restart::Never;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let outcome = supervisor.exited(0, generation, 1, 10);
        assert!(outcome.restart.is_none());
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
    }

    #[test]
    fn on_failure_ignores_clean_exit() {
        let mut manifest = service("a");
        manifest.restart = Restart::OnFailure;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let outcome = supervisor.exited(0, generation, 0, 10);
        assert!(outcome.restart.is_none());
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
    }

    #[test]
    fn exit_before_readiness_is_a_failure() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        manifest.restart = Restart::Never;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let notices = supervisor.exited(0, generation, 0, 10);
        assert_eq!(
            up_names(&notices.notices),
            ["failed 0 exited before it became ready"]
        );
        assert_eq!(supervisor.runtime()[0].state, State::Failed);
    }

    #[test]
    fn on_failure_restarts_after_nonzero_exit() {
        let mut manifest = service("a");
        manifest.restart = Restart::OnFailure;
        manifest.restart_delay_ms = 250;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let outcome = supervisor.exited(0, generation, 1, 1_000);
        let restart = outcome.restart.expect("restart");
        assert_eq!(restart.delay_ms, 250);
        assert_eq!(restart.generation, generation + 1);
        assert_eq!(supervisor.runtime()[0].state, State::Restarting);
        assert_eq!(supervisor.runtime()[0].restart_at_ms, Some(1_250));
        assert_eq!(supervisor.runtime()[0].restarts, 1);
    }

    #[test]
    fn always_restarts_after_clean_exit() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        assert!(
            supervisor.exited(0, generation, 0, 10).restart.is_some(),
            "always restarts"
        );
    }

    #[test]
    fn crash_loop_gives_up_at_the_limit() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 2)]);
        let mut generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let first = supervisor.exited(0, generation, 1, 10);
        generation = first.restart.unwrap().generation;
        assert!(supervisor.begin_restart(0, generation, 20));
        supervisor.spawned(0, generation, Handle(8), 20);
        let second = supervisor.exited(0, generation, 1, 30);
        generation = second.restart.unwrap().generation;
        assert!(supervisor.begin_restart(0, generation, 40));
        supervisor.spawned(0, generation, Handle(9), 40);
        let third = supervisor.exited(0, generation, 1, 50);
        assert!(third.restart.is_none());
        assert_eq!(up_names(&third.notices), ["crash 0"]);
        assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
    }

    #[test]
    fn zero_max_restarts_gives_up_immediately() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 0)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let outcome = supervisor.exited(0, generation, 1, 10);
        assert_eq!(up_names(&outcome.notices), ["crash 0"]);
        assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
    }

    #[test]
    fn restart_window_resets_the_counter() {
        let mut manifest = running_with_restarts("a", 1);
        manifest.window_ms = 100;
        manifest.restart_delay_ms = 1;
        let mut supervisor = supervisor(vec![manifest]);
        let mut generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        generation = supervisor
            .exited(0, generation, 1, 10)
            .restart
            .unwrap()
            .generation;
        assert!(supervisor.begin_restart(0, generation, 20));
        supervisor.spawned(0, generation, Handle(8), 20);
        // The second exit is far outside the first window, so the counter resets and a restart
        // is allowed instead of the crash loop.
        let outcome = supervisor.exited(0, generation, 1, 1_000);
        assert!(outcome.restart.is_some());
        assert_eq!(supervisor.runtime()[0].restarts, 1);
    }

    #[test]
    fn stale_generation_events_are_ignored() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        let restart = supervisor.exited(0, generation, 1, 10).restart.unwrap();
        assert!(supervisor.begin_restart(0, restart.generation, 20));
        supervisor.spawned(0, restart.generation, Handle(8), 20);
        assert!(supervisor.exited(0, generation, 1, 30).restart.is_none());
        assert_eq!(supervisor.runtime()[0].state, State::Running);
    }

    #[test]
    fn stop_takes_the_handle_and_ignores_late_exit() {
        let mut supervisor = supervisor(vec![service("a")]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        assert_eq!(supervisor.stop(0), Some(Handle(7)));
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
        assert!(supervisor.exited(0, generation, 1, 10).restart.is_none());
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
    }

    #[test]
    fn stop_cancels_a_pending_restart() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        supervisor.exited(0, generation, 1, 10);
        assert_eq!(supervisor.runtime()[0].state, State::Restarting);
        assert_eq!(supervisor.stop(0), None);
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
        assert_eq!(supervisor.runtime()[0].restart_at_ms, None);
    }

    #[test]
    fn start_requests_only_apply_to_terminal_states() {
        let mut supervisor = supervisor(vec![service("a")]);
        assert!(supervisor.request_start(0, 0).is_some());
        assert!(supervisor.request_start(0, 1).is_none());
        let generation = supervisor.runtime()[0].generation;
        supervisor.spawned(0, generation, Handle(7), 2);
        assert!(supervisor.request_start(0, 3).is_none());
        supervisor.stop(0);
        assert!(supervisor.request_start(0, 4).is_some());
    }

    #[test]
    fn queued_start_must_still_be_current() {
        let mut supervisor = supervisor(vec![service("a")]);
        let request = supervisor.request_start(0, 0).unwrap();
        assert!(supervisor.should_start(request));
        supervisor.stop(0);
        assert!(!supervisor.should_start(request));
    }

    #[test]
    fn status_reports_handle_and_restarts() {
        let mut supervisor = supervisor(vec![service("a")]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(42), 0);
        let status = supervisor.status(0).unwrap();
        assert_eq!(status.name, "a");
        assert_eq!(status.state, State::Running);
        assert_eq!(status.handle, 42);
        assert_eq!(status.restarts, 0);
        supervisor.stop(0);
        assert_eq!(supervisor.status(0).unwrap().handle, 0);
    }

    #[test]
    fn find_locates_services() {
        let supervisor = supervisor(vec![service("a"), service("b")]);
        assert_eq!(supervisor.find("b"), Some(1));
        assert_eq!(supervisor.find("missing"), None);
    }

    #[test]
    fn settled_requires_boot_complete() {
        let supervisor = supervisor(vec![service("a")]);
        assert!(!supervisor.settled());
    }

    #[test]
    fn settled_is_false_while_starting() {
        let mut supervisor = supervisor(vec![service("a")]);
        supervisor.set_boot_complete();
        assert!(!supervisor.settled());
        let generation = supervisor.start(0, 0).unwrap();
        assert!(!supervisor.settled());
        supervisor.spawned(0, generation, Handle(7), 0);
        assert!(supervisor.settled());
    }

    #[test]
    fn settled_after_a_clean_exit() {
        let mut supervisor = supervisor(vec![service("a")]);
        supervisor.set_boot_complete();
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        supervisor.exited(0, generation, 0, 10);
        assert!(supervisor.settled());
        assert_eq!(supervisor.settled_exit_code(), 0);
    }

    #[test]
    fn optional_crash_loop_settles_successfully() {
        let mut manifest = running_with_restarts("a", 0);
        manifest.required = false;
        let mut supervisor = supervisor(vec![manifest]);
        supervisor.set_boot_complete();
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        supervisor.exited(0, generation, 1, 10);
        assert!(supervisor.settled());
        assert_eq!(supervisor.settled_exit_code(), 0);
    }

    #[test]
    fn required_crash_loop_fails_settled() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 0)]);
        supervisor.set_boot_complete();
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        supervisor.exited(0, generation, 1, 10);
        assert!(supervisor.settled());
        assert_eq!(supervisor.settled_exit_code(), 1);
    }

    #[test]
    fn required_readiness_failure_fails_settled() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Scheme;
        manifest.ready_timeout_ms = 10;
        let mut supervisor = supervisor(vec![manifest]);
        supervisor.set_boot_complete();
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        supervisor.readiness_timed_out(0, generation, 10);
        assert!(supervisor.settled());
        assert_eq!(supervisor.settled_exit_code(), 1);
    }

    #[test]
    fn all_up_ignores_optional_never_services() {
        let mut optional = service("optional");
        optional.restart = Restart::Never;
        optional.required = false;
        let mut required = service("required");
        required.restart = Restart::Never;
        let mut supervisor = supervisor(vec![optional, required]);
        assert!(!supervisor.all_up());
        let generation = supervisor.start(1, 0).unwrap();
        supervisor.spawned(1, generation, Handle(7), 0);
        assert!(supervisor.all_up());
    }

    #[test]
    fn all_up_waits_for_restarting_services() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        assert!(supervisor.all_up());
        supervisor.exited(0, generation, 1, 10);
        assert!(!supervisor.all_up());
    }

    #[test]
    fn ever_ready_survives_a_restart() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        supervisor.spawned(0, generation, Handle(7), 0);
        assert!(supervisor.runtime()[0].ever_ready);
        let restart = supervisor.exited(0, generation, 1, 10).restart.unwrap();
        assert!(supervisor.begin_restart(0, restart.generation, 20));
        supervisor.spawned(0, restart.generation, Handle(8), 20);
        assert!(supervisor.runtime()[0].ever_ready);
    }

    #[test]
    fn spawn_failure_is_reported_once() {
        let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
        let generation = supervisor.start(0, 0).unwrap();
        let notices = supervisor.spawn_failed(0, generation, "no initfs".to_string());
        assert_eq!(up_names(&notices), ["failed 0 no initfs"]);
        assert_eq!(supervisor.runtime()[0].state, State::Failed);
        assert!(
            supervisor
                .spawn_failed(0, generation, "again".to_string())
                .is_empty()
        );
    }

    #[test]
    fn readiness_exit_marks_up_when_clean() {
        let mut manifest = service("a");
        manifest.readiness = Readiness::Exit;
        let mut supervisor = supervisor(vec![manifest]);
        let generation = supervisor.start(0, 0).unwrap();
        let notices = supervisor.exited(0, generation, 0, 10);
        assert_eq!(up_names(&notices.notices), ["up 0"]);
        assert_eq!(supervisor.runtime()[0].state, State::Stopped);
        assert!(supervisor.runtime()[0].ever_ready);
    }

    #[test]
    fn names_are_in_manifest_order() {
        let supervisor = supervisor(vec![service("b"), service("a")]);
        assert_eq!(supervisor.names().collect::<Vec<_>>(), ["b", "a"]);
    }

    #[test]
    fn invalid_indices_are_ignored() {
        let mut supervisor = supervisor(vec![service("a")]);
        assert!(supervisor.start(9, 0).is_none());
        assert!(supervisor.stop(9).is_none());
        assert!(supervisor.status(9).is_none());
        assert!(supervisor.request_start(9, 0).is_none());
        assert!(supervisor.exited(9, 1, 0, 0).restart.is_none());
    }
}
