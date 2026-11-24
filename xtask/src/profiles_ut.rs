// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for profile parsing, fragment merging and the release switch.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "raven-profile-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn write(&self, path: &str, text: &str) {
        let path = self.root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn profile(&self, name: &str, text: &str) {
        self.write(&format!("profiles/{name}.toml"), text);
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const BASE: &str = "name = \"base\"\n[initfs]\nprograms = [{ package = \"raven-init\", path = \"/init\" }]\n[boot]\ncmdline = \"console=ttyS0\"\nlog_level = \"info\"\n";

#[test]
fn fragments_merge_in_name_order() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/20-b.toml",
        "[[initfs.programs]]\npackage = \"b\"\npath = \"/b\"\n",
    );
    tree.write(
        "profiles/base.d/10-a.toml",
        "[[initfs.programs]]\npackage = \"a\"\npath = \"/a\"\n",
    );
    let names: Vec<_> = Profile::load(&tree.root, "base")
        .unwrap()
        .initfs
        .programs
        .into_iter()
        .map(|item| item.package)
        .collect();
    assert_eq!(names, ["raven-init", "a", "b"]);
}

#[test]
fn fragments_add_files_with_default_mode() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write("etc/a", "a");
    tree.write(
        "profiles/base.d/10-file.toml",
        "[[initfs.files]]\nsrc = \"etc/a\"\npath = \"/etc/a\"\n",
    );
    assert_eq!(
        Profile::load(&tree.root, "base").unwrap().initfs.files[0].mode,
        0o644
    );
}

#[test]
fn explicit_file_mode_is_preserved() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write("etc/a", "a");
    tree.write(
        "profiles/base.d/file.toml",
        "[[initfs.files]]\nsrc = \"etc/a\"\npath = \"/etc/a\"\nmode = 420\n",
    );
    assert_eq!(
        Profile::load(&tree.root, "base").unwrap().initfs.files[0].mode,
        0o644
    );
}

#[test]
fn duplicate_paths_name_both_sources() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/a.toml",
        "[[initfs.programs]]\npackage = \"a\"\npath = \"/same\"\n",
    );
    tree.write(
        "profiles/base.d/b.toml",
        "[[initfs.programs]]\npackage = \"b\"\npath = \"/same\"\n",
    );
    let error = Profile::load(&tree.root, "base").unwrap_err().to_string();
    assert!(error.contains("a.toml") && error.contains("b.toml"));
}

#[test]
fn unknown_fragment_keys_are_rejected() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write("profiles/base.d/bad.toml", "extra = true\n");
    assert!(Profile::load(&tree.root, "base").is_err());
}

#[test]
fn missing_file_source_is_rejected() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/file.toml",
        "[[initfs.files]]\nsrc = \"missing\"\npath = \"/missing\"\n",
    );
    assert!(Profile::load(&tree.root, "base").is_err());
}

#[test]
fn generated_file_source_may_be_missing() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/file.toml",
        "[[initfs.files]]\nsrc = \"build/fixtures/tool\"\npath = \"/tool\"\ngenerated = true\n",
    );
    let profile = Profile::load(&tree.root, "base").unwrap();
    assert!(profile.initfs.files[0].generated);
}

#[test]
fn generated_files_must_live_under_build() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write("etc/a", "a");
    tree.write(
        "profiles/base.d/file.toml",
        "[[initfs.files]]\nsrc = \"etc/a\"\npath = \"/a\"\ngenerated = true\n",
    );
    let error = Profile::load(&tree.root, "base").unwrap_err().to_string();
    assert!(error.contains("under build/"));
}

#[test]
fn external_bsp_generated_source_is_allowed() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/bsp.toml",
        "[[initfs.files]]\nsrc = \"../ravenbsd-rpi/build/aarch64/einkd\"\npath = \"/sbin/einkd\"\nmode = 493\ngenerated = true\n",
    );
    let profile = Profile::load(&tree.root, "base").unwrap();
    assert!(profile.initfs.files[0].generated);
}

#[test]
fn external_bsp_traversal_is_rejected() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/bsp.toml",
        "[[initfs.files]]\nsrc = \"../ravenbsd-rpi/build/../../etc/passwd\"\npath = \"/x\"\ngenerated = true\n",
    );
    let error = Profile::load(&tree.root, "base").unwrap_err().to_string();
    assert!(error.contains("not repo-relative"));
}

#[test]
fn regular_file_source_is_still_required() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/file.toml",
        "[[initfs.files]]\nsrc = \"build/fixtures/tool\"\npath = \"/tool\"\n",
    );
    assert!(Profile::load(&tree.root, "base").is_err());
}

