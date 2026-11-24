// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! CLI integration tests for `ufs-inject`.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

use ufs_core::mkfs::MkfsOptions;

static NEXT: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("ufs-inject-cli-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp dir");
        Self(path)
    }

    fn join(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fresh_image(dir: &TempDir) -> PathBuf {
    let path = dir.join("root.ufs");
    ufs_mkfs::format_image(
        &path,
        &MkfsOptions {
            size: 8 * 1024 * 1024,
            version: 2,
            block_size: 4096,
            fragment_size: 512,
            bytes_per_inode: 4096,
            minfree: 5,
            label: "",
            timestamp: 0,
        },
        None,
    )
    .expect("format image");
    path
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ufs-inject"))
        .args(args)
        .output()
        .expect("run ufs-inject")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[test]
fn help_is_available() {
    let output = run(&["--help"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("put HOST DEST"));
}

#[test]
fn check_put_cat_stat_and_mkdir() {
    let dir = TempDir::new();
    let image_path = fresh_image(&dir);
    let image = image_path.to_str().expect("image path");
    let host = dir.join("probe");
    fs::write(&host, b"probe-payload\n").expect("host file");
    let host = host.to_str().expect("host path");

    let output = run(&[image, "check"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("clean"));

    let output = run(&[
        image,
        "put",
        host,
        "/bin/upd1-probe",
        "--mode",
        "755",
        "--parents",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("fsck=clean"));

    let output = run(&[image, "cat", "/bin/upd1-probe"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"probe-payload\n");

    let output = run(&[
        image,
        "symlink",
        "/bin/upd1-probe",
        "/bin/upd1-link",
        "--parents",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[image, "cat", "/bin/upd1-link"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"probe-payload\n");

    let output = run(&[image, "stat", "/bin/upd1-probe"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let text = stdout(&output);
    assert!(text.starts_with("file mode=0o755"), "{text}");
    assert!(text.contains("size=14"), "{text}");

    let output = run(&[
        image,
        "mkdir",
        "/opt/upd1/bin",
        "--parents",
        "--mode",
        "750",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[image, "list", "/opt/upd1"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "bin\n");

    let output = run(&[image, "put", host, "/opt/upd1/bin/probe", "--parents"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[image, "list", "/opt/upd1/bin"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(stdout(&output), "probe\n");

    let output = run(&[image, "check"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn rm_and_mtime_override_are_accepted() {
    let dir = TempDir::new();
    let image_path = fresh_image(&dir);
    let image = image_path.to_str().expect("image path");
    let host = dir.join("stamp.txt");
    fs::write(&host, b"stamp\n").expect("host file");
    let host = host.to_str().expect("host path");

    let output = run(&[image, "put", host, "/stamp.txt", "--mtime", "1600000042"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[image, "stat", "/stamp.txt"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("mtime=1600000042.000000000"));

    let output = run(&[image, "rm", "/stamp.txt"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[image, "list", "/"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stdout(&output).contains("stamp.txt"));
    let output = run(&[image, "check"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

#[test]
fn missing_parent_without_parents_fails() {
    let dir = TempDir::new();
    let image_path = fresh_image(&dir);
    let image = image_path.to_str().expect("image path");
    let host = dir.join("file");
    fs::write(&host, b"x").expect("host file");
    let host = host.to_str().expect("host path");
    let output = run(&[image, "put", host, "/no/parent/file"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not found") || stderr(&output).contains("lookup"));
}

#[test]
fn partition_offset_is_honoured() {
    let dir = TempDir::new();
    let ufs_path = fresh_image(&dir);
    let ufs = fs::read(&ufs_path).expect("read ufs");
    let prefix = 1024 * 1024usize;
    let container = dir.join("disk.img");
    let mut disk = vec![0x5au8; prefix];
    disk.extend_from_slice(&ufs);
    fs::write(&container, &disk).expect("container");
    let image = container.to_str().expect("container path");
    let host = dir.join("slice.txt");
    fs::write(&host, b"slice\n").expect("host file");
    let host = host.to_str().expect("host path");
    let offset = prefix.to_string();
    let size = ufs.len().to_string();

    let output = run(&[
        "--offset",
        &offset,
        "--size",
        &size,
        image,
        "put",
        host,
        "/slice.txt",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    let output = run(&[
        "--offset",
        &offset,
        "--size",
        &size,
        image,
        "cat",
        "/slice.txt",
    ]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(output.stdout, b"slice\n");
    let output = run(&["--offset", &offset, "--size", &size, image, "check"]);
    assert!(output.status.success(), "{}", stderr(&output));
}
