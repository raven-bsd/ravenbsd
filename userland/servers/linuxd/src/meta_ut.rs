// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the Linux stat/statx metadata layout.

use raven_abi::types::Handle;

use super::*;
use crate::rootfs::{RootEntry, RootTree};
use crate::table::LinuxProcess;

fn process() -> LinuxProcess {
    let mut process = LinuxProcess::new(
        1,
        1,
        Handle(1),
        Handle(2),
        0x40_0000,
        0x50_0000,
        0x60_0000,
        0x70_0000,
    );
    process.set_identity(b"/compat/linux/bin/busybox", vec![b"busybox".to_vec()]);
    process.set_root(RootTree::new(&[RootEntry {
        path: b"/compat/linux/bin/busybox".to_vec(),
        size: 804_616,
        mode: 0o755,
    }]));
    process
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("u32"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("u64"))
}

#[test]
fn stat_has_the_x86_64_layout_and_offsets() {
    let meta = Meta::file(1234, 42);
    let bytes = stat_bytes(&meta);
    assert_eq!(bytes.len(), 144);
    assert_eq!(read_u64(&bytes, 0), 0); // st_dev
    assert_eq!(read_u64(&bytes, 8), 42); // st_ino
    assert_eq!(read_u64(&bytes, 16), 1); // st_nlink
    assert_eq!(read_u32(&bytes, 24), meta.mode);
    assert_eq!(read_u64(&bytes, 48), 1234); // st_size
    assert_eq!(read_u64(&bytes, 56), 4096); // st_blksize
    assert_eq!(read_u64(&bytes, 64), 3); // st_blocks (1234 -> 3)
}

#[test]
fn statx_reports_the_requested_basic_mask() {
    let meta = Meta::directory(7);
    let bytes = statx_bytes(&meta, 0x7ff);
    assert_eq!(bytes.len(), 256);
    assert_eq!(read_u32(&bytes, 0), 0x7ff);
    assert_eq!(read_u32(&bytes, 4), 4096);
    assert_eq!(read_u32(&bytes, 16), 2); // nlink
    assert_eq!(read_u32(&bytes, 28), meta.mode & 0xffff);
    assert_eq!(read_u64(&bytes, 32), 7); // ino
    assert_eq!(read_u64(&bytes, 40), 4096); // size
}

#[test]
fn path_metadata_comes_from_the_staged_tree() {
    let process = process();
    let meta = path_meta(&process, b"/compat/linux/bin/busybox").expect("busybox");
    assert_eq!(meta.size, 804_616);
    assert_eq!(meta.mode & 0o170000, super::S_IFREG);
    assert_eq!(meta.mode & 0o777, 0o755);
    let dir = path_meta(&process, b"/compat/linux").expect("dir");
    assert_eq!(dir.mode & 0o170000, super::S_IFDIR);
    assert!(path_meta(&process, b"/compat/linux/nope").is_err());
}

#[test]
fn synthetic_files_and_standard_descriptors_have_metadata() {
    let process = process();
    let status = path_meta(&process, b"/proc/self/status").expect("status");
    assert_eq!(status.mode & 0o170000, super::S_IFREG);
    assert!(status.size > 0);
    let stdio = file_meta(&process, 1).expect("stdout");
    assert_eq!(stdio.mode & 0o170000, super::S_IFCHR);
    assert!(file_meta(&process, 9).is_err());
}
