// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::device::Mount;

fn temp_dir(label: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "raven-flash-write-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn image(path: &Path, bytes: &[u8]) -> ImageInfo {
    std::fs::write(path, bytes).unwrap();
    ImageInfo {
        path: path.to_path_buf(),
        size: bytes.len() as u64,
        sha256: crate::image::sha256_file(path).unwrap(),
        build_id: None,
    }
}

fn device(size: u64) -> Device {
    Device {
        name: "sdb".to_string(),
        path: "/dev/sdb".into(),
        model: "test".to_string(),
        serial: "serial".to_string(),
        size_bytes: size,
        transport: "usb".to_string(),
        removable: true,
        mounted: Vec::new(),
        holders: Vec::new(),
        is_root: false,
    }
}

#[test]
fn writes_and_verifies_by_readback() {
    let dir = temp_dir("ok");
    let image = image(&dir.join("disk.img"), b"raven-flash payload");
    let target = dir.join("target");
    std::fs::write(&target, b"").unwrap();
    let mut seen = Vec::new();
    let report = write_and_verify(&image, &target, true, false, &mut |done, total| {
        seen.push((done, total));
    })
    .unwrap();
    assert!(report.verified);
    assert_eq!(report.bytes, image.size);
    assert_eq!(report.target_sha256, image.sha256);
    assert_eq!(seen.last(), Some(&(image.size, image.size)));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn validation_refuses_root_mounted_holders_and_oversize() {
    let dir = temp_dir("validate");
    let image = image(&dir.join("disk.img"), b"1234");
    let mut target = device(16);
    assert!(validate_pair(&image, &target).is_ok());
    target.is_root = true;
    assert!(validate_pair(&image, &target).is_err());
    target.is_root = false;
    target.mounted.push(Mount {
        source: "/dev/sdb1".to_string(),
        target: "/mnt/card".to_string(),
        fstype: "vfat".to_string(),
    });
    assert!(validate_pair(&image, &target).is_err());
    target.mounted.clear();
    target.holders.push("dm-0".to_string());
    assert!(validate_pair(&image, &target).is_err());
    target.holders.clear();
    assert!(validate_pair(&image, &device(2)).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
