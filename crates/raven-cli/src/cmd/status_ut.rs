// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `raven status` with fake backends.

use raven_output::{Exit, Format};

use super::{BackendError, Link, Status, StatusBackend, run_with_backend};
use crate::Context;

struct Fake(Status);

impl StatusBackend for Fake {
    fn status(&self) -> Result<Status, BackendError> {
        Ok(self.0.clone())
    }
}

struct Broken;

impl StatusBackend for Broken {
    fn status(&self) -> Result<Status, BackendError> {
        Err(BackendError::new("EIO", "backend failed"))
    }
}

fn status(core_ok: bool) -> Status {
    Status {
        version: Some(String::from("0.1.0")),
        target: Some(String::from("x86_64-unknown-ravenbsd")),
        uptime_s: Some(42),
        boot_id: Some(String::from("boot-1")),
        services_running: Some(9),
        services_ready: Some(8),
        services_total: Some(9),
        load: Some([0.5, 0.25, 0.125]),
        memory_total: Some(268_435_456),
        memory_free: Some(201_326_592),
        root_mount: Some(String::from("/")),
        root_read_only: Some(false),
        links: vec![Link {
            name: String::from("eth0"),
            state: String::from("up"),
        }],
        log_stored: Some(123),
        log_dropped: Some(0),
        core_ok,
    }
}

fn run(backend: &dyn StatusBackend, format: Format) -> (Exit, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = {
        let mut context = Context::new(format, false, &mut stdout, &mut stderr);
        run_with_backend(&mut context, backend)
    };
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn full_status_is_golden_json() {
    let (exit, stdout, stderr) = run(&Fake(status(true)), Format::Json);
    assert_eq!(exit, Exit::Success);
    assert!(stderr.is_empty());
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["schema"], 1);
    assert_eq!(value["kind"], "status");
    assert_eq!(value["services"]["running"], 9);
    assert_eq!(value["services"]["ready"], 8);
    assert_eq!(value["load"][0], 0.5);
    assert_eq!(value["memory"]["total"], 268_435_456);
    assert_eq!(value["root"]["read_only"], false);
    assert_eq!(value["links"][0]["name"], "eth0");
    assert_eq!(value["log"]["stored_records"], 123);
}

#[test]
fn missing_values_are_null_not_zero() {
    let mut value = status(true);
    value.uptime_s = None;
    value.memory_total = None;
    value.memory_free = None;
    value.load = None;
    value.links.clear();
    let (exit, stdout, _) = run(&Fake(value), Format::Json);
    assert_eq!(exit, Exit::Success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert!(value["uptime_s"].is_null());
    assert!(value["memory"].is_null());
    assert!(value["load"].is_null());
    assert_eq!(value["links"], serde_json::json!([]));
}

#[test]
fn a_core_service_failure_exits_one() {
    let (exit, stdout, _) = run(&Fake(status(false)), Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap()["kind"],
        "status"
    );
}

#[test]
fn a_backend_error_is_reported_without_data() {
    let (exit, stdout, stderr) = run(&Broken, Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    let value: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(value["code"], "EIO");
}

#[test]
fn human_status_has_no_fake_defaults() {
    let (exit, stdout, _) = run(&Fake(status(true)), Format::Human);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains("services: 9/8/9"));
    assert!(stdout.contains("link: eth0 up"));
}
