// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ufs-fsck` command-line tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ufs_core::mkfs::{MkfsOptions, format};
use ufs_core::{BlockIo, IoError};

const MIB: u64 = 1024 * 1024;
const STAMP: i64 = 1_767_225_600;

struct VecDisk {
    data: Vec<u8>,
}

impl BlockIo for VecDisk {
    fn size_bytes(&self) -> u64 {
        self.data.len() as u64
    }

    fn read_at(&mut self, offset: u64, buf: &mut [u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let src = self.data.get(start..end).ok_or(IoError)?;
        buf.copy_from_slice(src);
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buf: &[u8]) -> Result<(), IoError> {
        let start = usize::try_from(offset).map_err(|_| IoError)?;
        let end = start.checked_add(buf.len()).ok_or(IoError)?;
        let dst = self.data.get_mut(start..end).ok_or(IoError)?;
        dst.copy_from_slice(buf);
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        Ok(())
    }
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ufs-fsck-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn build_image(path: &Path) {
    let size = 16 * MIB;
    let options = MkfsOptions {
        size,
        version: 2,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k57",
        timestamp: STAMP,
    };
    let disk = VecDisk {
        data: vec![0u8; size as usize],
    };
    let mut ufs = format(disk, &options).expect("format");
    let file = ufs
        .create(ufs.root(), b"file", 0o644, 0, 0)
        .expect("create");
    ufs.write(file, 0, b"hello\n").expect("write");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");
    fs::write(path, &disk.data).expect("write image");
}

fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().expect("u32"))
}

fn inode_offset(data: &[u8], ino: u32) -> usize {
    let base = 8192usize;
    let ipg = read_u32(data, base + 184);
    let fpg = read_u32(data, base + 188);
    let inopb = read_u32(data, base + 120);
    let iblkno = i32::from_le_bytes(data[base + 16..base + 20].try_into().expect("iblkno"));
    let fsize = read_u32(data, base + 52);
    let cg = ino / ipg;
    let inum = ino % ipg;
    let block = inum / inopb;
    let slot = inum % inopb;
    let frag = u64::from(cg) * u64::from(fpg) + iblkno as u64 + u64::from(block) * 8;
    (frag * u64::from(fsize) + u64::from(slot) * 256) as usize
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ufs-fsck"))
        .args(args)
        .output()
        .expect("run ufs-fsck")
}

#[test]
fn clean_image_exits_zero() {
    let dir = scratch("clean");
    let image = dir.join("clean.img");
    build_image(&image);
    let output = run(&["-n", image.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("clean"));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn check_only_reports_and_does_not_write() {
    let dir = scratch("readonly");
    let image = dir.join("bad.img");
    build_image(&image);
    let mut data = fs::read(&image).expect("read");
    let off = inode_offset(&data, 3);
    data[off + 2..off + 4].copy_from_slice(&9i16.to_le_bytes());
    fs::write(&image, &data).expect("write");
    let before = fs::read(&image).expect("read");
    let output = run(&["-n", image.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(4));
    assert_eq!(fs::read(&image).expect("read"), before);
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn preen_repairs_and_a_second_run_is_clean() {
    let dir = scratch("preen");
    let image = dir.join("bad.img");
    build_image(&image);
    let mut data = fs::read(&image).expect("read");
    let off = inode_offset(&data, 3);
    data[off + 2..off + 4].copy_from_slice(&9i16.to_le_bytes());
    fs::write(&image, &data).expect("write");
    let repaired = run(&["-p", image.to_str().expect("path")]);
    assert_eq!(repaired.status.code(), Some(1));
    let second = run(&["-n", image.to_str().expect("path")]);
    assert_eq!(second.status.code(), Some(0));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn unrepairable_image_exits_four() {
    let dir = scratch("bad");
    let image = dir.join("garbage.img");
    fs::write(&image, vec![0u8; 2 * MIB as usize]).expect("write");
    let output = run(&["-n", image.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(4));
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn usage_errors_exit_eight() {
    let missing = run(&["/nonexistent/ufs-image.img"]);
    assert_eq!(missing.status.code(), Some(8));
    let conflict = run(&["-n", "-p", "image"]);
    assert_eq!(conflict.status.code(), Some(8));
}

#[test]
fn help_exits_zero() {
    let output = run(&["--help"]);
    assert_eq!(output.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&output.stdout).contains("usage:"));
}

#[test]
fn clean_image_preen_is_zero() {
    let dir = scratch("clean-preen");
    let image = dir.join("clean.img");
    build_image(&image);
    let output = run(&["-p", image.to_str().expect("path")]);
    assert_eq!(output.status.code(), Some(0));
    let _ = fs::remove_dir_all(&dir);
}
