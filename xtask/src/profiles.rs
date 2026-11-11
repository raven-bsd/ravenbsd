// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Profile parsing and deterministic fragment merging (`profiles/<name>.d/*.toml`).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component as PathComponent, Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

/// A fully merged build profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Profile {
    pub(crate) name: String,
    pub(crate) initfs: Initfs,
    pub(crate) boot: Boot,
}

/// Initial filesystem entries.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Initfs {
    #[serde(default)]
    pub(crate) programs: Vec<Program>,
    #[serde(default)]
    pub(crate) files: Vec<File>,
}

/// One program entry in a profile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Program {
    pub(crate) package: String,
    pub(crate) path: String,
}

/// One repository file copied into the initial filesystem.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct File {
    pub(crate) src: String,
    pub(crate) path: String,
    #[serde(default = "default_file_mode")]
    pub(crate) mode: u32,
    /// The source is a generated build artifact under `build/`; profile validation does not
    /// require it to exist yet. The image build still reads it, after the ticket's pre step.
    #[serde(default)]
    pub(crate) generated: bool,
}

const fn default_file_mode() -> u32 {
    0o644
}

/// Boot parameters stored in a profile.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Boot {
    pub(crate) cmdline: String,
    pub(crate) log_level: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileFile {
    name: String,
    #[serde(default)]
    extends: Option<String>,
    #[serde(default)]
    initfs: Initfs,
    boot: Option<Boot>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fragment {
    #[serde(default)]
    initfs: Initfs,
}

impl Profile {
    /// Load `profiles/<name>.toml`, its optional parent and its sorted fragments.
    pub(crate) fn load(root: &Path, name: &str) -> Result<Self> {
        let mut stack = Vec::new();
        load_profile(root, name, &mut stack)
    }
}

fn load_profile(root: &Path, name: &str, stack: &mut Vec<String>) -> Result<Profile> {
    if !valid_name(name) {
        bail!("invalid profile name `{name}`");
    }
    if stack.iter().any(|item| item == name) {
        stack.push(name.to_string());
        bail!("profile extends cycle: {}", stack.join(" -> "));
    }
    stack.push(name.to_string());
    let path = root.join("profiles").join(format!("{name}.toml"));
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read profile {}", path.display()))?;
    let parsed: ProfileFile = toml::from_str(&text)
        .with_context(|| format!("cannot parse profile {}", path.display()))?;
    if parsed.name != name {
        bail!("profile file `{name}.toml` declares name `{}`", parsed.name);
    }

    let mut entries = if let Some(parent_name) = &parsed.extends {
        if !valid_name(parent_name) {
            bail!("profile `{name}` has invalid parent `{parent_name}`");
        }
        if stack.iter().any(|item| item == parent_name) {
            let mut cycle = stack.clone();
            cycle.push(parent_name.clone());
            bail!("profile extends cycle: {}", cycle.join(" -> "));
        }
        if stack.len() > 1 {
            bail!("profile `{name}` extends `{parent_name}` beyond the one-level limit");
        }
        let parent = load_profile(root, parent_name, stack)
            .with_context(|| format!("profile `{name}` cannot extend `{parent_name}`"))?;
        EntrySet::from_profile(parent, format!("profiles/{parent_name}.toml"))
    } else {
        EntrySet::default()
    };
    stack.pop();
    entries.add(parsed.initfs, relative(&path, root))?;
    for fragment_path in fragment_paths(root, name)? {
        let text = fs::read_to_string(&fragment_path)
            .with_context(|| format!("cannot read fragment {}", fragment_path.display()))?;
        let fragment: Fragment = toml::from_str(&text)
            .with_context(|| format!("cannot parse fragment {}", fragment_path.display()))?;
        entries.add(fragment.initfs, relative(&fragment_path, root))?;
    }
    let boot = parsed
        .boot
        .ok_or_else(|| anyhow::anyhow!("profile `{name}` has no [boot] section"))?;
    validate_boot(name, &boot)?;
    entries.validate_files(root, name)?;
    Ok(Profile {
        name: name.to_string(),
        initfs: entries.into_initfs(),
        boot,
    })
}

#[derive(Default)]
struct EntrySet {
    programs: Vec<Program>,
    files: Vec<File>,
    paths: BTreeMap<String, String>,
}

impl EntrySet {
    fn from_profile(profile: Profile, source: String) -> Self {
        let mut entries = Self::default();
        for program in profile.initfs.programs {
            entries.paths.insert(program.path.clone(), source.clone());
            entries.programs.push(program);
        }
        for file in profile.initfs.files {
            entries.paths.insert(file.path.clone(), source.clone());
            entries.files.push(file);
        }
        entries
    }

    fn add(&mut self, initfs: Initfs, source: String) -> Result<()> {
        for program in initfs.programs {
            if program.package.is_empty() {
                bail!("{source}: initfs program has an empty package");
            }
            self.insert_path(&program.path, &source)?;
            self.programs.push(program);
        }
        for file in initfs.files {
            self.insert_path(&file.path, &source)?;
            self.files.push(file);
        }
        Ok(())
    }

    fn insert_path(&mut self, path: &str, source: &str) -> Result<()> {
        validate_initfs_path(path, source)?;
        if let Some(previous) = self.paths.get(path) {
            bail!("duplicate initfs path `{path}` in {previous} and {source}");
        }
        self.paths.insert(path.to_string(), source.to_string());
        Ok(())
    }

    fn validate_files(&self, root: &Path, name: &str) -> Result<()> {
        for file in &self.files {
            let source = Path::new(&file.src);
            if source.is_absolute()
                || source
                    .components()
                    .any(|part| matches!(part, PathComponent::ParentDir))
            {
                bail!(
                    "profile `{name}` file source `{}` is not repo-relative",
                    file.src
                );
            }
            if file.generated && !source.starts_with("build") {
                bail!(
                    "profile `{name}` generated file source `{}` is not under build/",
                    file.src
                );
            }
            if file.src.is_empty() || (!file.generated && !root.join(source).is_file()) {
                bail!("profile `{name}` file source `{}` does not exist", file.src);
            }
        }
        Ok(())
    }

    fn into_initfs(self) -> Initfs {
        Initfs {
            programs: self.programs,
            files: self.files,
        }
    }
}

fn validate_initfs_path(path: &str, source: &str) -> Result<()> {
    if !path.starts_with('/') || path.len() < 2 {
        bail!("{source}: initfs path `{path}` is not absolute");
    }
    Ok(())
}

fn validate_boot(name: &str, boot: &Boot) -> Result<()> {
    if !matches!(boot.log_level.as_str(), "error" | "warn" | "info" | "debug") {
        bail!(
            "profile `{name}` has invalid log_level `{}`",
            boot.log_level
        );
    }
    if boot.cmdline.contains('\0') {
        bail!("profile `{name}` has a NUL in cmdline");
    }
    Ok(())
}

fn fragment_paths(root: &Path, name: &str) -> Result<Vec<PathBuf>> {
    let directory = root.join("profiles").join(format!("{name}.d"));
    if !directory.is_dir() {
        return Ok(Vec::new());
    }
    let mut paths = Vec::new();
    for entry in
        fs::read_dir(&directory).with_context(|| format!("cannot list {}", directory.display()))?
    {
        let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
        let path = entry.path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("toml")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

fn relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '_')
}

/// Repository-relative allowlist of services that may keep a restartable policy in a one-shot
/// test profile. Every entry needs a reason; the lead reviews the file like any other code.
pub(crate) const RESTART_ALLOWLIST: &str = "tests/xtask/profile-restart-allow.toml";

/// One service shipped by the `shell` profile.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ShellService {
    /// Service name from its manifest.
    pub(crate) name: String,
    /// Repository-relative manifest path.
    pub(crate) manifest: String,
    /// The manifest would restart the service if it exits.
    pub(crate) may_restart: bool,
    /// The service stays resident after readiness.
    pub(crate) long_running: bool,
}

