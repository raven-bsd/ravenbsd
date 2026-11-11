// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integration inventory: what every workspace package is, how (or whether) it runs on a target,
//! who consumes it, and where the consumer integration is missing.
//!
//! The inventory is deliberately factual. It reads manifests, profile fragments, service
//! manifests and qtest cases; it never guesses that a package is unused, because several crates
//! in this tree are host-test mirrors whose sources the kernel or a server includes directly.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Markers around the generated block in the design document.
pub const BLOCK_BEGIN: &str = "<!-- integration-inventory:begin -->";
/// End marker of the generated block.
pub const BLOCK_END: &str = "<!-- integration-inventory:end -->";

/// What a package is, judged from its path and from how the target uses it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Shared library under `crates/`, consumed by a target component.
    CoreLibrary,
    /// A crate that exists only to host-test source the target includes directly, or that
    /// includes kernel sources to test them on the host.
    HostTestMirror,
    /// Host tool under `tools/`.
    Tool,
    /// Userspace server under `userland/servers/`.
    Server,
    /// Feasibility spike under `libc/spike/`.
    Spike,
    /// The kernel.
    Kernel,
    /// The UEFI loader.
    Loader,
    /// Userspace runtime or program that is not a server.
    UserspaceProgram,
    /// Test harness (qtest, oracle drivers).
    TestHarness,
    /// Guest program built only for a qtest case.
    TestFixture,
}

impl Kind {
    /// Stable spelling used in the table.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CoreLibrary => "core library",
            Self::HostTestMirror => "host test mirror",
            Self::Tool => "tool",
            Self::Server => "server",
            Self::Spike => "spike",
            Self::Kernel => "kernel",
            Self::Loader => "loader",
            Self::UserspaceProgram => "userspace program",
            Self::TestHarness => "test harness",
            Self::TestFixture => "test fixture",
        }
    }
}

/// One table row.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Cargo package name.
    pub package: String,
    /// Repository-relative package directory.
    pub path: String,
    /// What the package is.
    pub kind: Kind,
    /// Where it runs, comma-separated: `host test`, `QEMU guest`, `image`.
    pub execution: String,
    /// Who consumes it: dependent packages, profiles and qtest cases.
    pub consumers: String,
    /// Service restart policy when the package ships an `/etc/services.d` manifest.
    pub restart: String,
    /// The known integration gap, or `-`.
    pub gap: String,
}

/// The whole inventory, sorted by package name.
#[derive(Debug, Default)]
pub struct Inventory {
    /// Table rows.
    pub rows: Vec<Row>,
}

impl Inventory {
    /// Markdown table body, sorted by package name.
    pub fn table(&self) -> String {
        let mut text = String::from(
            "| component | path | kind | target execution | consumers | restart | known gap |\n\
             |---|---|---|---|---|---|---|\n",
        );
        for row in &self.rows {
            text.push_str(&format!(
                "| `{}` | `{}` | {} | {} | {} | {} | {} |\n",
                row.package,
                row.path,
                row.kind.as_str(),
                row.execution,
                row.consumers,
                row.restart,
                row.gap
            ));
        }
        text
    }

    /// Counts per kind and per target execution.
    pub fn summary(&self) -> String {
        let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &self.rows {
            *kinds.entry(row.kind.as_str()).or_default() += 1;
        }
        let mut executions: BTreeMap<&str, usize> = BTreeMap::new();
        for row in &self.rows {
            for item in row.execution.split(", ").filter(|item| !item.is_empty()) {
                *executions.entry(item).or_default() += 1;
            }
        }
        let mut text = format!("packages: {}\n", self.rows.len());
        text.push_str("by kind:");
        for (kind, count) in kinds {
            text.push_str(&format!(" {kind} {count};"));
        }
        text.push_str("\nby target execution:");
        for (execution, count) in executions {
            text.push_str(&format!(" {execution} {count};"));
        }
        text.push('\n');
        text
    }