#[test]
fn empty_package_is_rejected() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/bad.toml",
        "[[initfs.programs]]\npackage = \"\"\npath = \"/bad\"\n",
    );
    assert!(Profile::load(&tree.root, "base").is_err());
}

#[test]
fn child_inherits_parent_and_replaces_boot() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.profile("child", "name = \"child\"\nextends = \"base\"\n[initfs]\nprograms = [{ package = \"child\", path = \"/child\" }]\n[boot]\ncmdline = \"child\"\nlog_level = \"debug\"\n");
    let child = Profile::load(&tree.root, "child").unwrap();
    assert_eq!(child.initfs.programs.len(), 2);
    assert_eq!(child.boot.cmdline, "child");
}

#[test]
fn child_inherits_parent_fragments() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/a.toml",
        "[[initfs.programs]]\npackage = \"a\"\npath = \"/a\"\n",
    );
    tree.profile(
        "child",
        "name = \"child\"\nextends = \"base\"\n[boot]\ncmdline = \"child\"\nlog_level = \"info\"\n",
    );
    assert_eq!(
        Profile::load(&tree.root, "child")
            .unwrap()
            .initfs
            .programs
            .len(),
        2
    );
}

#[test]
fn missing_parent_is_rejected() {
    let tree = Tree::new();
    tree.profile(
        "child",
        "name = \"child\"\nextends = \"missing\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    assert!(Profile::load(&tree.root, "child").is_err());
}

#[test]
fn cycles_are_rejected() {
    let tree = Tree::new();
    tree.profile(
        "a",
        "name = \"a\"\nextends = \"b\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    tree.profile(
        "b",
        "name = \"b\"\nextends = \"a\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    assert!(format!("{:#}", Profile::load(&tree.root, "a").unwrap_err()).contains("cycle"));
}

#[test]
fn inheritance_is_limited_to_one_level() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.profile(
        "middle",
        "name = \"middle\"\nextends = \"base\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    tree.profile(
        "child",
        "name = \"child\"\nextends = \"middle\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    assert!(format!("{:#}", Profile::load(&tree.root, "child").unwrap_err()).contains("one-level"));
}

#[test]
fn shell_profile_parses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert_eq!(Profile::load(root, "shell").unwrap().name, "shell");
}

#[test]
fn release_defaults_to_false() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    assert!(!Profile::load(&tree.root, "base").unwrap().initfs.release);
}

#[test]
fn a_fragment_can_enable_release() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.write(
        "profiles/base.d/10-release.toml",
        "[initfs]\nrelease = true\n",
    );
    assert!(Profile::load(&tree.root, "base").unwrap().initfs.release);
}

#[test]
fn a_parent_enables_release_for_its_child() {
    let tree = Tree::new();
    tree.profile(
        "base",
        "name = \"base\"\n[initfs]\nrelease = true\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    tree.profile(
        "child",
        "name = \"child\"\nextends = \"base\"\n[boot]\ncmdline = \"child\"\nlog_level = \"info\"\n",
    );
    assert!(Profile::load(&tree.root, "child").unwrap().initfs.release);
}

#[test]
fn release_is_additive_and_a_child_false_does_not_disable_it() {
    let tree = Tree::new();
    tree.profile(
        "base",
        "name = \"base\"\n[initfs]\nrelease = true\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n",
    );
    tree.profile("child", "name = \"child\"\nextends = \"base\"\n[initfs]\nrelease = false\n[boot]\ncmdline = \"child\"\nlog_level = \"info\"\n");
    assert!(Profile::load(&tree.root, "child").unwrap().initfs.release);
}

#[test]
fn the_release_profile_enables_release() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    assert!(Profile::load(root, "release").unwrap().initfs.release);
    assert!(!Profile::load(root, "base").unwrap().initfs.release);
    assert!(!Profile::load(root, "shell").unwrap().initfs.release);
}

fn resident_service(tree: &Tree, name: &str, body: &str) {
    tree.write(
        &format!("etc/services.d/{name}.toml"),
        &format!("name = \"{name}\"\nexec = \"/sbin/{name}\"\n{body}"),
    );
}

const RESIDENT: &str = "readiness = \"scheme\"\nrestart = \"on-failure\"\n";

fn shipped(tree: &Tree, order: &str, name: &str) {
    resident_service(tree, name, RESIDENT);
    tree.write(
        &format!("profiles/shell.d/{order}-{name}.toml"),
        &format!(
            "[[initfs.files]]\nsrc = \"etc/services.d/{name}.toml\"\n\
             path = \"/etc/services.d/{name}.toml\"\n"
        ),
    );
}