/// Why a one-shot profile must pin a service to `restart = "never"`.
///
/// The rule lives here and nowhere else: `svcd` in `exit_when_settled` mode only waits for
/// services that can be restarted (`userland/init/src/worker.rs`, `should_wait_for_exit`), so a
/// resident service whose policy is `on-failure` keeps the case from ever reaching
/// `init exited (0)`. The one-shot marker is the manifest's own promise that the process
/// terminates: `readiness = "exit"` (ready when it exits) or `enable = false` (never started).
/// Every other service is treated as resident.
pub(crate) fn one_shot_restart_violations(root: &Path) -> Result<Vec<String>> {
    let services = shell_services(root)?;
    let allowlist = restart_allowlist(root)?;
    let mut violations = Vec::new();
    for profile in one_shot_shell_profiles(root)? {
        let target = rc_conf_target(root, &profile)?;
        let overrides = read_overrides(&target.text);
        for service in &services {
            if !service.long_running || !service.may_restart {
                continue;
            }
            if allowlist.permits(&profile, &service.name) || overrides.disables(&service.name) {
                continue;
            }
            if overrides.restarts(&service.name) == Some("never") {
                continue;
            }
            violations.push(format!(
                "{}: profile `{profile}` must set `{}_restart = \"never\"` for the resident \
                 service `{}` from {}",
                target.path, service.name, service.name, service.manifest
            ));
        }
    }
    Ok(violations)
}

