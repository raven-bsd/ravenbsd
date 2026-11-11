// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end `ufs-mkfs` tests: formats, population, reproducibility and geometry.

use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use ufs_core::mkfs::MkfsOptions;
use ufs_core::{FileType, Ino, MountOptions, Ufs};
use ufs_mkfs::{FileDisk, format_image, open_read_only};

const MIB: u64 = 1024 * 1024;
const STAMP: i64 = 1_767_225_600;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ufs-mkfs-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn options(size: u64, version: u8) -> MkfsOptions<'static> {
    MkfsOptions {
        size,
        version,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k56",
        timestamp: STAMP,
    }
}

fn read_i32(data: &[u8], off: usize) -> i32 {
    i32::from_le_bytes(data[off..off + 4].try_into().expect("i32"))
}

fn assert_root_and_lost_found(path: &Path) {
    let mut ufs = open_read_only(path).expect("read-only mount");
    let root = ufs.root();
    let attr = ufs.getattr(root).expect("root attr");
    assert_eq!(attr.kind, FileType::Directory);
    assert_eq!(attr.nlink, 3);
    let lost = ufs.lookup(root, b"lost+found").expect("lost+found");
    let lost = ufs.getattr(lost).expect("lost attr");
    assert_eq!(lost.kind, FileType::Directory);
    assert_eq!(lost.mode, 0o700);
}

