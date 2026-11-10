// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Profile parsing and deterministic fragment merging (`profiles/<name>.d/*.toml`).

use std::collections::BTreeMap;
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
            if file.src.is_empty() || !root.join(source).is_file() {
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
}
