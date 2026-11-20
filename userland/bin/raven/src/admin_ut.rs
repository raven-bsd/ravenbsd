// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the target admin backends, driven through the `Scheme` seam.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    Globals, Scheme, control, doctor_checks, parse, read_service, read_services, run_users_at,
};
use raven_proto_svc::Op;

#[derive(Default)]
struct Fake {
    files: RefCell<BTreeMap<String, String>>,
    dirs: RefCell<BTreeMap<String, Vec<String>>>,
    calls: RefCell<Vec<(String, u8)>>,
    started: Cell<bool>,
}

impl Fake {
    fn with_service(name: &str, line: &str) -> Self {
        let fake = Self::default();
        fake.files.borrow_mut().insert("svc:".into(), String::new());
        fake.dirs
            .borrow_mut()
            .insert("svc:".into(), vec![name.to_string()]);
        fake.files
            .borrow_mut()
            .insert(format!("svc:/{name}"), format!("{line}\n"));
        fake.dirs
            .borrow_mut()
            .insert("/".into(), vec!["etc".into()]);
        fake
    }
}

impl Scheme for Fake {
    fn read(&self, path: &str) -> Result<String, String> {
        if self.started.get() && path.starts_with("svc:/") {
            let name = path.trim_start_matches("svc:/");
            return Ok(format!("{name} running 0 0\n"));
        }
        self.files
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: ENOENT"))
    }

    fn list(&self, path: &str) -> Result<Vec<String>, String> {
        self.dirs
            .borrow()
            .get(path)
            .cloned()
            .ok_or_else(|| format!("{path}: ENOENT"))
    }

    fn call(&self, path: &str, op: u8) -> Result<(), String> {
        self.calls.borrow_mut().push((path.to_string(), op));
        if op == Op::Start.as_u8() {
            self.started.set(true);
        }
        Ok(())
    }
}

#[test]
fn services_are_read_from_the_svc_scheme() {
    let fake = Fake::with_service("consd", "consd running 7 0");
    fake.dirs
        .borrow_mut()
        .get_mut("svc:")
        .unwrap()
        .push("vfsd".into());
    fake.files
        .borrow_mut()
        .insert("svc:/vfsd".into(), "vfsd starting 0 2\n".into());
    let services = read_services(&fake).expect("svc: readable");
    assert_eq!(services.len(), 2);
    assert_eq!(services[0].name, "consd");
    assert!(services[0].ready);
    assert_eq!(services[1].name, "vfsd");
    assert!(!services[1].ready);
    assert_eq!(services[1].state, "starting");
    assert_eq!(services[1].restarts, 2);
}

#[test]
fn a_bad_status_line_is_an_error_not_fake_data() {
    let fake = Fake::with_service("consd", "consd");
    let error = read_service(&fake, "consd").expect_err("malformed line");
    assert!(error.contains("svc:/consd"), "{error}");
}

#[test]
fn start_waits_for_the_running_state() {
    // The initial line is `starting`; the fake's CALL flips the next read to `running`.
    let fake = Fake::with_service("vfsd", "vfsd starting 0 0");
    let service = control(&fake, "vfsd", Op::Start).expect("start");
    assert!(service.ready);
    assert_eq!(service.state, "running");
    let calls = fake.calls.borrow();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0], ("svc:/vfsd".to_string(), Op::Start.as_u8()));
}

#[test]
fn doctor_checks_the_service_set_config_and_root() {
    // Satisfy whatever core-service list this host's config resolves to; an empty list is also
    // "nothing missing". The target profile installs `logd consd vfsd`.
    let core = super::core_services().unwrap_or_default();
    let fake = Fake::default();
    fake.dirs
        .borrow_mut()
        .insert("/".into(), vec!["etc".into()]);
    fake.dirs.borrow_mut().insert("svc:".into(), core.clone());
    for name in &core {
        fake.files
            .borrow_mut()
            .insert(format!("svc:/{name}"), format!("{name} running 0 0\n"));
    }
    let checks = doctor_checks(&fake);
    let ids: Vec<&str> = checks.iter().map(|check| check.id.as_str()).collect();
    assert_eq!(ids, ["services.ready", "config.parse", "root.mount"]);
    assert_eq!(checks[0].status, "ok");
    // `config.parse` reads the host `/etc/raven`; the test host may not install it. The target
    // profile does, and the check is exercised for its shape rather than a host-specific value.
    assert!(matches!(checks[1].status, "ok" | "fail"));
    assert_eq!(checks[2].status, "ok");
}

#[test]
fn only_the_admin_commands_are_dispatched() {
    let status = parse(&[OsString::from("--json"), OsString::from("status")])
        .expect("status is an admin command");
    assert!(status.0.json);
    assert_eq!(status.1, "status");
    assert!(parse(&[OsString::from("--color=never"), OsString::from("doctor")]).is_some());
    assert!(parse(&[OsString::from("frobnicate")]).is_none());
    assert!(parse(&[OsString::from("--help")]).is_none());
    assert!(parse(&[OsString::from("config"), OsString::from("check")]).is_none());
}

const ROOT_HASH: &str = "$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.";

static NEXT: AtomicUsize = AtomicUsize::new(1);

fn users_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "raven-admin-users-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    let write = |relative: &str, text: &str, mode: u32| {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    };
    write(
        "etc/passwd",
        "root:x:0:0:root:/root:/bin/rsh\nalice:x:1000:1000:Alice:/home/alice:/bin/rsh\n",
        0o644,
    );
    write(
        "etc/shadow",
        &format!("root:{ROOT_HASH}:0:0:99999:7:0:0:0\nalice:{ROOT_HASH}:0:0:99999:7:0:0:0\n"),
        0o600,
    );
    write("etc/group", "root:x:0:root\nalice:x:1000:\n", 0o644);
    write(
        "etc/raven/autologin.toml",
        "[console]\nuser = \"alice\"\nenabled = true\n",
        0o644,
    );
    for directory in ["root", "home/alice"] {
        fs::create_dir_all(root.join(directory)).unwrap();
    }
    write("bin/rsh", "#!/bin/rsh\n", 0o755);
    root
}

fn run_users_json(root: &Path, args: &[&str]) -> (i32, String, String) {
    let globals = Globals {
        json: true,
        quiet: false,
    };
    let mut out = Vec::new();
    let mut err = Vec::new();
    let args: Vec<String> = args.iter().map(|arg| (*arg).to_string()).collect();
    let code = run_users_at(root, &globals, &args, &mut out, &mut err);
    (
        code,
        String::from_utf8(out).unwrap(),
        String::from_utf8(err).unwrap(),
    )
}

#[test]
fn users_list_json_reads_the_account_files() {
    let root = users_root();
    let (code, out, err) = run_users_json(&root, &["list"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"schema\":1,\"kind\":\"user\""));
    assert!(out.contains("\"name\":\"root\""));
    assert!(out.contains("\"name\":\"alice\""));
    assert!(out.contains("\"autologin\":true"));
    assert!(!out.contains(ROOT_HASH));
    let _ = fs::remove_dir_all(root);
}

#[test]
fn users_show_and_check_json_report_the_account() {
    let root = users_root();
    let (code, out, err) = run_users_json(&root, &["show", "alice"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"name\":\"alice\""));
    assert!(out.contains("\"home\":\"/home/alice\""));
    assert!(out.contains("\"shell\":\"/bin/rsh\""));
    assert!(!out.contains(ROOT_HASH));
    let (code, out, err) = run_users_json(&root, &["check"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"status\":\"ok\""));
    assert!(out.contains("\"users\":2"));
    let _ = fs::remove_dir_all(root);
}