fn one_shot_profile(tree: &Tree, name: &str, pins: &[&str]) {
    tree.write(
        &format!("profiles/{name}.toml"),
        &format!("name = \"{name}\"\nextends = \"shell\"\n"),
    );
    tree.write(
        &format!("profiles/{name}.d/30-rc-conf.toml"),
        &format!(
            "[[initfs.files]]\nsrc = \"profiles/{name}.d/rc.conf\"\npath = \"/etc/rc.conf\"\n"
        ),
    );
    let mut text = String::new();
    for service in pins {
        text.push_str(&format!("{service}_restart = \"never\"\n"));
    }
    tree.write(&format!("profiles/{name}.d/rc.conf"), &text);
}

fn case(tree: &Tree, name: &str, profile: &str, expect_exit: &str) {
    tree.write(
        &format!("tests/qtest/cases/{name}.toml"),
        &format!(
            "name = \"{name}\"\narch = \"x86_64\"\nprofile = \"{profile}\"\n\
             timeout_secs = 60\nexpect_exit = \"{expect_exit}\"\n"
        ),
    );
}

/// A miniature repository: the shell profile ships logd, ramfsd and initfsd; three one-shot
/// profiles extend it and pin everything.
fn fixture() -> Tree {
    let tree = Tree::new();
    tree.write("profiles/shell.toml", "name = \"shell\"\n");
    shipped(&tree, "20", "logd");
    shipped(&tree, "30", "initfsd");
    shipped(&tree, "31", "ramfsd");
    one_shot_profile(&tree, "core-test", &["logd", "initfsd", "ramfsd"]);
    one_shot_profile(&tree, "ramfsd-test", &["logd", "initfsd", "ramfsd"]);
    one_shot_profile(&tree, "initfsd-test", &["logd", "initfsd", "ramfsd"]);
    case(&tree, "core-servers", "core-test", "success");
    case(&tree, "ramfsd", "ramfsd-test", "success");
    case(&tree, "initfsd", "initfsd-test", "success");
    tree
}

/// Run the gate and echo every violation, so a `--nocapture` run shows the exact message.
fn violations(tree: &Tree) -> Vec<String> {
    let violations = one_shot_restart_violations(&tree.root).unwrap();
    for violation in &violations {
        println!("{violation}");
    }
    violations
}

