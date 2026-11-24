// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pure svcd supervision state machine.

use super::*;
use alloc::string::String;
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
            Notice::Unavailable(index, reason) => {
                alloc::format!("unavailable {index} {reason}")
            }
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
    assert_eq!(supervisor.runtime()[0].state, State::Starting);
    // The window is armed when the child exists, not when the plan selects it.
    assert_eq!(supervisor.runtime()[0].ready_deadline_ms, None);
    assert_eq!(supervisor.runtime()[0].generation, 1);
    assert!(supervisor.spawned(0, generation, Handle(7), 130).is_empty());
    assert_eq!(supervisor.runtime()[0].ready_deadline_ms, Some(5_130));
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
    assert_eq!(up_names(&supervisor.ready(0, generation, 5)), ["up 0"]);
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
    assert_eq!(up_names(&notices), ["unavailable 0 readiness timeout"]);
    assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
}

#[test]
fn scheme_readiness_without_a_provider_times_out() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 0);
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
        ["unavailable 0 exited before it became ready"]
    );
    assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
}

#[test]
fn startup_exit_after_restarts_is_unavailable() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    manifest.restart = Restart::OnFailure;
    manifest.max_restarts = 1;
    manifest.restart_delay_ms = 1;
    let mut supervisor = supervisor(vec![manifest]);
    let mut generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 0);
    let first = supervisor.exited(0, generation, 1, 10);
    generation = first.restart.expect("restart").generation;
    assert!(supervisor.begin_restart(0, generation, 20));
    supervisor.spawned(0, generation, Handle(8), 20);
    let second = supervisor.exited(0, generation, 1, 30);
    assert!(second.restart.is_none());
    assert_eq!(
        up_names(&second.notices),
        ["unavailable 0 could not become ready"]
    );
    assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
}

