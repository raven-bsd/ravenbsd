// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `raven svc` with a fake backend and a temporary enable file.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use raven_output::{Exit, Format};

use super::{BackendError, Service, SvcBackend, run_with_backend};
use crate::Context;

static NEXT: AtomicUsize = AtomicUsize::new(1);

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-svc-cmd-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, text: &str) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

struct Fake;

impl SvcBackend for Fake {
    fn list(&self) -> Result<Vec<Service>, BackendError> {
        Ok(vec![
            service("logd", "running", true, true),
            service("consd", "running", true, false),
        ])
    }
    fn status(&self, name: &str) -> Result<Service, BackendError> {
        Ok(service(name, "running", true, false))
    }
    fn start(&self, name: &str) -> Result<Service, BackendError> {
        Ok(service(name, "running", true, true))
    }
    fn stop(&self, name: &str) -> Result<Service, BackendError> {
        Ok(service(name, "stopped", false, true))
    }
    fn restart(&self, name: &str) -> Result<Service, BackendError> {
        Ok(service(name, "running", true, true))
    }
}

struct Broken;

impl SvcBackend for Broken {
    fn list(&self) -> Result<Vec<Service>, BackendError> {
        Err(BackendError::new("EIO", "control failed"))
    }
    fn status(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::new("EIO", "control failed"))
    }
    fn start(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::new("EIO", "control failed"))
    }
    fn stop(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::new("EIO", "control failed"))
    }
    fn restart(&self, _name: &str) -> Result<Service, BackendError> {
        Err(BackendError::new("EIO", "control failed"))
    }
}

fn service(name: &str, state: &str, ready: bool, changed: bool) -> Service {
    Service {
        name: name.to_string(),
        state: state.to_string(),
        ready,
        pid: Some(12),
        changed,
    }
}

fn run(
    root: &Path,
    backend: &dyn SvcBackend,
    args: &[&str],
    format: Format,
) -> (Exit, String, String) {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let exit = {
        let mut context = Context::new(format, false, &mut stdout, &mut stderr);
        run_with_backend(&mut context, root, backend, &args)
    };
    (
        exit,
        String::from_utf8(stdout).unwrap(),
        String::from_utf8(stderr).unwrap(),
    )
}

#[test]
fn list_emits_one_service_line_per_record() {
    let root = TempDir::new();
    let (exit, stdout, _) = run(root.path(), &Fake, &["list"], Format::Json);
    assert_eq!(exit, Exit::Success);
    assert_eq!(stdout.lines().count(), 2);
    let first: serde_json::Value = serde_json::from_str(stdout.lines().next().unwrap()).unwrap();
    assert_eq!(first["kind"], "service");
    assert_eq!(first["name"], "logd");
    assert_eq!(first["changed"], true);
}

#[test]
fn start_reports_changed() {
    let root = TempDir::new();
    let (exit, stdout, _) = run(root.path(), &Fake, &["start", "logd"], Format::Json);
    assert_eq!(exit, Exit::Success);
    let value: serde_json::Value = serde_json::from_str(stdout.trim()).unwrap();
    assert_eq!(value["name"], "logd");
    assert_eq!(value["state"], "running");
    assert_eq!(value["changed"], true);
}

#[test]
fn enable_and_disable_rewrite_the_sorted_file() {
    let root = TempDir::new();
    root.write("etc/services.d/a.toml", "name = \"a\"\nexec = \"/bin/a\"\n");
    root.write("etc/services.d/b.toml", "name = \"b\"\nexec = \"/bin/b\"\n");
    let (exit, stdout, _) = run(root.path(), &Fake, &["disable", "a"], Format::Json);
    assert_eq!(exit, Exit::Success);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(stdout.trim()).unwrap()["changed"],
        true
    );
    let text = fs::read_to_string(root.path().join("etc/raven/services.enable")).unwrap();
    assert_eq!(text, "b\n");
    let (exit, _, _) = run(root.path(), &Fake, &["disable", "a"], Format::Json);
    assert_eq!(exit, Exit::Success);
    let (exit, _, _) = run(root.path(), &Fake, &["enable", "a"], Format::Json);
    assert_eq!(exit, Exit::Success);
    let text = fs::read_to_string(root.path().join("etc/raven/services.enable")).unwrap();
    assert_eq!(text, "a\nb\n");
}

#[test]
fn a_backend_error_is_reported() {
    let root = TempDir::new();
    let (exit, stdout, stderr) = run(root.path(), &Broken, &["list"], Format::Json);
    assert_eq!(exit, Exit::Failure);
    assert!(stdout.is_empty());
    let value: serde_json::Value = serde_json::from_str(stderr.trim()).unwrap();
    assert_eq!(value["code"], "EIO");
}

#[test]
fn unknown_subcommand_is_usage() {
    let root = TempDir::new();
    let (exit, _, _) = run(root.path(), &Fake, &["wat"], Format::Human);
    assert_eq!(exit, Exit::Usage);
}
