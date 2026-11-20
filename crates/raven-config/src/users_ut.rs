// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the files-backed Users model, password hashing and atomic apply.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{
    Accounts, Autologin, CheckReport, UserAction, UserSpec, apply, check, parse_autologin,
    render_autologin, render_plan,
};

static NEXT: AtomicUsize = AtomicUsize::new(1);

const ROOT_HASH: &str = "$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.";

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "raven-config-users-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).unwrap();
        let root = Self { path };
        root.write("etc/passwd", "root:x:0:0:root:/root:/bin/rsh\n", 0o644);
        root.write(
            "etc/shadow",
            &format!("root:{ROOT_HASH}:0:0:99999:7:0:0:0\n"),
            0o600,
        );
        root.write("etc/group", "root:x:0:root\n", 0o644);
        root.write_exec("bin/rsh");
        root.write_exec("bin/tui-demo");
        fs::create_dir_all(root.path.join("root")).unwrap();
        root
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative: &str, text: &str, mode: u32) {
        let path = self.path.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn write_exec(&self, relative: &str) {
        self.write(relative, "#!/bin/rsh\n", 0o755);
    }

    fn read(&self, relative: &str) -> String {
        fs::read_to_string(self.path.join(relative)).unwrap()
    }

    fn exists(&self, relative: &str) -> bool {
        self.path.join(relative).exists()
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn spec(name: &str, uid: u32, gid: u32, shell: &str, password: Option<&str>) -> UserSpec {
    UserSpec {
        name: name.to_string(),
        uid,
        gid,
        home: String::new(),
        shell: shell.to_string(),
        password: password.map(str::to_string),
        autologin: false,
    }
}

fn add(root: &TempRoot, name: &str, uid: u32, password: &str) -> super::ApplyOutcome {
    apply(
        root.path(),
        &UserAction::Add(spec(name, uid, uid, "/bin/tui-demo", Some(password))),
    )
    .unwrap()
}

#[test]
fn load_reads_the_base_account_files() {
    let root = TempRoot::new();
    let accounts = Accounts::load(root.path()).unwrap();
    assert_eq!(accounts.users.len(), 1);
    assert_eq!(accounts.users[0].name, "root");
    assert_eq!(accounts.users[0].uid, 0);
    assert_eq!(accounts.shadows.len(), 1);
    assert!(accounts.shadows[0].has_hash());
    assert_eq!(accounts.groups[0].gid, 0);
    assert!(!accounts.autologin.enabled);
}

#[test]
fn empty_inactive_and_expire_parse_as_minus_one() {
    let root = TempRoot::new();
    let hash = "$6$rounds=5000$G3aRZtaEHFI9mxqP$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.";
    root.write(
        "etc/shadow",
        &format!("root:{hash}:20000:0:99999:7:::\nalice:{hash}:19000:0:99999:7:-1:-1:0\n"),
        0o600,
    );
    let accounts = Accounts::load(root.path()).unwrap();
    let root_entry = accounts.shadow("root").unwrap();
    assert_eq!(root_entry.inactive, -1);
    assert_eq!(root_entry.expire, -1);
    let alice = accounts.shadow("alice").unwrap();
    assert_eq!(alice.inactive, -1);
    assert_eq!(alice.expire, -1);
    assert_eq!(
        alice.render(),
        format!("alice:{hash}:19000:0:99999:7:-1:-1:0")
    );
}

#[test]
fn add_user_writes_passwd_shadow_group_home_and_autologin() {
    let root = TempRoot::new();
    let mut request = spec("musicbox", 1000, 1000, "/bin/tui-demo", Some("tui-pass"));
    request.autologin = true;
    let outcome = apply(root.path(), &UserAction::Add(request)).unwrap();

    let passwd = root.read("etc/passwd");
    assert!(passwd.contains("musicbox:x:1000:1000::/home/musicbox:/bin/tui-demo\n"));
    assert!(passwd.contains("root:x:0:0:root:/root:/bin/rsh\n"));

    let shadow = root.read("etc/shadow");
    let line = shadow
        .lines()
        .find(|line| line.starts_with("musicbox:"))
        .unwrap();
    let hash = line.split(':').nth(1).unwrap();
    assert!(hash.starts_with("$6$"));
    assert!(raven_auth::verify_password("tui-pass", hash));
    assert!(line.ends_with(":0:99999:7:-1:-1:0"));
    assert!(!shadow.contains("tui-pass"));

    let group = root.read("etc/group");
    assert!(group.contains("musicbox:x:1000:\n"));
    assert!(group.contains("root:x:0:root\n"));

    let home = root.path().join("home/musicbox");
    assert!(home.is_dir());
    let metadata = fs::metadata(&home).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o755);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(metadata.uid(), 1000);
        assert_eq!(metadata.gid(), 1000);
    }

    assert_eq!(
        root.read("etc/raven/autologin.toml"),
        "# Managed by raven config (K229).\n[console]\nuser = \"musicbox\"\nenabled = true\n"
    );
    assert_eq!(outcome.home.as_deref(), Some(home.as_path()));
}

