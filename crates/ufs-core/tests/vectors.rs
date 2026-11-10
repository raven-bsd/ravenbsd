// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden-image vector tests and the SHA-256 test helper check.

mod common;

use std::collections::BTreeMap;

use common::{check_manifest, hex, load_manifest, mount_image, sha256, vectors_dir, walk};

fn check_image(name: &str) {
    let mut ufs = mount_image(name);
    let root = ufs.root();
    let mut got = BTreeMap::new();
    walk(&mut ufs, root, "/", &mut got).expect("walk image");
    let manifest = load_manifest(
        &vectors_dir()
            .join("expected")
            .join(format!("{name}.manifest")),
    );
    check_manifest(&got, &manifest).unwrap_or_else(|error| panic!("{name}: {error}"));
}

#[test]
fn golden_ufs1_b4096() {
    check_image("ufs1-b4096");
}

#[test]
fn golden_ufs1_b8192() {
    check_image("ufs1-b8192");
}

#[test]
fn golden_ufs2_b4096() {
    check_image("ufs2-b4096");
}

#[test]
fn golden_ufs2_b8192() {
    check_image("ufs2-b8192");
}

#[test]
fn sha256_empty() {
    assert_eq!(
        hex(&sha256::digest(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn sha256_abc() {
    assert_eq!(
        hex(&sha256::digest(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn sha256_two_blocks() {
    assert_eq!(
        hex(&sha256::digest(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}
