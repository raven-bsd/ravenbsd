// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Build the UFS root filesystem used by later image integration.

use std::path::Path;

use anyhow::Result;
use ufs_mkfs::{MkfsOptions, format_image};

/// Create a deterministic UFS2 root image from a staging directory.
pub fn build_ufs_root(staging_dir: &Path, size: u64, out: &Path) -> Result<()> {
    let timestamp = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let options = MkfsOptions {
        size,
        version: 2,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "ravenbsd-root",
        timestamp,
    };
    format_image(out, &options, Some(staging_dir))
        .map_err(|error| anyhow::anyhow!("build UFS root {}: {error}", out.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn builds_a_root_image_from_a_staging_directory() {
        let dir = std::env::temp_dir().join(format!("xtask-rootfs-{}", std::process::id()));
        let staging = dir.join("staging");
        let image = dir.join("root.img");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(staging.join("etc")).expect("staging");
        fs::write(staging.join("etc/hello"), b"hello\n").expect("file");
        build_ufs_root(&staging, 16 * 1024 * 1024, &image).expect("build root");
        let mut ufs = ufs_mkfs::open_read_only(&image).expect("mount");
        let root = ufs.root();
        let etc = ufs.lookup(root, b"etc").expect("etc");
        let hello = ufs.lookup(etc, b"hello").expect("hello");
        let attr = ufs.getattr(hello).expect("attr");
        assert_eq!(attr.size, 6);
        let mut data = [0u8; 6];
        ufs.read(hello, 0, &mut data).expect("read");
        assert_eq!(&data, b"hello\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
