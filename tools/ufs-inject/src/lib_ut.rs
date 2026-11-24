// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the UFS in-place writer.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use ufs_core::mkfs::MkfsOptions;

use super::*;

static NEXT: AtomicU64 = AtomicU64::new(1);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("ufs-inject-{tag}-{}-{id}", std::process::id()));
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

fn image(path: &Path) -> Image {
    Image::new(path.to_path_buf(), 0, 0)
}

#[test]
fn put_replaces_and_reads_back_with_fsck() {
    let dir = TempDir::new("put");
    let image_path = fresh_image(&dir);
    let host = dir.join("hello.txt");
    fs::write(&host, b"hello\n").expect("host file");
    let source = image(&image_path);

    let first = put(
        &source,
        &host,
        "/sbin/hello",
        &PutOptions {
            mode: Some(0o755),
            uid: Some(0),
            gid: Some(0),
            mtime: None,
            parents: true,
        },
    )
    .expect("put");
    assert!(!first.replaced);
    assert_eq!(first.bytes, 6);
    assert!(first.report.clean_after);
    assert_eq!(cat(&source, "/sbin/hello").expect("cat"), b"hello\n");
    let attr = stat(&source, "/sbin/hello").expect("stat");
    assert_eq!(attr.kind, FileType::Regular);
    assert_eq!(attr.mode, 0o755);
    assert_eq!((attr.uid, attr.gid), (0, 0));
    assert_eq!(attr.size, 6);

    fs::write(&host, b"a longer replacement payload\n").expect("replace file");
    let second = put(&source, &host, "/sbin/hello", &PutOptions::default()).expect("replace");
    assert!(second.replaced);
    assert_eq!(
        cat(&source, "/sbin/hello").expect("cat replaced"),
        b"a longer replacement payload\n"
    );

    fs::write(&host, b"x").expect("shorter file");
    put(&source, &host, "/sbin/hello", &PutOptions::default()).expect("shorten");
    assert_eq!(cat(&source, "/sbin/hello").expect("cat short"), b"x");
    assert!(fsck(&source).expect("fsck").clean_after);
}

#[test]
fn mkdir_and_put_with_parents() {
    let dir = TempDir::new("parents");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("tool");
    fs::write(&host, b"binary\n").expect("host file");

    mkdir(
        &source,
        "/opt/app/bin",
        &MkdirOptions {
            mode: Some(0o750),
            uid: Some(0),
            gid: Some(0),
            mtime: None,
            parents: true,
        },
    )
    .expect("mkdir -p");
    let attr = stat(&source, "/opt/app/bin").expect("stat dir");
    assert_eq!(attr.kind, FileType::Directory);
    assert_eq!(attr.mode, 0o750);

    put(
        &source,
        &host,
        "/opt/app/bin/tool",
        &PutOptions {
            mode: Some(0o755),
            uid: Some(0),
            gid: Some(0),
            mtime: None,
            parents: true,
        },
    )
    .expect("put parents");
    assert_eq!(
        list(&source, "/opt/app/bin").expect("list"),
        vec!["tool".to_string()]
    );
    assert_eq!(cat(&source, "/opt/app/bin/tool").expect("cat"), b"binary\n");
    assert!(fsck(&source).expect("fsck").clean_after);
}

#[test]
fn missing_parent_is_rejected_without_parents() {
    let dir = TempDir::new("missing");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("file");
    fs::write(&host, b"data").expect("host file");
    let error = put(&source, &host, "/no/such/dir/file", &PutOptions::default())
        .expect_err("missing parent");
    assert!(
        error.contains("not found") || error.contains("lookup"),
        "{error}"
    );
}

#[test]
fn non_zero_offset_updates_only_the_ufs_slice() {
    let dir = TempDir::new("offset");
    let ufs_path = fresh_image(&dir);
    let ufs = fs::read(&ufs_path).expect("read ufs");
    let prefix_len = 1024 * 1024usize;
    let container = dir.join("disk.img");
    let mut disk = vec![0xa5u8; prefix_len];
    disk.extend_from_slice(&ufs);
    fs::write(&container, &disk).expect("container");
    let source = Image::new(&container, prefix_len as u64, ufs.len() as u64);
    let host = dir.join("offset.txt");
    fs::write(&host, b"slice-data\n").expect("host file");

    put(&source, &host, "/slice.txt", &PutOptions::default()).expect("put slice");
    assert_eq!(
        cat(&source, "/slice.txt").expect("cat slice"),
        b"slice-data\n"
    );
    assert!(fsck(&source).expect("fsck slice").clean_after);
    let after = fs::read(&container).expect("read container");
    assert_eq!(&after[..16], &[0xa5u8; 16]);
}

