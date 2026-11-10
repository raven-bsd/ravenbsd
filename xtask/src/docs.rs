// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Man-page coverage and rendering checks.
//!
//! Every program packed into an initfs profile, every directory under `userland/bin`, `init`, and
//! every syscall declared in `crates/raven-abi/src/syscall.rs` must have an mdoc page under
//! `docs/man<N>/`. The scan is deterministic: expected pages are kept in a sorted set and missing
//! paths are printed in sorted order.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::profiles::Profile;

/// One required man page.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Page {
    /// Manual section (`1`, `2`, `4` or `8`).
    pub(crate) section: u8,
    /// Page base name without the section suffix.
    pub(crate) name: String,
}

impl Page {
    /// Path relative to the repository root.
    pub(crate) fn relative_path(&self) -> String {
        format!("docs/man{0}/{1}.{0}", self.section, self.name)
    }
}

/// Run the docs gate. Returns 1 when pages are missing or `mandoc` reports warnings.
pub(crate) fn run(root: &Path) -> Result<u8> {
    let expected = expected_pages(root)?;
    let missing: Vec<String> = expected
        .iter()
        .filter(|page| !root.join(page.relative_path()).is_file())
        .map(Page::relative_path)
        .collect();
    if !missing.is_empty() {
        for path in &missing {
            println!("missing man page: {path}");
        }
        return Ok(1);
    }

    match lint_pages(root)? {
        LintOutcome::Skipped => {
            println!("docs: mandoc not installed; rendering check skipped");
            Ok(0)
        }
        LintOutcome::Checked { checked, failures } if failures.is_empty() => {
            println!("docs: {checked} page(s) checked");
            Ok(0)
        }
        LintOutcome::Checked { failures, .. } => {
            for failure in failures {
                println!("{failure}");
            }
            Ok(1)
        }
    }
}

/// Expected pages for the current tree, sorted by section and name.
pub(crate) fn expected_pages(root: &Path) -> Result<BTreeSet<Page>> {
    let mut pages = BTreeSet::new();

    let profiles = root.join("profiles");
    if profiles.is_dir() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&profiles)
            .with_context(|| format!("cannot list profile directory {}", profiles.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", profiles.display()))?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) == Some("toml") {
                let Some(name) = path.file_stem().and_then(|stem| stem.to_str()) else {
                    bail!("profile {} has no valid file name", path.display());
                };
                names.push(name.to_string());
            }
        }
        names.sort();
        for name in names {
            let profile = Profile::load(root, &name)?;
            for program in &profile.initfs.programs {
                let page_name = program
                    .package
                    .strip_prefix("raven-")
                    .unwrap_or(&program.package);
                let section = if page_name == "init" || program.path.starts_with("/sbin/") {
                    8
                } else {
                    1
                };
                pages.insert(Page {
                    section,
                    name: page_name.to_string(),
                });
            }
        }
    }

    let bin = root.join("userland/bin");
    if bin.is_dir() {
        let mut names = Vec::new();
        for entry in fs::read_dir(&bin).with_context(|| format!("cannot list {}", bin.display()))? {
            let entry = entry.with_context(|| format!("cannot read {}", bin.display()))?;
            let path = entry.path();
            if path.is_dir() && path.join("Cargo.toml").is_file() {
                let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                    bail!("binary directory {} has no valid name", path.display());
                };
                names.push(name.to_string());
            }
        }
        names.sort();
        for name in names {
            pages.insert(Page { section: 1, name });
        }
    }

    if root.join("userland/init").is_dir() {
        pages.insert(Page {
            section: 8,
            name: "init".to_string(),
        });
    }

    let syscalls = syscall_names(root)?;
    for name in syscalls {
        pages.insert(Page {
            section: 2,
            name: name.to_ascii_lowercase(),
        });
    }

    Ok(pages)
}

fn syscall_names(root: &Path) -> Result<Vec<String>> {
    let path = root.join("crates/raven-abi/src/syscall.rs");
    let text = fs::read_to_string(&path)
        .with_context(|| format!("cannot read syscall table {}", path.display()))?;
    let mut names = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || byte == b'_')
        {
            continue;
        }
        let rest = rest.trim();
        if rest.starts_with("0x") && rest.ends_with(';') {
            names.push(name.to_string());
        }
    }
    names.sort();
    names.dedup();
    Ok(names)
}

enum LintOutcome {
    Skipped,
    Checked {
        checked: usize,
        failures: Vec<String>,
    },
}