    /// Packages that only run as host tests: the candidate integration gaps.
    pub fn host_only(&self) -> Vec<&Row> {
        let mut rows: Vec<&Row> = self
            .rows
            .iter()
            .filter(|row| row.execution == "host test")
            .collect();
        rows.sort_by(|left, right| left.package.cmp(&right.package));
        rows
    }
}

/// Build the inventory for the repository at `root`.
pub fn inventory(root: &Path) -> Result<Inventory> {
    let root = fs::canonicalize(root).unwrap_or_else(|_| normalise(root.to_path_buf()));
    let root = root.as_path();
    let members = workspace_members(root)?;
    let packages = read_packages(root, &members)?;
    let links = source_links(root, &packages)?;
    let profiles = read_profiles(root, &packages)?;
    let cases = read_cases(root)?;
    let installed = installed_packages(&profiles);
    let execution_tags = execution_tags(root, &packages, &links, &installed, &cases);

    let mut rows = Vec::with_capacity(packages.len());
    for package in &packages {
        let kind = classify(root, package, &links);
        let dependents: Vec<&str> = packages
            .iter()
            .filter(|other| other.name != package.name && other.deps.contains(&package.name))
            .map(|other| other.name.as_str())
            .collect();
        let includers: Vec<&str> = links
            .included_by
            .get(&package.path)
            .map(|set| set.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let includes: Vec<&str> = links
            .includes
            .get(&package.path)
            .map(|set| set.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let installs: Vec<&str> = installed
            .get(&package.name)
            .map(|set| set.iter().map(String::as_str).collect())
            .unwrap_or_default();
        let mut used_cases: Vec<&str> = cases
            .iter()
            .filter(|case| {
                case.profile
                    .as_deref()
                    .is_some_and(|name| installs.contains(&name))
            })
            .map(|case| case.name.as_str())
            .collect();
        // The kernel and the loader are in every booted image, even though no profile installs
        // them into the initfs.
        if matches!(kind, Kind::Kernel | Kind::Loader) {
            used_cases = cases.iter().map(|case| case.name.as_str()).collect();
        }
        let fixture_cases: Vec<&str> = if kind == Kind::TestFixture {
            cases
                .iter()
                .filter(|case| builds_fixture(package, case))
                .map(|case| case.name.as_str())
                .collect()
        } else {
            Vec::new()
        };
        if !fixture_cases.is_empty() {
            used_cases.extend(fixture_cases.iter().copied());
            used_cases.sort();
            used_cases.dedup();
        }

        let mut execution = Vec::new();
        if has_tests(&root.join(&package.path)) || matches!(kind, Kind::Tool | Kind::TestHarness) {
            execution.push("host test".to_string());
        }
        if let Some(tags) = execution_tags.get(&package.name) {
            execution.extend(tags.iter().cloned());
        }
        let execution = if execution.is_empty() {
            "-".to_string()
        } else {
            execution.join(", ")
        };

        let consumers = render_consumers(
            &dependents,
            &includers,
            &includes,
            &installs,
            &used_cases,
            cases.len(),
        );
        let restart = profiles.restart_for(&package.name);
        let gap = gap_for(
            kind,
            &execution,
            &Usage {
                used_cases: &used_cases,
                installs: &installs,
            },
            &fixture_cases,
        );

        rows.push(Row {
            package: package.name.clone(),
            path: package.path.clone(),
            kind,
            execution,
            consumers,
            restart,
            gap,
        });
    }
    rows.sort_by(|left, right| left.package.cmp(&right.package));
    Ok(Inventory { rows })
}

/// Where each package ends up executing: a package runs wherever its consumers run.
///
/// Seeds are the packages installed into a profile (QEMU guest when a case boots that profile,
/// otherwise image), the kernel and the loader. Dependency and source-inclusion edges propagate
/// the tags from consumer to consumed package until nothing changes.
fn execution_tags(
    root: &Path,
    packages: &[Package],
    links: &Links,
    installed: &BTreeMap<String, BTreeSet<String>>,
    cases: &[Case],
) -> BTreeMap<String, BTreeSet<String>> {
    let mut tags: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let profile_cases: BTreeSet<&str> = cases
        .iter()
        .filter_map(|case| case.profile.as_deref())
        .collect();
    for (package, profiles) in installed {
        let guest = profiles
            .iter()
            .any(|profile| profile_cases.contains(profile.as_str()));
        let tag = if guest { "QEMU guest" } else { "image" };
        tags.entry(package.clone())
            .or_default()
            .insert(tag.to_string());
    }
    for package in packages {
        if matches!(classify(root, package, links), Kind::Kernel | Kind::Loader)
            && !cases.is_empty()
        {
            tags.entry(package.name.clone())
                .or_default()
                .insert("QEMU guest".to_string());
        }
        // A fixture runs in the guest of the case that builds it.
        if classify(root, package, links) == Kind::TestFixture
            && cases.iter().any(|case| builds_fixture(package, case))
        {
            tags.entry(package.name.clone())
                .or_default()
                .insert("QEMU guest".to_string());
        }
    }

    let known: BTreeSet<&str> = packages
        .iter()
        .map(|package| package.name.as_str())
        .collect();
    let by_path: BTreeMap<&str, &Package> = packages
        .iter()
        .map(|package| (package.path.as_str(), package))
        .collect();

    loop {
        let mut additions: Vec<(String, String)> = Vec::new();
        for package in packages {
            let Some(own) = tags.get(&package.name).cloned() else {
                continue;
            };
            for dep in &package.deps {
                if known.contains(dep.as_str()) {
                    for tag in &own {
                        additions.push((dep.clone(), tag.clone()));
                    }
                }
            }
            if let Some(included) = links.includes.get(&package.path) {
                for path in included {
                    if let Some(target) = by_path.get(path.as_str()) {
                        for tag in &own {
                            additions.push((target.name.clone(), tag.clone()));
                        }
                    }
                }
            }
        }
        let mut changed = false;
        for (package, tag) in additions {
            if tags.entry(package).or_default().insert(tag) {
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    // A package that only reaches an image (no case boots it) is not also a QEMU guest.
    for set in tags.values_mut() {
        if set.contains("QEMU guest") {
            set.remove("image");
        }
    }
    tags
}

fn render_consumers(
    dependents: &[&str],
    includers: &[&str],
    includes: &[&str],
    installs: &[&str],
    cases: &[&str],
    total_cases: usize,
) -> String {
    let mut parts = Vec::new();
    if !dependents.is_empty() {
        parts.push(format!("packages: {}", dependents.join(", ")));
    }
    if !includers.is_empty() {
        parts.push(format!("source included by: {}", includers.join(", ")));
    }
    if !includes.is_empty() {
        parts.push(format!("includes source: {}", includes.join(", ")));
    }
    if !installs.is_empty() {
        parts.push(format!("profiles: {}", installs.join(", ")));
    }
    if !cases.is_empty() {
        if cases.len() == total_cases && total_cases > 0 && cases.len() > 6 {
            parts.push(format!("cases: all {total_cases}"));
        } else {
            parts.push(format!("cases: {}", cases.join(", ")));
        }
    }
    if parts.is_empty() {
        "-".to_string()
    } else {
        parts.join("; ")
    }
}

/// Repository-relative package directories from the root `Cargo.toml`.
fn workspace_members(root: &Path) -> Result<Vec<String>> {
    let path = root.join("Cargo.toml");
    let text =
        fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
    let members = value
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("{}: no [workspace] members", path.display()))?;
    let mut directories = Vec::new();
    for member in members {
        let Some(pattern) = member.as_str() else {
            continue;
        };
        if let Some(parent) = pattern.strip_suffix("/*") {
            let directory = root.join(parent);
            let entries = fs::read_dir(&directory)
                .with_context(|| format!("cannot list {}", directory.display()))?;
            for entry in entries {
                let entry = entry?;
                if entry.path().join("Cargo.toml").is_file() {
                    directories.push(format!("{parent}/{}", entry.file_name().to_string_lossy()));
                }
            }
        } else if root.join(pattern).join("Cargo.toml").is_file() {
            directories.push(pattern.to_string());
        }
    }
    directories.sort();
    directories.dedup();
    Ok(directories)
}

#[derive(Clone, Debug)]
struct Package {
    name: String,
    path: String,
    /// Normal and build dependencies on workspace members.
    deps: BTreeSet<String>,
}

fn read_packages(root: &Path, members: &[String]) -> Result<Vec<Package>> {
    let mut packages = Vec::with_capacity(members.len());
    for directory in members {
        let path = root.join(directory).join("Cargo.toml");
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let value: toml::Value =
            toml::from_str(&text).with_context(|| format!("cannot parse {}", path.display()))?;
        let name = value
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
            .ok_or_else(|| anyhow::anyhow!("{}: missing package.name", path.display()))?
            .to_string();
        let mut deps = BTreeSet::new();
        collect_deps(&value, &mut deps);
        packages.push(Package {
            name,
            path: directory.clone(),
            deps,
        });
    }
    Ok(packages)
}

/// Collect normal and build dependency names, including `[target.'cfg(...)'.dependencies]`.
fn collect_deps(value: &toml::Value, deps: &mut BTreeSet<String>) {
    let Some(table) = value.as_table() else {
        return;
    };
    for (key, child) in table {
        match key.as_str() {
            "dependencies" | "build-dependencies" => {
                if let Some(entries) = child.as_table() {
                    for name in entries.keys() {
                        deps.insert(name.clone());
                    }
                }
            }
            // Dev-dependencies are test-only and never a target consumer.
            "dev-dependencies" => {}
            _ => {
                if child.is_table() {
                    collect_deps(child, deps);
                }
            }
        }
    }
}

#[derive(Debug, Default)]
struct Links {
    /// package path -> packages that include its sources.
    included_by: BTreeMap<String, BTreeSet<String>>,
    /// package path -> package paths whose sources it includes.
    includes: BTreeMap<String, BTreeSet<String>>,
}

fn source_links(root: &Path, packages: &[Package]) -> Result<Links> {
    let mut links = Links::default();
    let mut files = Vec::new();
    for directory in ["kernel", "boot", "userland", "libc", "crates"] {
        collect_rust_files(&root.join(directory), &mut files)?;
    }
    for file in files {
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(_) => continue,
        };
        let Some(owner) = package_of(packages, root, &file) else {
            continue;
        };
        for target in path_attributes(&text) {
            let resolved = normalise(file.parent().unwrap_or(root).join(&target));
            let Some(included) = package_of(packages, root, &resolved) else {
                continue;
            };
            if included == owner {
                continue;
            }
            links
                .included_by
                .entry(included.clone())
                .or_default()
                .insert(owner.clone());
            links
                .includes
                .entry(owner.clone())
                .or_default()
                .insert(included);
        }
    }
    Ok(links)
}

fn collect_rust_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !directory.is_dir() {
        return Ok(());
    }
    for entry in
        fs::read_dir(directory).with_context(|| format!("cannot list {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if matches!(name.as_ref(), "target" | "build" | ".git") {
                continue;
            }
            collect_rust_files(&path, files)?;
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("rs") {
            files.push(path);
        }
    }
    Ok(())
}

/// `#[path = "..."]` values in a source file.
fn path_attributes(text: &str) -> Vec<String> {
    let mut targets = Vec::new();
    let mut rest = text;
    while let Some(index) = rest.find("#[path = \"") {
        rest = &rest[index + "#[path = \"".len()..];
        if let Some(end) = rest.find('"') {
            targets.push(rest[..end].to_string());
            rest = &rest[end..];
        } else {
            break;
        }
    }
    targets
}

fn normalise(path: PathBuf) -> PathBuf {
    let mut output = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                output.pop();
            }
            std::path::Component::CurDir => {}
            other => output.push(other),
        }
    }
    output
}