#[test]
fn password_change_does_not_touch_unrelated_records() {
    let root = TempRoot::new();
    add(&root, "alice", 1000, "alice-pass");
    add(&root, "musicbox", 1001, "first-pass");
    let alice_passwd = root
        .read("etc/passwd")
        .lines()
        .find(|line| line.starts_with("alice:"))
        .unwrap()
        .to_string();
    let alice_shadow = root
        .read("etc/shadow")
        .lines()
        .find(|line| line.starts_with("alice:"))
        .unwrap()
        .to_string();
    let old_hash = root
        .read("etc/shadow")
        .lines()
        .find(|line| line.starts_with("musicbox:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap()
        .to_string();

    let mut edit = spec("musicbox", 1001, 1001, "/bin/tui-demo", Some("second-pass"));
    edit.home = String::new();
    apply(
        root.path(),
        &UserAction::Edit {
            name: "musicbox".to_string(),
            spec: edit,
        },
    )
    .unwrap();

    assert_eq!(
        root.read("etc/passwd")
            .lines()
            .find(|line| line.starts_with("alice:"))
            .unwrap(),
        alice_passwd
    );
    assert_eq!(
        root.read("etc/shadow")
            .lines()
            .find(|line| line.starts_with("alice:"))
            .unwrap(),
        alice_shadow
    );
    let shadow = root.read("etc/shadow");
    let new_hash = shadow
        .lines()
        .find(|line| line.starts_with("musicbox:"))
        .unwrap()
        .split(':')
        .nth(1)
        .unwrap();
    assert_ne!(old_hash, new_hash);
    assert!(raven_auth::verify_password("second-pass", new_hash));
    assert!(!raven_auth::verify_password("first-pass", new_hash));
}

#[test]
fn dry_run_plan_never_prints_the_password() {
    let action = UserAction::Add(spec(
        "musicbox",
        1000,
        1000,
        "/bin/tui-demo",
        Some("secret-value"),
    ));
    let plan = render_plan(&action);
    assert!(plan.contains("action = \"add\""));
    assert!(plan.contains("password = \"<set>\""));
    assert!(!plan.contains("secret-value"));
}

#[test]
fn duplicate_name_and_uid_are_rejected() {
    let root = TempRoot::new();
    add(&root, "alice", 1000, "alice-pass");
    let duplicate_name = apply(
        root.path(),
        &UserAction::Add(spec("alice", 1001, 1001, "/bin/tui-demo", Some("other"))),
    );
    assert!(duplicate_name.is_err());
    let duplicate_uid = apply(
        root.path(),
        &UserAction::Add(spec("bob", 1000, 1001, "/bin/tui-demo", Some("other"))),
    );
    assert!(duplicate_uid.is_err());
}

#[test]
fn root_cannot_be_deleted_or_reassigned() {
    let root = TempRoot::new();
    let delete = apply(
        root.path(),
        &UserAction::Delete {
            name: "root".to_string(),
        },
    );
    assert!(delete.is_err());
    let mut edit = spec("root", 1000, 0, "/bin/rsh", None);
    edit.home = "/root".to_string();
    let reassign = apply(
        root.path(),
        &UserAction::Edit {
            name: "root".to_string(),
            spec: edit,
        },
    );
    assert!(reassign.is_err());
    assert!(
        root.read("etc/passwd")
            .contains("root:x:0:0:root:/root:/bin/rsh")
    );
}

#[test]
fn missing_or_non_executable_shell_is_rejected() {
    let root = TempRoot::new();
    let missing = apply(
        root.path(),
        &UserAction::Add(spec("musicbox", 1000, 1000, "/bin/missing", Some("pass"))),
    );
    assert!(missing.is_err());
    root.write("bin/plain", "not executable\n", 0o644);
    let not_executable = apply(
        root.path(),
        &UserAction::Add(spec("musicbox", 1000, 1000, "/bin/plain", Some("pass"))),
    );
    assert!(not_executable.is_err());
    assert!(!root.exists("etc/passwd.tmp"));
}

#[test]
fn delete_removes_the_user_group_shadow_and_autologin() {
    let root = TempRoot::new();
    let mut request = spec("musicbox", 1000, 1000, "/bin/tui-demo", Some("pass"));
    request.autologin = true;
    apply(root.path(), &UserAction::Add(request)).unwrap();
    apply(
        root.path(),
        &UserAction::Delete {
            name: "musicbox".to_string(),
        },
    )
    .unwrap();
    assert!(!root.read("etc/passwd").contains("musicbox:"));
    assert!(!root.read("etc/shadow").contains("musicbox:"));
    assert!(!root.read("etc/group").contains("musicbox:"));
    assert!(!Accounts::load(root.path()).unwrap().autologin.enabled);
}

#[test]
fn check_reports_a_clean_database_and_file_modes() {
    let root = TempRoot::new();
    add(&root, "musicbox", 1000, "pass");
    let report = check(root.path()).unwrap();
    assert!(report.ok(), "{report:?}");
    assert_eq!(report.users, 2);
    assert_eq!(report.groups, 2);
}

#[test]
fn check_reports_a_bad_shadow_mode() {
    let root = TempRoot::new();
    fs::set_permissions(
        root.path().join("etc/shadow"),
        fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let report: CheckReport = check(root.path()).unwrap();
    assert!(!report.ok());
    assert!(report.errors.iter().any(|error| error.contains("shadow")));
}

#[test]
fn autologin_round_trips_and_malformed_input_is_disabled() {
    let autologin = Autologin {
        enabled: true,
        user: "musicbox".to_string(),
    };
    let text = render_autologin(&autologin);
    assert_eq!(parse_autologin(&text), autologin);
    assert_eq!(
        parse_autologin("[console]\nuser = \"musicbox\"\nenabled = false\n"),
        Autologin {
            enabled: false,
            user: "musicbox".to_string(),
        }
    );
    assert_eq!(
        parse_autologin("user = \"bad:name\"\nenabled = true\n"),
        Autologin::default()
    );
}

#[test]
fn a_locked_database_is_reported_busy() {
    let root = TempRoot::new();
    fs::write(root.path().join("etc/.pwd.lock"), "other\n").unwrap();
    let result = apply(
        root.path(),
        &UserAction::Add(spec("musicbox", 1000, 1000, "/bin/tui-demo", Some("pass"))),
    );
    assert!(matches!(result, Err(super::UserError::Busy)));
    assert!(!root.read("etc/passwd").contains("musicbox:"));
}