#[test]
fn dependency_closure_follows_service_and_scheme_needs() {
    let mut pcid = service("pcid");
    pcid.provides = vec!["scheme:pci".to_string()];
    let mut netd = with_needs("netd", &["scheme:pci"]);
    netd.provides = vec!["scheme:net".to_string()];
    let app = with_needs("app", &["scheme:net"]);
    let unrelated = service("unrelated");
    let supervisor = supervisor(vec![pcid, netd, app, unrelated]);
    assert_eq!(
        supervisor.dependency_closure(2),
        alloc::vec![true, true, true, false]
    );
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
    assert_eq!(up_names(&notices), ["unavailable 0 no initfs"]);
    assert_eq!(supervisor.runtime()[0].state, State::GaveUp);
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
fn health_ready_waits_for_required_exit_services() {
    let mut required = service("required");
    required.readiness = Readiness::Exit;
    let mut optional = service("optional");
    optional.readiness = Readiness::Exit;
    optional.required = false;
    let mut supervisor = supervisor(vec![required, optional]);
    assert!(!supervisor.health_ready());

    let required_generation = supervisor.start(0, 0).unwrap();
    let optional_generation = supervisor.start(1, 0).unwrap();
    assert!(!supervisor.health_ready());
    supervisor.exited(1, optional_generation, 0, 10);
    assert!(!supervisor.health_ready());
    supervisor.exited(0, required_generation, 0, 10);
    assert!(supervisor.health_ready());
}

#[test]
fn health_ready_ignores_optional_failures_and_rejects_required_failures() {
    let mut optional = service("optional");
    optional.required = false;
    let required = service("required");
    let mut mixed = supervisor(vec![required, optional]);
    let required_generation = mixed.start(0, 0).unwrap();
    mixed.spawned(0, required_generation, Handle(7), 0);
    let optional_generation = mixed.start(1, 0).unwrap();
    mixed.spawn_failed(1, optional_generation, "optional failure".to_string());
    assert!(mixed.health_ready());

    let mut failing = supervisor(vec![service("required")]);
    let generation = failing.start(0, 0).unwrap();
    failing.spawn_failed(0, generation, "required failure".to_string());
    assert!(!failing.health_ready());
}

#[test]
fn names_are_in_manifest_order() {
    let supervisor = supervisor(vec![service("b"), service("a")]);
    assert_eq!(supervisor.names().collect::<Vec<_>>(), ["b", "a"]);
}

#[test]
fn merge_adds_root_services_and_recomputes_the_plan() {
    let mut supervisor = supervisor(vec![service("a")]);
    let added = supervisor
        .merge(vec![with_needs("b", &["a"]), service("c")], None)
        .unwrap();
    assert_eq!(added, 2);
    assert_eq!(supervisor.names().collect::<Vec<_>>(), ["a", "b", "c"]);
    assert_eq!(supervisor.plan().waves.len(), 2);
}

#[test]
fn merge_replaces_a_manifest_without_resetting_its_runtime() {
    let mut supervisor = supervisor(vec![service("a")]);
    let generation = supervisor.start(0, 0).unwrap();
    let mut replacement = service("a");
    replacement.exec = "/root/sbin/a".to_string();
    let added = supervisor.merge(vec![replacement], None).unwrap();
    assert_eq!(added, 0);
    assert_eq!(supervisor.services()[0].exec, "/root/sbin/a");
    assert_eq!(supervisor.runtime()[0].generation, generation);
    assert!(supervisor.runtime()[0].started);
}

#[test]
fn merge_applies_root_restart_overrides() {
    let overrides = Overrides {
        entries: vec![(
            "a".to_string(),
            raven_svc_manifest::Override::Restart(Restart::Never),
        )],
    };
    let mut supervisor = supervisor(vec![service("a")]);
    supervisor.merge(Vec::new(), Some(&overrides)).unwrap();
    assert_eq!(supervisor.services()[0].restart, Restart::Never);
}

#[test]
fn merge_rejects_an_unknown_override_without_changing_the_set() {
    let overrides = Overrides {
        entries: vec![(
            "missing".to_string(),
            raven_svc_manifest::Override::Restart(Restart::Never),
        )],
    };
    let mut supervisor = supervisor(vec![service("a")]);
    assert_eq!(
        supervisor.merge(vec![service("b")], Some(&overrides)),
        Err(MergeError::UnknownService("missing".to_string()))
    );
    assert_eq!(supervisor.names().collect::<Vec<_>>(), ["a"]);
}

#[test]
fn merge_rolls_back_when_the_merged_graph_is_invalid() {
    let mut supervisor = supervisor(vec![service("a")]);
    let error = supervisor
        .merge(vec![with_needs("b", &["missing"])], None)
        .unwrap_err();
    assert!(matches!(
        error,
        MergeError::Graph(GraphError::Missing { .. })
    ));
    assert_eq!(supervisor.names().collect::<Vec<_>>(), ["a"]);
    assert_eq!(supervisor.runtime().len(), 1);
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

fn with_provides(name: &str, provides: &[&str]) -> Manifest {
    let mut manifest = service(name);
    manifest.provides = provides.iter().map(|provide| provide.to_string()).collect();
    manifest
}

#[test]
fn accepts_a_late_scheme_dependency() {
    let supervisor = supervisor(vec![with_needs("blockd", &["scheme:disk"])]);
    assert_eq!(supervisor.plan().waves, vec![vec![0]]);
    assert_eq!(
        supervisor.dynamic_scheme_prefixes(),
        vec!["disk".to_string()]
    );
}

#[test]
fn keeps_ordering_for_an_enabled_scheme_provider() {
    let blockd = with_provides("blockd", &["scheme:block"]);
    let fsd = with_needs("fsd-ufs", &["scheme:block"]);
    let supervisor = supervisor(vec![fsd, blockd]);
    assert_eq!(supervisor.plan().waves, vec![vec![1], vec![0]]);
    assert!(supervisor.dynamic_scheme_prefixes().is_empty());
}

#[test]
fn still_rejects_an_ambiguous_scheme_provider() {
    let first = with_provides("first", &["scheme:block"]);
    let second = with_provides("second", &["scheme:block"]);
    let user = with_needs("user", &["scheme:block"]);
    assert_eq!(
        Supervisor::new(vec![first, second, user]).unwrap_err(),
        GraphError::AmbiguousProvider
    );
}

#[test]
fn dynamic_prefixes_include_disabled_providers_and_late_needs() {
    let mut blockd = with_provides("blockd", &["scheme:block"]);
    blockd.enable = false;
    blockd.needs = alloc::vec!["scheme:disk".to_string()];
    let mut fsd = with_provides("fsd-ufs", &["scheme:ufs"]);
    fsd.enable = false;
    let mut shell = service("getty");
    shell.provides = alloc::vec!["scheme:file".to_string()];
    // `file` is provided by the enabled `getty`; `block`, `ufs` and `disk` are not.
    let supervisor = supervisor(vec![blockd, fsd, shell]);
    assert_eq!(
        supervisor.dynamic_scheme_prefixes(),
        vec!["block".to_string(), "disk".to_string(), "ufs".to_string()]
    );
}

#[test]
fn skips_invalid_and_duplicate_scheme_names() {
    let mut odd = with_provides("odd", &["scheme:Block", "scheme:net"]);
    odd.enable = false;
    let other = with_provides("other", &["scheme:net"]);
    let supervisor = supervisor(vec![odd, other]);
    assert_eq!(supervisor.dynamic_scheme_prefixes(), Vec::<String>::new());
}

#[test]
fn release_disk_manifests_defer_the_late_disk_need() {
    let mut blockd = Manifest::parse(include_str!("../services/blockd.toml")).expect("blockd");
    blockd.enable = true;
    let supervisor = supervisor(vec![blockd]);
    // `scheme:disk` has no enabled provider (devd publishes it at runtime) but must not fail the
    // graph, and the service still starts in the first wave.
    assert_eq!(supervisor.plan().waves, vec![vec![0]]);
}

#[test]
fn release_disk_manifests_grant_block_ufs_and_disk_prefixes() {
    let blockd = Manifest::parse(include_str!("../services/blockd.toml")).expect("blockd");
    let fsd = Manifest::parse(include_str!("../services/fsd-ufs.toml")).expect("fsd-ufs");
    let supervisor = supervisor(vec![blockd, fsd]);
    assert_eq!(
        supervisor.dynamic_scheme_prefixes(),
        vec!["block".to_string(), "disk".to_string(), "ufs".to_string()]
    );
}

#[test]
fn ready_timing_line_is_standard_and_carries_the_four_stamps() {
    let timing = ReadyTiming {
        generation: 3,
        start_ms: Some(10),
        spawned_ms: Some(20),
        first_poll_ms: Some(25),
        deadline_ms: Some(5_020),
        stalled_ms: 0,
        finished_ms: Some(30),
    };
    assert_eq!(
        format_ready_timing("vfsd", &timing, "ready_at"),
        "svcd: vfsd timing: generation=3 start=10 spawn=20 first-ready=25 ready=30 deadline=5020 stalled=0 status=ready_at"
    );

    let no_readiness = ReadyTiming {
        generation: 1,
        start_ms: Some(10),
        spawned_ms: Some(42),
        first_poll_ms: None,
        deadline_ms: None,
        stalled_ms: 0,
        finished_ms: Some(42),
    };
    assert_eq!(
        format_ready_timing("logd", &no_readiness, "ready_at"),
        "svcd: logd timing: generation=1 start=10 spawn=42 first-ready=none ready=42 deadline=none stalled=0 status=ready_at"
    );
}

#[test]
fn ready_timing_records_start_spawn_poll_and_finish() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 1_000).unwrap();
    assert_eq!(
        supervisor.runtime()[0].ready_timing,
        ReadyTiming {
            generation,
            start_ms: Some(1_000),
            spawned_ms: None,
            first_poll_ms: None,
            deadline_ms: None,
            stalled_ms: 0,
            finished_ms: None,
        }
    );
    supervisor.spawned(0, generation, Handle(7), 1_120);
    supervisor.polled(0, generation, 1_135);
    supervisor.polled(0, generation, 1_145);
    let timing = supervisor.runtime()[0].ready_timing;
    assert_eq!(timing.spawned_ms, Some(1_120));
    assert_eq!(timing.first_poll_ms, Some(1_135));
    assert_eq!(timing.deadline_ms, Some(6_120));
    assert_eq!(timing.finished_ms, None);
    supervisor.ready(0, generation, 1_180);
    assert_eq!(
        supervisor.runtime()[0].ready_timing.finished_ms,
        Some(1_180)
    );
}

