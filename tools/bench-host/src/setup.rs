// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Static musl helper binaries and the microVM initramfs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const WORK_DIR: &str = "/tmp/opencode/B31";
const NS_INIT_SOURCE: &str = "fn main() {}\n";
const MICROVM_INIT_SOURCE: &str = r#"
use std::io::Write;
extern "C" {
    fn reboot(cmd: i32) -> i32;
}
fn main() {
    println!("READY");
    let _ = std::io::stdout().flush();
    unsafe {
        reboot(0x4321_fedc);
    }
    std::process::exit(0);
}
"#;

pub(crate) fn work_dir() -> PathBuf {
    PathBuf::from(WORK_DIR)
}

pub(crate) fn run_setup() -> Result<()> {
    let ns = build_helper("bench-ns-init", NS_INIT_SOURCE)?;
    let microvm = build_helper("bench-microvm-init", MICROVM_INIT_SOURCE)?;
    println!("ns init:      {}", ns.display());
    println!("microVM init: {}", microvm.display());
    Ok(())
}

pub(crate) fn ensure_ns_init() -> Result<PathBuf> {
    ensure_helper("bench-ns-init", NS_INIT_SOURCE)
}

pub(crate) fn ensure_microvm_init() -> Result<PathBuf> {
    ensure_helper("bench-microvm-init", MICROVM_INIT_SOURCE)
}

fn ensure_helper(name: &str, source: &str) -> Result<PathBuf> {
    let path = work_dir().join(name);
    if path.exists() {
        return Ok(path);
    }
    build_helper(name, source)
}

fn build_helper(name: &str, source: &str) -> Result<PathBuf> {
    let directory = work_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let source_path = directory.join(format!("{name}.rs"));
    let output = directory.join(name);
    fs::write(&source_path, source)
        .with_context(|| format!("cannot write {}", source_path.display()))?;
    let rustc = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let result = Command::new(rustc)
        .args([
            "--edition",
            "2021",
            "--target",
            "x86_64-unknown-linux-musl",
            "-O",
            "-o",
        ])
        .arg(&output)
        .arg(&source_path)
        .output()
        .context("cannot run rustc for the musl helper")?;
    if !result.status.success() {
        bail!(
            "rustc failed for {name}: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    Ok(output)
}

pub(crate) fn write_initramfs(init: &Path) -> Result<PathBuf> {
    let data = fs::read(init).with_context(|| format!("cannot read {}", init.display()))?;
    let mut archive = Vec::new();
    newc_entry(&mut archive, "init", &data, 0o100755);
    newc_entry(&mut archive, "TRAILER!!!", &[], 0);
    while archive.len() % 512 != 0 {
        archive.push(0);
    }
    let directory = work_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let path = directory.join("initramfs.cpio");
    fs::write(&path, archive).with_context(|| format!("cannot write {}", path.display()))?;
    Ok(path)
}

fn newc_entry(out: &mut Vec<u8>, name: &str, data: &[u8], mode: u32) {
    let namesize = name.len() + 1;
    let mut header = String::with_capacity(110);
    header.push_str("070701");
    for value in [
        1,    // inode
        mode, // mode
        0,    // uid
        0,    // gid
        1,    // nlink
        0,    // mtime
        data.len() as u32,
        0, // devmajor
        0, // devminor
        0, // rdevmajor
        0, // rdevminor
        namesize as u32,
        0, // check
    ] {
        header.push_str(&format!("{value:08x}"));
    }
    debug_assert_eq!(header.len(), 110);
    out.extend_from_slice(header.as_bytes());
    out.extend_from_slice(name.as_bytes());
    out.push(0);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out.extend_from_slice(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newc_archive_has_magic_and_trailer() {
        let mut archive = Vec::new();
        newc_entry(&mut archive, "init", b"hello", 0o100755);
        newc_entry(&mut archive, "TRAILER!!!", &[], 0);
        assert_eq!(&archive[..6], b"070701");
        assert!(archive.windows(10).any(|window| window == b"TRAILER!!!"));
        assert!(archive.len() > 110);
    }
}