#[test]
fn formats_ufs2_at_three_sizes() {
    let dir = scratch("sizes");
    for size in [16 * MIB, 64 * MIB, 512 * MIB] {
        let path = dir.join(format!("ufs2-{size}.img"));
        let opts = options(size, 2);
        format_image(&path, &opts, None).expect("format");
        assert_eq!(fs::metadata(&path).expect("stat").len(), size);
        assert_root_and_lost_found(&path);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn formats_ufs1_at_three_sizes() {
    let dir = scratch("ufs1-sizes");
    for size in [16 * MIB, 64 * MIB, 512 * MIB] {
        let path = dir.join(format!("ufs1-{size}.img"));
        format_image(&path, &options(size, 1), None).expect("format");
        assert_root_and_lost_found(&path);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn two_runs_are_identical() {
    let dir = scratch("repro");
    let first = dir.join("first.img");
    let second = dir.join("second.img");
    let opts = options(16 * MIB, 2);
    format_image(&first, &opts, None).expect("first");
    format_image(&second, &opts, None).expect("second");
    assert_eq!(
        fs::read(&first).expect("first bytes"),
        fs::read(&second).expect("second bytes")
    );
    let _ = fs::remove_dir_all(&dir);
}

fn make_host_tree(root: &Path) {
    fs::create_dir_all(root.join("etc")).expect("etc");
    fs::create_dir_all(root.join("sbin")).expect("sbin");
    for index in 0..500 {
        let dir = if index % 2 == 0 {
            root.join("etc")
        } else {
            root.join("sbin")
        };
        let path = dir.join(format!("file-{index:04}.txt"));
        let body = format!("payload-{index}\n").repeat(1 + index % 17);
        fs::write(&path, body).expect("host file");
        let mode = if index % 5 == 0 { 0o755 } else { 0o644 };
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("chmod");
    }
    std::os::unix::fs::symlink("etc/file-0000.txt", root.join("link")).expect("symlink");
    fs::set_permissions(root.join("etc"), fs::Permissions::from_mode(0o755)).expect("chmod dir");
}

fn walk_image(ufs: &mut Ufs<FileDisk>, dir: Ino, host: &Path, root: bool) -> Result<(), String> {
    let mut image_entries = Vec::new();
    ufs.readdir(dir, 0, &mut |entry, _| {
        if entry.name != b"." && entry.name != b".." {
            image_entries.push(entry.name.clone());
        }
        true
    })
    .map_err(|error| format!("readdir {}: {error}", host.display()))?;
    image_entries.sort();

    let mut host_entries = Vec::new();
    for entry in fs::read_dir(host).map_err(|error| format!("readdir host: {error}"))? {
        let entry = entry.map_err(|error| format!("readdir host: {error}"))?;
        host_entries.push(entry.file_name().as_os_str().as_bytes().to_vec());
    }
    host_entries.sort();

    for name in &host_entries {
        if !image_entries.iter().any(|entry| entry == name) {
            return Err(format!(
                "missing {} under {}",
                String::from_utf8_lossy(name),
                host.display()
            ));
        }
    }
    for name in &image_entries {
        if root && name == b"lost+found" {
            continue;
        }
        if !host_entries.iter().any(|entry| entry == name) {
            return Err(format!(
                "extra image entry {} under {}",
                String::from_utf8_lossy(name),
                host.display()
            ));
        }
        let host_path = host.join(String::from_utf8_lossy(name).as_ref());
        let ino = ufs
            .lookup(dir, name)
            .map_err(|error| format!("lookup: {error}"))?;
        let attr = ufs
            .getattr(ino)
            .map_err(|error| format!("getattr: {error}"))?;
        let metadata =
            fs::symlink_metadata(&host_path).map_err(|error| format!("host stat: {error}"))?;
        if attr.mode != (metadata.permissions().mode() & 0o7777) as u16 {
            return Err(format!("mode differs for {}", host_path.display()));
        }
        if attr.uid != 0 || attr.gid != 0 {
            return Err(format!("owner differs for {}", host_path.display()));
        }
        if metadata.is_dir() {
            if attr.kind != FileType::Directory {
                return Err(format!(
                    "{} is not a directory in the image",
                    host_path.display()
                ));
            }
            walk_image(ufs, ino, &host_path, false)?;
        } else if metadata.is_file() {
            if attr.kind != FileType::Regular {
                return Err(format!(
                    "{} is not a regular file in the image",
                    host_path.display()
                ));
            }
            let want = fs::read(&host_path).map_err(|error| format!("host read: {error}"))?;
            let mut got = vec![0u8; want.len()];
            ufs.read(ino, 0, &mut got)
                .map_err(|error| format!("image read: {error}"))?;
            if got != want {
                return Err(format!("content differs for {}", host_path.display()));
            }
        } else if metadata.is_symlink() {
            if attr.kind != FileType::Symlink {
                return Err(format!(
                    "{} is not a symlink in the image",
                    host_path.display()
                ));
            }
            let mut target = vec![0u8; attr.size as usize];
            let got = ufs
                .readlink(ino, &mut target)
                .map_err(|error| format!("readlink: {error}"))?;
            if got != target.len()
                || target
                    != fs::read_link(&host_path)
                        .expect("host link")
                        .as_os_str()
                        .as_encoded_bytes()
            {
                return Err(format!(
                    "symlink target differs for {}",
                    host_path.display()
                ));
            }
        }
    }
    Ok(())
}

#[test]
fn populates_500_files_and_compares_through_reader() {
    let dir = scratch("populate");
    let host = dir.join("tree");
    fs::create_dir_all(&host).expect("host tree");
    make_host_tree(&host);
    let image = dir.join("populated.img");
    format_image(&image, &options(64 * MIB, 2), Some(&host)).expect("format+populate");
    let mut ufs = open_read_only(&image).expect("mount");
    let root = ufs.root();
    walk_image(&mut ufs, root, &host, true).expect("tree comparison");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn geometry_matches_makefs_determined_fields() {
    let dir = scratch("geometry");
    let image = dir.join("geometry.img");
    format_image(&image, &options(16 * MIB, 2), None).expect("format");
    let generated = fs::read(&image).expect("generated bytes");
    let golden_gz = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/ufs-vectors/golden/ufs2-b4096.img.gz");
    let output = Command::new("gzip")
        .arg("-dc")
        .arg(&golden_gz)
        .output()
        .expect("gzip golden");
    assert!(output.status.success());
    let golden = output.stdout;
    let base = 8192;
    for (name, off, width) in [
        ("magic", 1372usize, 4usize),
        ("bsize", 48, 4),
        ("fsize", 52, 4),
        ("frag", 56, 4),
        ("sblkno", 8, 4),
        ("cblkno", 12, 4),
        ("iblkno", 16, 4),
        ("sbsize", 104, 4),
        ("maxcontig", 88, 4),
        ("maxbpg", 92, 4),
        ("maxbsize", 860, 4),
        ("sblockloc", 1000, 4),
        ("nindir", 116, 4),
        ("inopb", 120, 4),
        ("cgsize", 160, 4),
        ("ncg", 44, 4),
    ] {
        let want = if width == 4 {
            read_i32(&golden, base + off) as u32
        } else {
            0
        };
        let got = read_i32(&generated, base + off) as u32;
        assert_eq!(got, want, "{name}");
    }
    println!(
        "geometry: generated ipg={} fpg={} dblkno={}; golden ipg={} fpg={} dblkno={} (density-dependent)",
        read_i32(&generated, base + 184),
        read_i32(&generated, base + 188),
        read_i32(&generated, base + 20),
        read_i32(&golden, base + 184),
        read_i32(&golden, base + 188),
        read_i32(&golden, base + 20),
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn cli_formats_an_image() {
    let dir = scratch("cli");
    let image = dir.join("cli.img");
    let status = Command::new(env!("CARGO_BIN_EXE_ufs-mkfs"))
        .args([
            "-O",
            "2",
            "-b",
            "4096",
            "-f",
            "512",
            "-s",
            "16m",
            "-T",
            "1767225600",
        ])
        .arg(&image)
        .status()
        .expect("run ufs-mkfs");
    assert!(status.success());
    assert_root_and_lost_found(&image);
    let mut ufs = Ufs::mount(
        FileDisk::open(&image).expect("open"),
        MountOptions { read_only: false },
    )
    .expect("mount");
    ufs.verify_allocation().expect("verify");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn oracle_fsck_accepts_generated_images_when_present() {
    let Some(binary) = std::env::var_os("UFS_FSCK_FFS")
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            let dir = PathBuf::from(std::env::var_os("UFS_ORACLE_DIR")?);
            [dir.join("bin/fsck_ffs"), dir.join("fsck_ffs")]
                .into_iter()
                .find(|path| path.is_file())
        })
    else {
        println!("ORACLE ABSENT: fsck_ffs not installed; skipped");
        return;
    };
    let dir = scratch("oracle");
    let image = dir.join("oracle.img");
    format_image(&image, &options(16 * MIB, 2), None).expect("format");
    let output = Command::new(binary)
        .arg("-n")
        .arg(&image)
        .output()
        .expect("fsck_ffs");
    let _ = fs::remove_dir_all(&dir);
    assert!(
        output.status.success(),
        "fsck_ffs failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