#[test]
fn ready_timing_keeps_the_deadline_after_a_timeout() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    manifest.ready_timeout_ms = 100;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 500).unwrap();
    supervisor.spawned(0, generation, Handle(7), 530);
    supervisor.polled(0, generation, 540);
    assert!(
        supervisor
            .readiness_timed_out(0, generation, 629)
            .is_empty()
    );
    supervisor.readiness_timed_out(0, generation, 630);
    let runtime = &supervisor.runtime()[0];
    assert_eq!(runtime.ready_deadline_ms, None);
    assert_eq!(runtime.ready_timing.deadline_ms, Some(630));
    assert_eq!(runtime.ready_timing.spawned_ms, Some(530));
    assert_eq!(runtime.ready_timing.first_poll_ms, Some(540));
    assert_eq!(runtime.ready_timing.finished_ms, Some(630));
}

#[test]
fn ready_timing_ignores_stale_generations() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 100).unwrap();
    supervisor.spawned(0, generation, Handle(7), 110);
    supervisor.polled(0, generation + 1, 120);
    assert_eq!(supervisor.runtime()[0].ready_timing.first_poll_ms, None);
    assert!(supervisor.ready(0, generation + 1, 130).is_empty());
    assert_eq!(supervisor.runtime()[0].ready_timing.finished_ms, None);
}

