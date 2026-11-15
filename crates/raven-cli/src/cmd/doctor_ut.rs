// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `raven doctor` with a fake backend.

use raven_output::{Exit, Format};

use super::{BackendError, Check, CheckStatus, DoctorBackend, run_with_backend};
use crate::Context;

struct Fake(Vec<Check>);

impl DoctorBackend for Fake {
    fn checks(&self) -> Result<Vec<Check>, BackendError> {
        Ok(self.0.clone())
    }
}

struct Broken;

impl DoctorBackend for Broken {
    fn checks(&self) -> Result<Vec<Check>, BackendError> {
        Err(BackendError::new("EIO", "backend failed"))
    }
}

fn check(id: &str, status: CheckStatus) -> Check {
    Check {
        id: id.to_string(),
        status,
        detail: format!("{id} detail"),
        fix: (status == CheckStatus::Warn).then(|| "fix it".to_string()),
    }
}

fn run(backend: &dyn DoctorBackend, strict: bool, format: Format) -> (Exit, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = {
        let mut context = Context::new(format, false, &mut stdout, &mut stderr);
        run_with_backend(&mut context, backend, strict)
    };
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn warnings_pass_without_strict() {
    let backend = Fake(vec![
        check("services", CheckStatus::Ok),
        check("clock", CheckStatus::Warn),
    ]);
    let (exit, stdout, _) = run(&backend, false, Format::Json);
    assert_eq!(exit, Exit::Success);
    assert_eq!(stdout.lines().count(), 2);
    let first: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(first["kind"], "check");
    assert_eq!(first["status"], "ok");
}

#[test]
fn warnings_fail_with_strict() {
    let backend = Fake(vec![check("clock", CheckStatus::Warn)]);
    let (exit, _, _) = run(&backend, true, Format::Json);
    assert_eq!(exit, Exit::Failure);
}

#[test]
fn a_failed_check_fails_without_strict() {
    let backend = Fake(vec![check("services", CheckStatus::Fail)]);
    let (exit, _, _) = run(&backend, false, Format::Json);
    assert_eq!(exit, Exit::Failure);
}

#[test]
fn a_backend_error_is_reported_without_checks() {
    let (exit, stdout, stderr) = run(&Broken, false, Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    let value: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(value["code"], "EIO");
}

#[test]
fn human_checks_show_the_fix_text() {
    let backend = Fake(vec![check("clock", CheckStatus::Warn)]);
    let (exit, stdout, _) = run(&backend, false, Format::Human);
    assert_eq!(exit, Exit::Success);
    assert!(stdout.contains("clock: warn clock detail"));
    assert!(stdout.contains("fix: fix it"));
}
