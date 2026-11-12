// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the UFS root staging, GPT wrapping and initfs insertion.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

use xtask::{build_root_disk, build_ufs_root, inject_initfs_file, stage_rootfs_spec};

static NEXT_TREE: AtomicUsize = AtomicUsize::new(0);

struct Tree {
    root: PathBuf,
}

impl Tree {
    fn new() -> Self {
        let id = NEXT_TREE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("raven-rootfs-{}-{id}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create fixture root");
        Self { root }
    }

    fn write(&self, relative: &str, contents: &[u8]) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, contents).expect("write fixture");
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn spec(marker: bool) -> String {
    let mut text = String::from(
        "size = 16777216\n\
         [[files]]\n\
         src = \"rootfs/hw/etc/hostname\"\n\
         path = \"/etc/hostname\"\n",
    );
    if marker {
        text.push_str(
            "[[files]]\n\
             src = \"rootfs/hw/etc/services.d/root-marker.toml\"\n\
             path = \"/etc/services.d/root-marker.toml\"\n",
        );
    }
    text
}

#[test]
fn builds_a_root_image_from_a_staging_directory() {
    let tree = Tree::new();
    let staging = tree.root.join("staging");
    let image = tree.root.join("root.img");
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
}

#[test]
fn removing_a_root_manifest_removes_it_from_the_next_root_image() {
    let tree = Tree::new();
    tree.write("rootfs/hw/etc/hostname", b"k33-root\n");
    tree.write(
        "rootfs/hw/etc/services.d/root-marker.toml",
        b"name = \"root-marker\"\n",
    );
    let with_marker = tree.root.join("with.toml");
    fs::write(&with_marker, spec(true)).expect("spec");
    let staging_a = tree.root.join("staging-a");
    stage_rootfs_spec(&tree.root, &with_marker, &staging_a).expect("stage with marker");
    assert!(staging_a.join("etc/services.d/root-marker.toml").is_file());
    let image_a = tree.root.join("root-a.ufs");
    build_ufs_root(&staging_a, 16 * 1024 * 1024, &image_a).expect("build image a");
    let mut ufs = ufs_mkfs::open_read_only(&image_a).expect("mount a");
    let root = ufs.root();
    let etc = ufs.lookup(root, b"etc").expect("etc");
    let mut services = None;
    ufs.readdir(etc, 0, &mut |entry, _| {
        if entry.name == b"services.d" {
            services = Some(entry.ino);
        }
        true
    })
    .expect("readdir etc");
    let services = services.expect("services.d directory");
    let mut marker = false;
    ufs.readdir(services, 0, &mut |entry, _| {
        if entry.name == b"root-marker.toml" {
            marker = true;
        }
        true
    })
    .expect("readdir services.d");
    assert!(marker);

    let without_marker = tree.root.join("without.toml");
    fs::write(&without_marker, spec(false)).expect("spec");
    let staging_b = tree.root.join("staging-b");
    stage_rootfs_spec(&tree.root, &without_marker, &staging_b).expect("stage without marker");
    assert!(!staging_b.join("etc/services.d/root-marker.toml").exists());
    let image_b = tree.root.join("root-b.ufs");
    build_ufs_root(&staging_b, 16 * 1024 * 1024, &image_b).expect("build image b");
    let mut ufs = ufs_mkfs::open_read_only(&image_b).expect("mount b");
    let root = ufs.root();
    let etc = ufs.lookup(root, b"etc").expect("etc");
    let mut services = None;
    ufs.readdir(etc, 0, &mut |entry, _| {
        if entry.name == b"services.d" {
            services = Some(entry.ino);
        }
        true
    })
    .expect("readdir etc");
    let mut marker = false;
    if let Some(services) = services {
        ufs.readdir(services, 0, &mut |entry, _| {
            if entry.name == b"root-marker.toml" {
                marker = true;
            }
            true
        })
        .expect("readdir services.d");
    }
    assert!(!marker);
}

#[test]
fn staged_file_data_roundtrips_through_the_ufs_image() {
    let tree = Tree::new();
    let staging = tree.root.join("staging");
    fs::create_dir_all(staging.join("sbin")).expect("staging");
    let mut sizes = vec![20_904usize, 37_288, 45_480, 82_344, 1_772_368];
    for size in sizes.drain(..) {
        let data: Vec<u8> = (0..size).map(|index| (index % 251) as u8).collect();
        fs::write(staging.join("sbin/file"), &data).expect("file");
        let image = tree.root.join("file.ufs");
        build_ufs_root(&staging, 8 * 1024 * 1024, &image).expect("ufs");
        let mut ufs = ufs_mkfs::open_read_only(&image).expect("mount");
        let root = ufs.root();
        let sbin = ufs.lookup(root, b"sbin").expect("sbin");
        let file = ufs.lookup(sbin, b"file").expect("file");
        let attr = ufs.getattr(file).expect("attr");
        assert_eq!(attr.size as usize, size);
        let mut read = vec![0u8; size];
        ufs.read(file, 0, &mut read).expect("read");
        assert_eq!(read, data, "size {size}");
    }
}

#[test]
fn root_disk_wraps_the_ufs_payload_at_lba_34() {
    let tree = Tree::new();
    let staging = tree.root.join("staging");
    fs::create_dir_all(&staging).expect("staging");
    fs::write(staging.join("hostname"), b"k33-root\n").expect("file");
    let ufs_path = tree.root.join("root.ufs");
    build_ufs_root(&staging, 4 * 1024 * 1024, &ufs_path).expect("ufs");
    let disk_path = tree.root.join("blockd.img");
    build_root_disk(&ufs_path, &disk_path).expect("disk");

    let ufs = fs::read(&ufs_path).expect("read ufs");
    let disk = fs::read(&disk_path).expect("read disk");
    assert_eq!(&disk[512..520], b"EFI PART");
    let payload = 34 * 512;
    assert_eq!(&disk[payload..payload + ufs.len()], &ufs[..]);
    let expected = (34 + ufs.len() / 512 + 33) * 512;
    assert_eq!(disk.len(), expected);
}

#[test]
fn injected_file_is_visible_in_the_repacked_initfs() {
    let mut builder = raven_initfs::InitfsBuilder::new();
    builder.add_file("/etc/a", b"a\n", 0o644).expect("a");
    let image = builder.build();
    let image = inject_initfs_file(&image, "/etc/blockd.img", b"disk", 0o444).expect("inject");
    let parsed = raven_initfs::Initfs::parse(&image).expect("parse");
    assert_eq!(parsed.get("/etc/a").expect("a").data, b"a\n");
    let disk = parsed.get("/etc/blockd.img").expect("disk");
    assert_eq!(disk.data, b"disk");
    assert_eq!(disk.mode, 0o444);
}

#[test]
fn injecting_a_duplicate_path_is_rejected() {
    let mut builder = raven_initfs::InitfsBuilder::new();
    builder.add_file("/etc/a", b"a\n", 0o644).expect("a");
    let image = builder.build();
    assert!(inject_initfs_file(&image, "/etc/a", b"b", 0o644).is_err());
}

#[test]
fn a_program_rootfs_spec_needs_the_full_build_path() {
    let tree = Tree::new();
    let spec = tree.root.join("spec.toml");
    fs::write(
        &spec,
        "size = 1048576\n[[programs]]\npackage = \"raven-uname\"\npath = \"/sbin/uname\"\n",
    )
    .expect("spec");
    let staging = tree.root.join("staging");
    assert!(stage_rootfs_spec(&tree.root, &spec, &staging).is_err());
}
