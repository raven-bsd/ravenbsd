// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure svcd supervision state machine.

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
fn stop_keeps_the_handle_until_the_worker_observes_exit() {
    let mut supervisor = supervisor(vec![service("a")]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 0);
    assert_eq!(supervisor.stop(0, false), Some(Handle(7)));
    assert_eq!(supervisor.runtime()[0].state, State::Running);
    assert!(supervisor.stop_pending(0, generation));
    let outcome = supervisor.exited(0, generation, 0, 10);
    assert!(outcome.restart.is_none());
    assert_eq!(supervisor.runtime()[0].state, State::Stopped);
    assert_eq!(supervisor.runtime()[0].handle, None);
}

#[test]
fn stop_cancels_a_pending_restart() {
    let mut supervisor = supervisor(vec![running_with_restarts("a", 3)]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 0);
    supervisor.exited(0, generation, 1, 10);
    assert_eq!(supervisor.runtime()[0].state, State::Restarting);
    assert_eq!(supervisor.stop(0, false), None);
    assert_eq!(supervisor.runtime()[0].state, State::Stopped);
    assert_eq!(supervisor.runtime()[0].restart_at_ms, None);
}

#[test]
fn stop_before_spawn_is_completed_after_spawn() {
    let mut supervisor = supervisor(vec![service("a")]);
    let generation = supervisor.start(0, 0).unwrap();
    assert_eq!(supervisor.stop(0, false), None);
    assert!(supervisor.stop_pending(0, generation));
    supervisor.spawned(0, generation, Handle(7), 0);
    assert!(supervisor.stop_pending(0, generation));
    supervisor.exited(0, generation, 0, 10);
    assert_eq!(supervisor.runtime()[0].state, State::Stopped);
}

#[test]
fn explicit_restart_starts_a_new_generation_after_stop() {
    let mut supervisor = supervisor(vec![service("a")]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 0);
    assert_eq!(supervisor.stop(0, true), Some(Handle(7)));
    let outcome = supervisor.exited(0, generation, 0, 10);
    let request = outcome.start.expect("restart start");
    assert_eq!(request.index, 0);
    assert_eq!(supervisor.runtime()[0].state, State::Starting);
    assert!(supervisor.should_start(request));
}

#[test]
fn start_requests_only_apply_to_terminal_states() {
    let mut supervisor = supervisor(vec![service("a")]);
    assert!(supervisor.request_start(0, 0).is_some());
    assert!(supervisor.request_start(0, 1).is_none());
    let generation = supervisor.runtime()[0].generation;
    supervisor.spawned(0, generation, Handle(7), 2);
    assert!(supervisor.request_start(0, 3).is_none());
    supervisor.stop(0, false);
    supervisor.exited(0, generation, 0, 4);
    assert!(supervisor.request_start(0, 4).is_some());
}

#[test]
fn queued_start_must_still_be_current() {
    let mut supervisor = supervisor(vec![service("a")]);
    let request = supervisor.request_start(0, 0).unwrap();
    assert!(supervisor.should_start(request));
    supervisor.stop(0, false);
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
    supervisor.stop(0, false);
    supervisor.exited(0, generation, 0, 10);
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
    assert!(supervisor.stop(9, false).is_none());
    assert!(supervisor.status(9).is_none());
    assert!(supervisor.request_start(9, 0).is_none());
    assert!(supervisor.exited(9, 1, 0, 0).restart.is_none());
}