/// Resident-or-one-shot status of every service the `shell` profile installs.
///
/// The services are discovered from `profiles/shell.d/*.toml` (the fragments the profile
/// inherits), not from a hard-coded list: a fragment that adds `/etc/services.d/<name>.toml`
/// contributes that manifest.
pub(crate) fn shell_services(root: &Path) -> Result<Vec<ShellService>> {
    let mut services = Vec::new();
    for fragment in fragment_paths(root, "shell")? {
        let text = fs::read_to_string(&fragment)
            .with_context(|| format!("cannot read fragment {}", fragment.display()))?;
        let parsed: toml::Value = toml::from_str(&text)
            .with_context(|| format!("cannot parse fragment {}", fragment.display()))?;
        let entries = parsed
            .get("initfs")
            .and_then(|initfs| initfs.get("files"))
            .and_then(toml::Value::as_array);
        let Some(entries) = entries else {
            continue;
        };
        for entry in entries {
            let (Some(source), Some(destination)) = (
                entry.get("src").and_then(toml::Value::as_str),
                entry.get("path").and_then(toml::Value::as_str),
            ) else {
                continue;
            };
            if !is_service_manifest(destination) {
                continue;
            }
            services.push(read_service_manifest(root, source)?);
        }
    }
    services.sort_by(|left, right| left.name.cmp(&right.name));
    services.dedup_by(|left, right| left.name == right.name);
    Ok(services)
}

fn is_service_manifest(destination: &str) -> bool {
    destination.starts_with("/etc/services.d/") && destination.ends_with(".toml")
}

fn read_service_manifest(root: &Path, relative: &str) -> Result<ShellService> {
    let path = root.join(relative);
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read service manifest {}", path.display()))?;
    let value: toml::Value = toml::from_str(&text)
        .with_context(|| format!("cannot parse service manifest {}", path.display()))?;
    let table = value
        .as_table()
        .ok_or_else(|| anyhow::anyhow!("{}: not a table", path.display()))?;
    let name = table
        .get("name")
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("{}: missing `name`", path.display()))?;
    // Defaults mirror `raven-svc-manifest`'s parser: on-failure, no readiness wait, enabled.
    let restart = table
        .get("restart")
        .and_then(toml::Value::as_str)
        .unwrap_or("on-failure");
    let readiness = table
        .get("readiness")
        .and_then(toml::Value::as_str)
        .unwrap_or("none");
    let enabled = table
        .get("enable")
        .and_then(toml::Value::as_bool)
        .unwrap_or(true);
    if !matches!(restart, "never" | "on-failure" | "always") {
        bail!("{}: invalid restart policy `{restart}`", path.display());
    }
    if !matches!(readiness, "scheme" | "exit" | "none") {
        bail!("{}: invalid readiness `{readiness}`", path.display());
    }
    Ok(ShellService {
        name: name.to_string(),
        manifest: relative.to_string(),
        may_restart: restart != "never",
        long_running: enabled && readiness != "exit",
    })
}

/// Profiles that extend `shell` and are booted by a qtest that expects the guest to exit.
fn one_shot_shell_profiles(root: &Path) -> Result<Vec<String>> {
    let mut used = Vec::new();
    for case in case_files(root)? {
        if !case.one_shot {
            continue;
        }
        if let Some(profile) = case.profile
            && extends_shell(root, &profile)?
            && !used.contains(&profile)
        {
            used.push(profile);
        }
    }
    used.sort();
    Ok(used)
}

struct CaseFile {
    profile: Option<String>,
    one_shot: bool,
}

fn case_files(root: &Path) -> Result<Vec<CaseFile>> {
    let directory = root.join("tests/qtest/cases");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(Vec::new());
    };
    let mut cases = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("toml") {
            continue;
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read case {}", path.display()))?;
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        // A case is one-shot unless it expects the timeout: only a guest that terminates needs
        // every inherited resident service to be pinned.
        let one_shot = value
            .get("expect_exit")
            .and_then(toml::Value::as_str)
            .is_none_or(|exit| exit != "timeout");
        cases.push(CaseFile {
            profile: value
                .get("profile")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            one_shot,
        });
    }
    Ok(cases)
}

