// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic 2,000-mutation fuzz seed over a golden image.

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{SharedDisk, load_gz, vectors_dir};
use ufs_core::{MountOptions, Ufs};

fn u32le(data: &[u8], off: usize) -> u32 {
    let mut value = [0u8; 4];
    value.copy_from_slice(&data[off..off + 4]);
    u32::from_le_bytes(value)
}

fn i32le(data: &[u8], off: usize) -> i32 {
    let mut value = [0u8; 4];
    value.copy_from_slice(&data[off..off + 4]);
    i32::from_le_bytes(value)
}

fn u64le(data: &[u8], off: usize) -> u64 {
    let mut value = [0u8; 8];
    value.copy_from_slice(&data[off..off + 8]);
    u64::from_le_bytes(value)
}

#[test]
fn two_thousand_deterministic_mutations_do_not_panic() {
    let base = load_gz(&vectors_dir().join("golden/ufs2-b4096.img.gz"));
    let sb = 8192usize;
    let fsize = u32le(&base, sb + 52) as usize;
    let iblkno = i32le(&base, sb + 16) as usize;
    let root_ino = iblkno * fsize + 2 * 256;
    let root_dir = u64le(&base, root_ino + 112) as usize * fsize;
    assert!(
        root_dir + 512 <= base.len(),
        "root directory block is in range"
    );

    let regions = [(sb, 1376usize), (root_ino, 256usize), (root_dir, 512usize)];
    let disk = SharedDisk::new(base);
    let opts = MountOptions { read_only: true };

    for index in 0..2000usize {
        let (start, len) = regions[index % regions.len()];
        let offset = start + (index * 37 + 11) % len;
        let original = disk.data.borrow()[offset];
        disk.data.borrow_mut()[offset] ^= 0xa5;

        let mount_disk = disk.clone();
        let result = catch_unwind(AssertUnwindSafe(|| {
            if let Ok(mut ufs) = Ufs::mount(mount_disk, opts) {
                let _ = ufs.getattr(2);
                let _ = ufs.lookup(2, b"empty");
                let mut seen = 0usize;
                let _ = ufs.readdir(2, 0, &mut |_entry, _cookie| {
                    seen += 1;
                    seen < 8
                });
                let _ = ufs.read(3, 0, &mut [0u8; 64]);
            }
        }));
        disk.data.borrow_mut()[offset] = original;

        assert!(result.is_ok(), "panic after mutating byte {offset}");
        assert_eq!(
            disk.bad_reads(),
            0,
            "out-of-range read after mutating byte {offset}"
        );
    }
}