/// The package whose directory contains `file`.
fn package_of(packages: &[Package], root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    let text = relative.to_string_lossy().replace('\\', "/");
    packages
        .iter()
        .filter(|package| text == package.path || text.starts_with(&format!("{}/", package.path)))
        .max_by_key(|package| package.path.len())
        .map(|package| package.path.clone())
}

#[derive(Debug, Default)]
struct Profiles {
    /// profile name -> packages it installs, its own plus the parent's.
    installs: BTreeMap<String, BTreeSet<String>>,
    /// package name -> guest paths it installs (`/sbin/logd`, ...).
    guest_paths: BTreeMap<String, BTreeSet<String>>,
    /// guest path -> service restart policy from the manifest installed for it.
    restart_by_path: BTreeMap<String, String>,
    /// generated fixture directory (`build/fixtures/<dir>`) -> package name.
    fixtures: BTreeMap<String, String>,
}

impl Profiles {
    /// Restart policy of the service the package starts, if it ships one.
    fn restart_for(&self, package: &str) -> String {
        let mut restarts = Vec::new();
        if let Some(paths) = self.guest_paths.get(package) {
            for path in paths {
                if let Some(restart) = self.restart_by_path.get(path)
                    && !restarts.contains(restart)
                {
                    restarts.push(restart.clone());
                }
            }
        }
        if restarts.is_empty() {
            "-".to_string()
        } else {
            restarts.join(", ")
        }
    }
}

