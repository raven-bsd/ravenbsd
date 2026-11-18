// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT_TREE: AtomicUsize = AtomicUsize::new(0);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new(label: &str) -> Self {
        let id = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("raven-docs-{}-{}-{label}", std::process::id(), id));
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