#[test]
fn the_repository_is_covered_by_the_gate() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let profiles = one_shot_shell_profiles(root).unwrap();
    for profile in ["blockd-test", "initfsd-test", "ramfsd-test"] {
        assert!(profiles.contains(&profile.to_string()), "{profiles:?}");
    }
    let services = shell_services(root).unwrap();
    for service in [
        "initfsd", "logd", "nulld", "ramfsd", "randd", "sysd", "zerod",
    ] {
        assert!(
            services.iter().any(|item| item.name == service),
            "the gate must see `{service}`: {services:?}"
        );
    }
    assert!(
        services.iter().all(|item| item.long_running),
        "every shell service is resident today: {services:?}"
    );
    assert_eq!(
        one_shot_restart_violations(root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn complete_one_shot_profiles_pass() {
    let tree = fixture();
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn missing_ramfsd_pin_fails_and_names_ramfsd() {
    let tree = fixture();
    tree.write(
        "profiles/core-test.d/rc.conf",
        "logd_restart = \"never\"\ninitfsd_restart = \"never\"\n",
    );
    let violations = violations(&tree);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(
        violations[0].contains("profiles/core-test.d/rc.conf"),
        "the edit location must be named: {}",
        violations[0]
    );
    assert!(violations[0].contains("ramfsd_restart = \"never\""));
    assert!(violations[0].contains("etc/services.d/ramfsd.toml"));
    assert!(!violations[0].contains("logd"));
}

#[test]
fn missing_initfsd_pin_fails_and_names_initfsd() {
    let tree = fixture();
    tree.write(
        "profiles/ramfsd-test.d/rc.conf",
        "logd_restart = \"never\"\nramfsd_restart = \"never\"\n",
    );
    let violations = violations(&tree);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert!(violations[0].contains("profiles/ramfsd-test.d/rc.conf"));
    assert!(violations[0].contains("initfsd_restart = \"never\""));
}

#[test]
fn a_new_resident_shell_service_fails_every_one_shot_profile() {
    let tree = fixture();
    resident_service(&tree, "vfsd", RESIDENT);
    tree.write(
        "profiles/shell.d/40-vfsd.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/vfsd.toml\"\n\
         path = \"/etc/services.d/vfsd.toml\"\n",
    );
    let violations = violations(&tree);
    assert_eq!(violations.len(), 3, "{violations:?}");
    for (violation, profile) in violations
        .iter()
        .zip(["core-test", "initfsd-test", "ramfsd-test"])
    {
        assert!(
            violation.contains(&format!("profile `{profile}`")),
            "{violation}"
        );
        assert!(violation.contains("vfsd_restart = \"never\""));
    }
}

#[test]
fn an_allowlist_entry_exempts_a_service() {
    let tree = fixture();
    resident_service(&tree, "vfsd", RESIDENT);
    tree.write(
        "profiles/shell.d/40-vfsd.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/vfsd.toml\"\n\
         path = \"/etc/services.d/vfsd.toml\"\n",
    );
    tree.write(
        "tests/xtask/profile-restart-allow.toml",
        "[[allow]]\nservice = \"vfsd\"\nreason = \"vfsd shuts itself down in test mode\"\n",
    );
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn an_allowlist_entry_without_a_reason_is_rejected() {
    let tree = fixture();
    tree.write(
        "tests/xtask/profile-restart-allow.toml",
        "[[allow]]\nservice = \"vfsd\"\nreason = \"  \"\n",
    );
    let error = format!("{:#}", one_shot_restart_violations(&tree.root).unwrap_err());
    assert!(error.contains("needs a reason"), "{error}");
}

#[test]
fn an_allowlist_entry_can_be_scoped_to_profiles() {
    let tree = fixture();
    resident_service(&tree, "vfsd", RESIDENT);
    tree.write(
        "profiles/shell.d/40-vfsd.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/vfsd.toml\"\n\
         path = \"/etc/services.d/vfsd.toml\"\n",
    );
    tree.write(
        "tests/xtask/profile-restart-allow.toml",
        "[[allow]]\nservice = \"vfsd\"\nreason = \"only core-test needs it up\"\n\
         profiles = [\"core-test\"]\n",
    );
    let violations = one_shot_restart_violations(&tree.root).unwrap();
    assert_eq!(violations.len(), 2, "{violations:?}");
    assert!(violations.iter().all(|item| !item.contains("core-test")));
}

#[test]
fn one_shot_markers_are_not_required_to_pin() {
    let tree = fixture();
    // `readiness = "exit"` promises the process terminates; `enable = false` never starts it.
    resident_service(&tree, "probe", "readiness = \"exit\"\n");
    tree.write(
        "profiles/shell.d/50-probe.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/probe.toml\"\n\
         path = \"/etc/services.d/probe.toml\"\n",
    );
    resident_service(&tree, "opt-in", "enable = false\n");
    tree.write(
        "profiles/shell.d/60-opt-in.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/opt-in.toml\"\n\
         path = \"/etc/services.d/opt-in.toml\"\n",
    );
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn a_timeout_case_does_not_make_its_profile_one_shot() {
    let tree = Tree::new();
    tree.write("profiles/shell.toml", "name = \"shell\"\n");
    shipped(&tree, "20", "logd");
    one_shot_profile(&tree, "server-test", &[]);
    case(&tree, "server", "server-test", "timeout");
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn a_manifest_that_is_already_never_does_not_need_a_pin() {
    let tree = fixture();
    resident_service(&tree, "oneshot-ish", "restart = \"never\"\n");
    tree.write(
        "profiles/shell.d/70-oneshot-ish.toml",
        "[[initfs.files]]\nsrc = \"etc/services.d/oneshot-ish.toml\"\n\
         path = \"/etc/services.d/oneshot-ish.toml\"\n",
    );
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn a_disabled_service_needs_no_pin() {
    let tree = fixture();
    tree.write(
        "profiles/core-test.d/rc.conf",
        "logd_restart = \"never\"\ninitfsd_restart = \"never\"\nramfsd_enable = \"NO\"\n",
    );
    assert_eq!(
        one_shot_restart_violations(&tree.root).unwrap(),
        Vec::<String>::new()
    );
}

#[test]
fn image_release_is_additive_across_extends_and_fragments() {
    let tree = Tree::new();
    tree.profile("base", BASE);
    tree.profile(
        "demo",
        "name = \"demo\"\nextends = \"base\"\n[image]\nrelease = true\n[boot]\ncmdline = \"console=ttyS0\"\nlog_level = \"info\"\n",
    );
    assert!(Profile::load(&tree.root, "demo").unwrap().image.release);
    assert!(!Profile::load(&tree.root, "base").unwrap().image.release);
    tree.write("profiles/base.d/30-image.toml", "[image]\nrelease = true\n");
    assert!(Profile::load(&tree.root, "base").unwrap().image.release);
}