fn read_profiles(root: &Path, packages: &[Package]) -> Result<Profiles> {
    let mut profiles = Profiles::default();
    let fixtures = fixture_directories(root, packages)?;
    profiles.fixtures = fixtures.clone();

    let directory = root.join("profiles");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Ok(profiles);
    };
    let mut names = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path.extension().and_then(|extension| extension.to_str()) == Some("toml")
            && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str())
        {
            names.push(stem.to_string());
        }
    }
    names.sort();

    let mut own: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    for name in &names {
        let mut fragments = vec![root.join("profiles").join(format!("{name}.toml"))];
        fragments.extend(list_toml(&root.join("profiles").join(format!("{name}.d")))?);
        let mut installed = BTreeSet::new();
        for fragment in fragments {
            let text = fs::read_to_string(&fragment)
                .with_context(|| format!("cannot read {}", fragment.display()))?;
            let Ok(value) = toml::from_str::<toml::Value>(&text) else {
                continue;
            };
            if let Some(parent) = value.get("extends").and_then(toml::Value::as_str) {
                parents.insert(name.clone(), parent.to_string());
            }
            let Some(initfs) = value.get("initfs") else {
                continue;
            };
            if let Some(programs) = initfs.get("programs").and_then(toml::Value::as_array) {
                for program in programs {
                    let (Some(package), Some(path)) = (
                        program.get("package").and_then(toml::Value::as_str),
                        program.get("path").and_then(toml::Value::as_str),
                    ) else {
                        continue;
                    };
                    installed.insert(package.to_string());
                    profiles
                        .guest_paths
                        .entry(package.to_string())
                        .or_default()
                        .insert(path.to_string());
                }
            }
            let Some(files) = initfs.get("files").and_then(toml::Value::as_array) else {
                continue;
            };
            for file in files {
                let (Some(source), Some(destination)) = (
                    file.get("src").and_then(toml::Value::as_str),
                    file.get("path").and_then(toml::Value::as_str),
                ) else {
                    continue;
                };
                if let Some(package) = fixture_of(source, &fixtures) {
                    installed.insert(package.clone());
                    profiles
                        .guest_paths
                        .entry(package)
                        .or_default()
                        .insert(destination.to_string());
                }
                if destination.starts_with("/etc/services.d/")
                    && destination.ends_with(".toml")
                    && let Some((name, restart)) = manifest_service(&root.join(source))?
                {
                    profiles.restart_by_path.insert(name, restart);
                }
            }
        }
        own.insert(name.clone(), installed);
    }

    // One level of inheritance, as `Profile::load` enforces.
    for name in &names {
        let mut installed = own.get(name).cloned().unwrap_or_default();
        if let Some(parent) = parents.get(name)
            && let Some(inherited) = own.get(parent)
        {
            installed.extend(inherited.iter().cloned());
        }
        profiles.installs.insert(name.clone(), installed);
    }
    Ok(profiles)
}

