// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem checks for `file` classification.

use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::PathBuf;

use raven_file::{Kind, describe};

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-file-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn classifies_real_text_binary_directory_and_symlink() {
    let root = temp("classify");
    let text = root.join("text");
    fs::write(&text, b"hello\n").unwrap();
    let binary = root.join("binary");
    fs::write(&binary, b"a\0b").unwrap();
    let dir = root.join("dir");
    fs::create_dir(&dir).unwrap();
    let link = root.join("link");
    symlink(&text, &link).unwrap();

    let text_mode = fs::symlink_metadata(&text).unwrap().permissions().mode();
    assert_eq!(Kind::from_mode(text_mode), Kind::Regular);
    assert_eq!(
        describe(Kind::Regular, None, b"hello\n", false),
        "ASCII text"
    );

    let binary_mode = fs::symlink_metadata(&binary).unwrap().permissions().mode();
    assert_eq!(Kind::from_mode(binary_mode), Kind::Regular);
    assert_eq!(describe(Kind::Regular, None, b"a\0b", false), "data");

    assert_eq!(
        Kind::from_mode(fs::metadata(&dir).unwrap().permissions().mode()),
        Kind::Directory
    );
    assert_eq!(describe(Kind::Directory, None, &[], false), "directory");

    assert_eq!(
        Kind::from_mode(fs::symlink_metadata(&link).unwrap().permissions().mode()),
        Kind::Symlink
    );
    assert_eq!(
        describe(Kind::Symlink, Some(&text.display().to_string()), &[], false),
        format!("symbolic link to {}", text.display())
    );

    let _ = fs::remove_dir_all(&root);
}

#[test]
fn classifies_a_real_elf_binary() {
    let exe = std::env::current_exe().unwrap();
    let mut sample = [0u8; 4096];
    let mut file = fs::File::open(exe).unwrap();
    let read = std::io::Read::read(&mut file, &mut sample).unwrap();
    let sample = &sample[..read];
    assert!(describe(Kind::Regular, None, sample, false).starts_with("ELF "));
    let mime = describe(Kind::Regular, None, sample, true);
    assert!(
        mime == "application/x-executable" || mime == "application/x-sharedlib",
        "{mime}"
    );
}