fn extends_shell(root: &Path, profile: &str) -> Result<bool> {
    if !valid_name(profile) {
        return Ok(false);
    }
    let path = root.join("profiles").join(format!("{profile}.toml"));
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Ok(false);
    };
    Ok(value.get("extends").and_then(toml::Value::as_str) == Some("shell"))
}

/// The `rc.conf` the profile installs, and the file a developer must edit.
struct RcConf {
    path: String,
    text: String,
}

fn rc_conf_target(root: &Path, profile: &str) -> Result<RcConf> {
    let suggested = format!("profiles/{profile}.d/rc.conf");
    for fragment in fragment_paths(root, profile)? {
        let text = fs::read_to_string(&fragment)
            .with_context(|| format!("cannot read fragment {}", fragment.display()))?;
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        let Some(entries) = value
            .get("initfs")
            .and_then(|initfs| initfs.get("files"))
            .and_then(toml::Value::as_array)
        else {
            continue;
        };
        for entry in entries {
            if entry.get("path").and_then(toml::Value::as_str) != Some("/etc/rc.conf") {
                continue;
            }
            let Some(source) = entry.get("src").and_then(toml::Value::as_str) else {
                continue;
            };
            let text = fs::read_to_string(root.join(source)).unwrap_or_default();
            return Ok(RcConf {
                path: source.to_string(),
                text,
            });
        }
    }
    Ok(RcConf {
        path: suggested,
        text: String::new(),
    })
}

#[derive(Default)]
struct Overrides {
    restart: BTreeMap<String, String>,
    disabled: BTreeSet<String>,
}

impl Overrides {
    fn restarts(&self, service: &str) -> Option<&str> {
        self.restart.get(service).map(String::as_str)
    }

    fn disables(&self, service: &str) -> bool {
        self.disabled.contains(service)
    }
}

/// Read the `<service>_restart` and `<service>_enable` lines of an `rc.conf`.
///
/// Only these two override kinds matter for the gate; unknown keys are the guest's business.
fn read_overrides(text: &str) -> Overrides {
    let mut overrides = Overrides::default();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        if let Some(service) = key.trim().strip_suffix("_restart") {
            overrides
                .restart
                .insert(service.to_string(), value.to_string());
        } else if let Some(service) = key.trim().strip_suffix("_enable")
            && value.eq_ignore_ascii_case("NO")
        {
            overrides.disabled.insert(service.to_string());
        }
    }
    overrides
}

/// A reviewed exemption from the one-shot restart rule.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestartAllowFile {
    #[serde(default)]
    allow: Vec<RestartAllowEntry>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct RestartAllowEntry {
    service: String,
    reason: String,
    #[serde(default)]
    profiles: Vec<String>,
}

#[derive(Debug, Default)]
struct RestartAllowlist {
    entries: Vec<RestartAllowEntry>,
}

impl RestartAllowlist {
    fn permits(&self, profile: &str, service: &str) -> bool {
        self.entries.iter().any(|entry| {
            entry.service == service
                && (entry.profiles.is_empty() || entry.profiles.iter().any(|item| item == profile))
        })
    }
}

fn restart_allowlist(root: &Path) -> Result<RestartAllowlist> {
    let path = root.join(RESTART_ALLOWLIST);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(RestartAllowlist::default());
    };
    let parsed: RestartAllowFile =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    for entry in &parsed.allow {
        if entry.reason.trim().is_empty() {
            bail!(
                "{}: allowlist entry `{}` needs a reason",
                path.display(),
                entry.service
            );
        }
    }
    Ok(RestartAllowlist {
        entries: parsed.allow,
    })
}

#[cfg(test)]
mod tests {
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
        tree.profile("child", "name = \"child\"\nextends = \"base\"\n[boot]\ncmdline = \"child\"\nlog_level = \"info\"\n");
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
        tree.profile("child", "name = \"child\"\nextends = \"missing\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n");
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
        tree.profile("middle", "name = \"middle\"\nextends = \"base\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n");
        tree.profile("child", "name = \"child\"\nextends = \"middle\"\n[boot]\ncmdline = \"x\"\nlog_level = \"info\"\n");
        assert!(
            format!("{:#}", Profile::load(&tree.root, "child").unwrap_err()).contains("one-level")
        );
    }
    #[test]
    fn shell_profile_parses() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert_eq!(Profile::load(root, "shell").unwrap().name, "shell");
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
        for profile in ["core-test", "initfsd-test", "ramfsd-test"] {
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
        for (violation, profile) in
            violations
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
}