fn lint_pages(root: &Path) -> Result<LintOutcome> {
    let mut pages = Vec::new();
    for section in [1u8, 2, 4, 8] {
        let directory = root.join(format!("docs/man{section}"));
        if !directory.is_dir() {
            continue;
        }
        for entry in fs::read_dir(&directory)
            .with_context(|| format!("cannot list {}", directory.display()))?
        {
            let entry = entry.with_context(|| format!("cannot read {}", directory.display()))?;
            let path = entry.path();
            if path.is_file() {
                pages.push(path);
            }
        }
    }
    pages.sort();

    let mut checked = 0usize;
    let mut failures = Vec::new();
    for page in pages {
        let output = match Command::new("mandoc")
            .args(["-T", "lint"])
            .arg(&page)
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LintOutcome::Skipped);
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("cannot run mandoc on {}", page.display()));
            }
        };
        checked += 1;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !output.status.success() || !stderr.trim().is_empty() {
            let relative = page
                .strip_prefix(root)
                .unwrap_or(&page)
                .display()
                .to_string();
            let detail = stderr.trim();
            if detail.is_empty() {
                failures.push(format!(
                    "mandoc: {} failed with {}",
                    relative, output.status
                ));
            } else {
                failures.push(format!("mandoc: {relative}: {detail}"));
            }
        }
    }
    Ok(LintOutcome::Checked { checked, failures })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TREE: AtomicUsize = AtomicUsize::new(0);

    struct Tree {
        root: PathBuf,
    }

    impl Tree {
        fn new(label: &str) -> Self {
            let id = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "raven-docs-{}-{}-{label}",
                std::process::id(),
                id
            ));
            let _ = fs::remove_dir_all(&root);
            fs::create_dir_all(&root).expect("create fixture root");
            Self { root }
        }

        fn write(&self, relative: &str, contents: &str) {
            let path = self.root.join(relative);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).expect("create fixture parent");
            }
            fs::write(path, contents).expect("write fixture file");
        }

        fn profile(&self) {
            self.write(
                "profiles/base.toml",
                "name = \"base\"\n[initfs]\nprograms = [\n\
                 { package = \"raven-init\", path = \"/init\" },\n\
                 { package = \"raven-foo\", path = \"/bin/foo\" },\n\
                 ]\n[boot]\ncmdline = \"console=ttyS0\"\nlog_level = \"info\"\n",
            );
        }

        fn programs(&self) {
            self.write(
                "userland/init/Cargo.toml",
                "[package]\nname = \"raven-init\"\n",
            );
            self.write(
                "userland/bin/foo/Cargo.toml",
                "[package]\nname = \"raven-foo\"\n",
            );
        }

        fn syscalls(&self) {
            self.write(
                "crates/raven-abi/src/syscall.rs",
                "syscalls! {\n    EXIT = 0x01;\n    DEBUG_WRITE = 0x03;\n}\n",
            );
        }
    }

    impl Drop for Tree {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn fixture() -> Tree {
        let tree = Tree::new("fixture");
        tree.profile();
        tree.programs();
        tree.syscalls();
        tree
    }

    #[test]
    fn expected_pages_cover_profiles_programs_and_syscalls() {
        let tree = fixture();
        let pages = expected_pages(&tree.root).expect("scan");
        let names: Vec<String> = pages.iter().map(Page::relative_path).collect();
        assert_eq!(
            names,
            vec![
                "docs/man1/foo.1".to_string(),
                "docs/man2/debug_write.2".to_string(),
                "docs/man2/exit.2".to_string(),
                "docs/man8/init.8".to_string(),
            ]
        );
    }

    #[test]
    fn missing_pages_are_listed_and_sorted() {
        let tree = fixture();
        let pages = expected_pages(&tree.root).expect("scan");
        let missing: Vec<String> = pages
            .iter()
            .filter(|page| !tree.root.join(page.relative_path()).is_file())
            .map(Page::relative_path)
            .collect();
        assert_eq!(
            missing,
            vec![
                "docs/man1/foo.1".to_string(),
                "docs/man2/debug_write.2".to_string(),
                "docs/man2/exit.2".to_string(),
                "docs/man8/init.8".to_string(),
            ]
        );
    }

    #[test]
    fn present_pages_are_not_missing() {
        let tree = fixture();
        for page in expected_pages(&tree.root).expect("scan") {
            tree.write(&page.relative_path(), ".Dd January 1, 2026\n");
        }
        let pages = expected_pages(&tree.root).expect("scan");
        assert!(
            pages
                .iter()
                .all(|page| tree.root.join(page.relative_path()).is_file())
        );
    }

    #[test]
    fn scan_is_deterministic() {
        let tree = fixture();
        let first = expected_pages(&tree.root).expect("scan");
        let second = expected_pages(&tree.root).expect("scan");
        assert_eq!(first, second);
    }

    #[test]
    fn syscall_names_are_parsed_from_the_abi_table() {
        let tree = fixture();
        assert_eq!(
            syscall_names(&tree.root).expect("scan"),
            vec!["DEBUG_WRITE".to_string(), "EXIT".to_string()]
        );
    }

    #[test]
    fn profile_package_prefix_is_removed() {
        let tree = fixture();
        let pages = expected_pages(&tree.root).expect("scan");
        assert!(
            pages
                .iter()
                .any(|page| page.name == "foo" && page.section == 1)
        );
        assert!(
            pages
                .iter()
                .any(|page| page.name == "init" && page.section == 8)
        );
    }

    #[test]
    fn missing_syscall_page_is_reported() {
        let tree = fixture();
        tree.write("docs/man1/foo.1", "");
        tree.write("docs/man8/init.8", "");
        let pages = expected_pages(&tree.root).expect("scan");
        let missing: Vec<String> = pages
            .iter()
            .filter(|page| !tree.root.join(page.relative_path()).is_file())
            .map(Page::relative_path)
            .collect();
        assert!(missing.contains(&"docs/man2/exit.2".to_string()));
        assert!(missing.contains(&"docs/man2/debug_write.2".to_string()));
    }
}
