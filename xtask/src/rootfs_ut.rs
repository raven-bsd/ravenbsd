// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for rootfs spec resolution and Pi 4 staging paths.

use super::*;

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("raven-sd1-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn profile_adjacent_rootfs_spec_is_resolved() {
    let root = temp_root("adjacent");
    let spec = root.join("profiles/pi4-rootfs/rootfs.toml");
    fs::create_dir_all(spec.parent().unwrap()).unwrap();
    fs::write(&spec, "size = 268435456\n").unwrap();
    assert_eq!(spec_path(&root, "pi4-rootfs"), Some(spec));
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn conventional_rootfs_spec_wins_over_the_profile_adjacent_one() {
    let root = temp_root("conventional");
    let conventional = root.join("rootfs/pi4-rootfs.toml");
    let adjacent = root.join("profiles/pi4-rootfs/rootfs.toml");
    fs::create_dir_all(conventional.parent().unwrap()).unwrap();
    fs::create_dir_all(adjacent.parent().unwrap()).unwrap();
    fs::write(&conventional, "size = 268435456\n").unwrap();
    fs::write(&adjacent, "size = 134217728\n").unwrap();
    assert_eq!(spec_path(&root, "pi4-rootfs"), Some(conventional));
    fs::remove_dir_all(&root).unwrap();
}
