// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! File discovery: `git ls-files` with a directory-walk fallback.

use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};

const SKIP_DIRS: &[&str] = &[".git", "target", "build", "vendor"];

/// List repository-relative files under `root`, sorted.
pub(crate) fn discover(root: &Path) -> Result<Vec<String>> {
    if let Ok(files) = git_ls_files(root) {
        return Ok(files);
    }
    walk(root)
}

fn git_ls_files(root: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["ls-files", "-z"])
        .output()
        .context("running git ls-files")?;
    if !output.status.success() {
        anyhow::bail!("git ls-files failed");
    }
    let mut files: Vec<String> = output
        .stdout
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .map(|entry| String::from_utf8_lossy(entry).replace('\\', "/"))
        .collect();
    files.sort();
    files.dedup();
    Ok(files)
}

fn walk(root: &Path) -> Result<Vec<String>> {
    let mut files = Vec::new();
    walk_dir(root, root, &mut files)?;
    files.sort();
    files.dedup();
    Ok(files)
}

fn walk_dir(root: &Path, directory: &Path, files: &mut Vec<String>) -> Result<()> {
    let entries =
        fs::read_dir(directory).with_context(|| format!("reading {}", directory.display()))?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if SKIP_DIRS.contains(&name.as_ref()) {
                continue;
            }
            walk_dir(root, &path, files)?;
        } else if file_type.is_file() {
            let relative = path.strip_prefix(root).unwrap_or(&path);
            files.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walk_finds_files_and_skips_build_directories() {
        let temp = crate::testutil::TempDir::new("discover-walk");
        temp.write("src/lib.rs", "fn f() {}\n");
        temp.write("target/debug/junk.rs", "junk\n");
        temp.write("build/out.rs", "junk\n");
        temp.write("vendor/dep.rs", "junk\n");
        temp.write(".git/config", "junk\n");
        temp.mkdir("docs");
        temp.write("docs/readme.md", "# docs\n");

        let files = walk(temp.path()).unwrap();
        assert_eq!(files, vec!["docs/readme.md", "src/lib.rs"]);
    }

    #[test]
    fn results_are_sorted() {
        let temp = crate::testutil::TempDir::new("discover-sorted");
        temp.write("z.rs", "");
        temp.write("a.rs", "");
        temp.write("m/n.rs", "");
        let files = walk(temp.path()).unwrap();
        assert_eq!(files, vec!["a.rs", "m/n.rs", "z.rs"]);
    }

    #[test]
    fn git_repository_lists_tracked_files() {
        let temp = crate::testutil::TempDir::new("discover-git");
        temp.write("tracked.rs", "fn f() {}\n");
        temp.write("untracked.rs", "fn f() {}\n");
        let status = Command::new("git")
            .arg("-C")
            .arg(temp.path())
            .args(["init", "-q"])
            .status()
            .unwrap();
        assert!(status.success());
        let status = Command::new("git")
            .arg("-C")
            .arg(temp.path())
            .args(["add", "tracked.rs"])
            .status()
            .unwrap();
        assert!(status.success());

        let files = discover(temp.path()).unwrap();
        assert_eq!(files, vec!["tracked.rs"]);
    }
}