#[test]
fn path_traversal_is_rejected() {
    let dir = TempDir::new("traversal");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("file");
    fs::write(&host, b"data").expect("host file");
    let error = put(&source, &host, "/a/../b", &PutOptions::default()).expect_err("traversal");
    assert!(error.contains(".."), "{error}");
}

#[test]
fn symlink_can_be_created_and_replaced() {
    let dir = TempDir::new("symlink");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("target");
    fs::write(&host, b"target-data\n").expect("host file");
    put(
        &source,
        &host,
        "/bin/target",
        &PutOptions {
            parents: true,
            ..PutOptions::default()
        },
    )
    .expect("put target");
    symlink(
        &source,
        "/bin/target",
        "/bin/link",
        &SymlinkOptions::default(),
    )
    .expect("symlink");
    assert_eq!(
        cat(&source, "/bin/link").expect("cat link"),
        b"target-data\n"
    );
    symlink(
        &source,
        "/bin/target",
        "/bin/link",
        &SymlinkOptions::default(),
    )
    .expect("replace symlink");
    assert_eq!(
        cat(&source, "/bin/link").expect("cat link 2"),
        b"target-data\n"
    );
    assert!(fsck(&source).expect("fsck").clean_after);
}

#[test]
fn fresh_image_is_clean_and_listable() {
    let dir = TempDir::new("fresh");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    assert!(fsck(&source).expect("fsck").clean_after);
    let root = list(&source, "/").expect("list root");
    assert!(root.iter().any(|name| name == "lost+found"), "{root:?}");
}

#[test]
fn repair_keeps_a_clean_image_clean() {
    let dir = TempDir::new("repair");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let report = repair(&source).expect("repair");
    assert!(report.clean_after);
    assert!(report.findings.is_empty());
    assert!(fsck(&source).expect("fsck after repair").clean_after);
}

#[test]
fn remove_file_and_recursive_tree_keep_fsck_clean() {
    let dir = TempDir::new("remove");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("payload");
    fs::write(&host, b"keep\n").expect("host file");
    put(
        &source,
        &host,
        "/rm/file",
        &PutOptions {
            parents: true,
            ..PutOptions::default()
        },
    )
    .expect("put file");
    mkdir(
        &source,
        "/rm/sub",
        &MkdirOptions {
            parents: true,
            ..MkdirOptions::default()
        },
    )
    .expect("mkdir sub");
    put(&source, &host, "/rm/sub/child", &PutOptions::default()).expect("put child");
    symlink(&source, "/rm/file", "/rm/link", &SymlinkOptions::default()).expect("symlink");

    remove(&source, "/rm/file", false).expect("rm file");
    remove(&source, "/rm/link", false).expect("rm symlink");
    assert!(remove(&source, "/rm", false).is_err(), "non-empty rmdir");
    remove(&source, "/rm", true).expect("rm -r");
    assert!(
        list(&source, "/")
            .expect("list")
            .iter()
            .all(|name| name != "rm")
    );
    assert!(fsck(&source).expect("fsck").clean_after);
}

#[test]
fn put_preserves_host_mtime_and_accepts_an_override() {
    use std::time::{Duration, UNIX_EPOCH};

    let dir = TempDir::new("mtime");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("stamp.txt");
    fs::write(&host, b"stamp\n").expect("host file");
    let stamp = UNIX_EPOCH + Duration::from_secs(1_700_000_123);
    let handle = fs::OpenOptions::new()
        .write(true)
        .open(&host)
        .expect("open host");
    handle
        .set_times(fs::FileTimes::new().set_modified(stamp))
        .expect("set mtime");
    drop(handle);

    put(&source, &host, "/stamp.txt", &PutOptions::default()).expect("put host mtime");
    assert_eq!(
        stat(&source, "/stamp.txt").expect("stat").mtime.sec,
        1_700_000_123
    );

    put(
        &source,
        &host,
        "/stamp.txt",
        &PutOptions {
            mtime: Some(Timespec {
                sec: 1_600_000_000,
                nsec: 42,
            }),
            ..PutOptions::default()
        },
    )
    .expect("put override mtime");
    let attr = stat(&source, "/stamp.txt").expect("stat override");
    assert_eq!(attr.mtime.sec, 1_600_000_000);
    assert_eq!(attr.mtime.nsec, 42);
}

#[test]
fn host_mode_is_preserved_without_an_override() {
    let dir = TempDir::new("mode");
    let image_path = fresh_image(&dir);
    let source = image(&image_path);
    let host = dir.join("mode.txt");
    fs::write(&host, b"mode").expect("host file");
    let mut permissions = fs::metadata(&host).expect("metadata").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut permissions, 0o640);
    fs::set_permissions(&host, permissions).expect("chmod");
    put(&source, &host, "/mode.txt", &PutOptions::default()).expect("put mode");
    assert_eq!(stat(&source, "/mode.txt").expect("stat").mode, 0o640);
}