fn list_toml(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(paths);
    };
    for entry in entries {
        let path = entry?.path();
        if path.is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("toml")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Service exec path and restart policy of an installed manifest.
fn manifest_service(path: &Path) -> Result<Option<(String, String)>> {
    let Ok(text) = fs::read_to_string(path) else {
        return Ok(None);
    };
    let Ok(value) = toml::from_str::<toml::Value>(&text) else {
        return Ok(None);
    };
    let Some(exec) = value.get("exec").and_then(toml::Value::as_str) else {
        return Ok(None);
    };
    let restart = value
        .get("restart")
        .and_then(toml::Value::as_str)
        .unwrap_or("on-failure");
    Ok(Some((exec.to_string(), restart.to_string())))
}

/// `build/fixtures/<dir>/...` sources mapped to the package whose build script produces them.
fn fixture_directories(root: &Path, packages: &[Package]) -> Result<BTreeMap<String, String>> {
    let mut fixtures = BTreeMap::new();
    for package in packages {
        let script = root.join(&package.path).join("build.sh");
        let Ok(text) = fs::read_to_string(&script) else {
            continue;
        };
        for line in text.lines() {
            let line = line.trim();
            let Some(value) = line.strip_prefix("out=") else {
                continue;
            };
            if let Some(name) = value.trim().strip_prefix("build/fixtures/") {
                fixtures.insert(name.trim_end_matches('/').to_string(), package.name.clone());
            }
        }
    }
    Ok(fixtures)
}

fn fixture_of(source: &str, fixtures: &BTreeMap<String, String>) -> Option<String> {
    let rest = source.strip_prefix("build/fixtures/")?;
    let directory = rest.split('/').next()?;
    fixtures.get(directory).cloned()
}

#[derive(Debug)]
struct Case {
    name: String,
    profile: Option<String>,
    /// The whole case text, for fixture attribution.
    text: String,
}

fn read_cases(root: &Path) -> Result<Vec<Case>> {
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
        let text =
            fs::read_to_string(&path).with_context(|| format!("cannot read {}", path.display()))?;
        let Ok(value) = toml::from_str::<toml::Value>(&text) else {
            continue;
        };
        let Some(name) = value.get("name").and_then(toml::Value::as_str) else {
            continue;
        };
        cases.push(Case {
            name: name.to_string(),
            profile: value
                .get("profile")
                .and_then(toml::Value::as_str)
                .map(str::to_string),
            text,
        });
    }
    cases.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(cases)
}