#[test]
fn ready_timing_marks_a_no_readiness_service_at_spawn() {
    let mut supervisor = supervisor(vec![service("a")]);
    let generation = supervisor.start(0, 10).unwrap();
    let notices = supervisor.spawned(0, generation, Handle(7), 42);
    assert_eq!(up_names(&notices), ["up 0"]);
    let timing = supervisor.runtime()[0].ready_timing;
    assert_eq!(timing.start_ms, Some(10));
    assert_eq!(timing.spawned_ms, Some(42));
    assert_eq!(timing.deadline_ms, None);
    assert_eq!(timing.finished_ms, Some(42));
}

#[test]
fn begin_restart_resets_ready_timing_for_the_new_generation() {
    let mut manifest = running_with_restarts("a", 3);
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 100).unwrap();
    supervisor.spawned(0, generation, Handle(7), 110);
    supervisor.polled(0, generation, 120);
    let outcome = supervisor.exited(0, generation, 1, 200);
    let restart = outcome.restart.expect("restart should be scheduled");
    assert!(supervisor.begin_restart(0, restart.generation, 500));
    let timing = supervisor.runtime()[0].ready_timing;
    assert_eq!(timing.generation, restart.generation);
    assert_eq!(timing.start_ms, Some(500));
    assert_eq!(timing.spawned_ms, None);
    assert_eq!(timing.first_poll_ms, None);
    assert_eq!(timing.deadline_ms, None);
    assert_eq!(timing.finished_ms, None);
    assert!(
        supervisor
            .spawned(0, restart.generation, Handle(8), 520)
            .is_empty()
    );
    assert_eq!(
        supervisor.runtime()[0].ready_timing.deadline_ms,
        Some(5_520)
    );
}

#[test]
fn stalled_poll_gap_extends_the_readiness_deadline() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 100);
    supervisor.polled(0, generation, 110);
    // Spawn armed a 5 000 ms window (deadline 5 100); a 4 000 ms unobserved stall moves it.
    assert!(supervisor.extend_readiness(0, generation, 4_000));
    assert_eq!(supervisor.runtime()[0].ready_deadline_ms, Some(9_100));
    assert_eq!(supervisor.runtime()[0].ready_timing.stalled_ms, 4_000);
    assert!(
        supervisor
            .readiness_timed_out(0, generation, 9_099)
            .is_empty()
    );
    let notices = supervisor.readiness_timed_out(0, generation, 9_100);
    assert_eq!(up_names(&notices), ["unavailable 0 readiness timeout"]);
    assert_eq!(supervisor.runtime()[0].ready_timing.stalled_ms, 4_000);
}

#[test]
fn zero_or_stale_stall_extension_leaves_the_deadline_alone() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 100);
    assert!(!supervisor.extend_readiness(0, generation, 0));
    assert!(!supervisor.extend_readiness(0, generation + 1, 4_000));
    assert_eq!(supervisor.runtime()[0].ready_deadline_ms, Some(5_100));
    assert_eq!(supervisor.runtime()[0].ready_timing.stalled_ms, 0);
}

#[test]
fn release_plan_starts_ptyd_before_vfsd() {
    let vfsd =
        Manifest::parse(include_str!("../../../etc/services.d/vfsd.toml")).expect("vfsd manifest");
    let ptyd = Manifest::parse(include_str!(
        "../../../profiles/ssh.d/ptyd.service.manifest"
    ))
    .expect("ptyd manifest");
    let mut services = vec![vfsd, ptyd];
    for name in ["logd", "initfsd", "ramfsd", "nulld", "zerod", "randd"] {
        services.push(service(name));
    }
    let plan = start_order(&services).expect("release service graph");
    let wave = |name: &str| {
        plan.waves
            .iter()
            .position(|wave| wave.iter().any(|&index| services[index].name == name))
            .unwrap_or(usize::MAX)
    };
    assert!(
        wave("ptyd") < wave("vfsd"),
        "ptyd wave {} and vfsd wave {}: vfsd must declare the pty scheme dependency, or its /dev/pts mount waits for the 5 s scheme timeout",
        wave("ptyd"),
        wave("vfsd")
    );
}

#[test]
fn a_ready_worker_still_times_out_a_hung_child() {
    let mut manifest = service("a");
    manifest.readiness = Readiness::Scheme;
    manifest.ready_timeout_ms = 5_000;
    let mut supervisor = supervisor(vec![manifest]);
    let generation = supervisor.start(0, 0).unwrap();
    supervisor.spawned(0, generation, Handle(7), 100);
    // Polls every 10 ms with no stall: the window is not extended and the deadline fires.
    for poll in 1..499 {
        supervisor.polled(0, generation, 110 + poll * 10);
        assert!(
            supervisor
                .readiness_timed_out(0, generation, 110 + poll * 10)
                .is_empty()
        );
    }
    let notices = supervisor.readiness_timed_out(0, generation, 5_100);
    assert_eq!(up_names(&notices), ["unavailable 0 readiness timeout"]);
}
