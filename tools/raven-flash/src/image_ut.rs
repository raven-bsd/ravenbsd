// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use std::io::Write;

fn temp_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "raven-flash-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn image_inspection_reports_size_and_hash() {
    let dir = temp_dir("image");
    let path = dir.join("disk.img");
    let mut file = File::create(&path).unwrap();
    file.write_all(b"raven image").unwrap();
    drop(file);
    let info = inspect(&path).unwrap();
    assert_eq!(info.size, 11);
    assert_eq!(info.sha256.len(), 64);
    assert!(info.sha256.chars().all(|value| value.is_ascii_hexdigit()));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn image_inspection_reads_a_build_id() {
    let dir = temp_dir("build-id");
    let path = dir.join("disk.img");
    std::fs::write(&path, b"x").unwrap();
    std::fs::write(dir.join("manifest.txt"), "commit=abc123\n").unwrap();
    let info = inspect(&path).unwrap();
    assert_eq!(info.build_id.as_deref(), Some("abc123"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn non_img_and_empty_images_are_refused() {
    let dir = temp_dir("refuse");
    let text = dir.join("disk.bin");
    std::fs::write(&text, b"x").unwrap();
    assert!(inspect(&text).is_err());
    let empty = dir.join("empty.img");
    std::fs::write(&empty, b"").unwrap();
    assert!(inspect(&empty).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