fn installed_packages(profiles: &Profiles) -> BTreeMap<String, BTreeSet<String>> {
    let mut installed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (profile, names) in &profiles.installs {
        for name in names {
            installed
                .entry(name.clone())
                .or_default()
                .insert(profile.clone());
        }
    }
    installed
}

fn classify(root: &Path, package: &Package, links: &Links) -> Kind {
    let path = package.path.as_str();
    if path == "kernel" {
        return Kind::Kernel;
    }
    if path == "boot/loader" {
        return Kind::Loader;
    }
    if path.starts_with("libc/spike") {
        return Kind::Spike;
    }
    if path.starts_with("tools/") || path == "xtask" {
        return Kind::Tool;
    }
    if path.starts_with("userland/servers/") {
        return Kind::Server;
    }
    if path.starts_with("userland/") {
        return Kind::UserspaceProgram;
    }
    if path.starts_with("tests/fixtures/") {
        return Kind::TestFixture;
    }
    if path.starts_with("tests/") {
        return Kind::TestHarness;
    }
    if path.starts_with("crates/") {
        // A binary crate under `crates/` is a host tool (the provenance checker), not a library.
        if root.join(path).join("src/main.rs").is_file() {
            return Kind::Tool;
        }
        // A mirror exists because the target includes the source file directly (the crate is
        // host-tested) or because the crate includes target sources to test them on the host.
        if links.included_by.contains_key(path) || links.includes.contains_key(path) {
            return Kind::HostTestMirror;
        }
        return Kind::CoreLibrary;
    }
    Kind::Tool
}

/// The `build.sh` scripts a fixture package can be built by: its own, or a parent's
/// (`tests/fixtures/s24-spawn/build.sh` builds the parent and child packages).
fn fixture_scripts(package: &Package) -> Vec<String> {
    let mut scripts = Vec::new();
    let mut directory = package.path.as_str();
    loop {
        scripts.push(format!("{directory}/build.sh"));
        let Some((parent, _)) = directory.rsplit_once('/') else {
            break;
        };
        if !parent.starts_with("tests/fixtures") {
            break;
        }
        directory = parent;
    }
    scripts
}

fn has_tests(directory: &Path) -> bool {
    if directory.join("tests").is_dir() {
        return true;
    }
    let mut files = Vec::new();
    if collect_rust_files(&directory.join("src"), &mut files).is_err() {
        return false;
    }
    files.iter().any(|file| {
        fs::read_to_string(file)
            .map(|text| text.contains("#[cfg(test)]") || text.contains("#[test]"))
            .unwrap_or(false)
    })
}

#[derive(Clone, Copy)]
struct Usage<'a> {
    used_cases: &'a [&'a str],
    installs: &'a [&'a str],
}

fn gap_for(kind: Kind, execution: &str, usage: &Usage<'_>, fixture_cases: &[&str]) -> String {
    match kind {
        Kind::TestFixture => {
            if fixture_cases.is_empty() {
                "no qtest case builds this fixture".to_string()
            } else {
                format!("fixture for {}", fixture_cases.join(", "))
            }
        }
        Kind::Tool | Kind::TestHarness => "-".to_string(),
        _ => {
            let integrated = execution.contains("QEMU guest") || execution.contains("image");
            if !integrated
                && matches!(
                    kind,
                    Kind::CoreLibrary | Kind::HostTestMirror | Kind::UserspaceProgram | Kind::Spike
                )
            {
                "host tests only; no target consumer".to_string()
            } else if usage.installs.is_empty() && kind == Kind::Server {
                "server is not installed by any profile".to_string()
            } else if usage.used_cases.is_empty() && !usage.installs.is_empty() {
                "image only; no qtest case boots it".to_string()
            } else {
                "-".to_string()
            }
        }
    }
}

/// Whether the case builds this fixture (its own or its directory's `build.sh`).
fn builds_fixture(package: &Package, case: &Case) -> bool {
    fixture_scripts(package)
        .iter()
        .any(|script| case.text.contains(script.as_str()))
}

/// The generated block of the design document, if the markers are present.
///
/// The block is exactly the text between the markers, including the blank lines around the
/// fenced sections, so it can be compared with [`render_block`] byte for byte.
pub fn document_block(text: &str) -> Option<String> {
    let start = text.find(BLOCK_BEGIN)? + BLOCK_BEGIN.len();
    let end = text.find(BLOCK_END)?;
    if end < start {
        return None;
    }
    Some(text[start..end].to_string())
}

/// Render the block the document must contain.
///
/// The result is the exact text between the two markers: a blank line, the fenced table, the
/// fenced counts, and a trailing blank line before the end marker.
pub fn render_block(inventory: &Inventory) -> String {
    format!(
        "\n\n```\n{}\n```\n\n```\n{}```\n\n",
        inventory.table().trim_end(),
        inventory.summary()
    )
}

/// Compare the document against a fresh inventory.
///
/// Returns `Ok(None)` when it matches and `Ok(Some(path))` when the checked-out document is stale.
pub fn check_document(root: &Path, document: &Path) -> Result<Option<String>> {
    let inventory = inventory(root)?;
    let text = fs::read_to_string(document)
        .with_context(|| format!("cannot read {}", document.display()))?;
    let expected = render_block(&inventory);
    match document_block(&text) {
        Some(current) if current == expected => Ok(None),
        Some(_) => Ok(Some(document.display().to_string())),
        None => bail!(
            "{} has no `{}`/`{}` block",
            document.display(),
            BLOCK_BEGIN,
            BLOCK_END
        ),
    }
}

/// Replace the generated block of the document with a fresh inventory.
///
/// The prose around the markers is preserved; only the block between them is rewritten.
pub fn write_document(root: &Path, document: &Path) -> Result<()> {
    let inventory = inventory(root)?;
    let text = fs::read_to_string(document)
        .with_context(|| format!("cannot read {}", document.display()))?;
    let start = text
        .find(BLOCK_BEGIN)
        .ok_or_else(|| anyhow::anyhow!("{} has no `{BLOCK_BEGIN}`", document.display()))?
        + BLOCK_BEGIN.len();
    let end = text
        .find(BLOCK_END)
        .ok_or_else(|| anyhow::anyhow!("{} has no `{BLOCK_END}`", document.display()))?;
    if end < start {
        bail!(
            "{}: the block markers are in the wrong order",
            document.display()
        );
    }
    let mut updated = String::with_capacity(text.len());
    updated.push_str(&text[..start]);
    updated.push_str(&render_block(&inventory));
    updated.push_str(&text[end..]);
    fs::write(document, updated).with_context(|| format!("cannot write {}", document.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_attributes_are_extracted() {
        let text = "#[path = \"../a.rs\"]\nmod a;\n#[path = \"../b.rs\"] mod b;\n";
        assert_eq!(path_attributes(text), vec!["../a.rs", "../b.rs"]);
        assert!(path_attributes("mod a;").is_empty());
    }

    #[test]
    fn parent_components_are_folded() {
        assert_eq!(
            normalise(PathBuf::from("/repo/kernel/src/../../crates/x/src/imp.rs")),
            PathBuf::from("/repo/crates/x/src/imp.rs")
        );
    }

    #[test]
    fn document_block_is_between_the_markers() {
        let text = format!("intro\n{BLOCK_BEGIN}\nbody\n{BLOCK_END}\noutro\n");
        assert_eq!(document_block(&text).as_deref(), Some("\nbody\n"));
        assert_eq!(document_block("no markers"), None);
    }

    #[test]
    fn render_block_contains_the_table_and_the_counts() {
        let inventory = Inventory {
            rows: vec![Row {
                package: "raven-abi".to_string(),
                path: "crates/raven-abi".to_string(),
                kind: Kind::CoreLibrary,
                execution: "host test".to_string(),
                consumers: "packages: raven-kernel".to_string(),
                restart: "-".to_string(),
                gap: "-".to_string(),
            }],
        };
        let block = render_block(&inventory);
        assert!(block.starts_with("\n\n```\n"));
        assert!(block.ends_with("```\n\n"));
        assert!(block.contains("| `raven-abi` | `crates/raven-abi` | core library |"));
        assert!(block.contains("packages: 1"));
    }

    #[test]
    fn consumers_collapse_a_full_case_list() {
        let cases: Vec<&str> = (0..8).map(|_| "case").collect();
        let text = render_consumers(&[], &[], &[], &[], &cases, 8);
        assert_eq!(text, "cases: all 8");
        let text = render_consumers(&[], &[], &[], &["base"], &["case"], 8);
        assert_eq!(text, "profiles: base; cases: case");
        assert_eq!(render_consumers(&[], &[], &[], &[], &[], 0), "-");
    }
}
