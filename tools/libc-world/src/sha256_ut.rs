// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_file(name: &str, data: &[u8]) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "libc-world-sha-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, data).expect("write");
    path
}

#[test]
fn empty_string_vector() {
    assert_eq!(
        hex_digest(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
}

#[test]
fn abc_vector() {
    assert_eq!(
        hex_digest(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn two_block_vector() {
    assert_eq!(
        hex_digest(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn long_message_vector() {
    let data = vec![b'a'; 1_000_000];
    assert_eq!(
        hex_digest(&data),
        "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
    );
}

#[test]
fn incremental_updates_match_one_shot() {
    let data = b"the quick brown fox jumps over the lazy dog";
    let mut state = Sha256::new();
    for chunk in data.chunks(7) {
        state.update(chunk);
    }
    assert_eq!(state.finish(), digest(data));
}

#[test]
fn buffer_boundaries_match_one_shot() {
    for size in [0usize, 1, 55, 56, 63, 64, 65, 119, 120, 128] {
        let data = vec![0x5au8; size];
        let first = digest(&data);
        let mut state = Sha256::new();
        state.update(&data[..size / 2]);
        state.update(&data[size / 2..]);
        assert_eq!(state.finish(), first, "size {size}");
    }
}

#[test]
fn hex_is_lowercase_and_fixed_width() {
    let text = hex(&[0u8; 32]);
    assert_eq!(text.len(), 64);
    assert_eq!(text, "0".repeat(64));
    assert!(hex_digest(b"x").chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn digest_matches_sha256sum_when_available() {
    let available = Command::new("sha256sum").arg("--version").output();
    if !available.is_ok_and(|output| output.status.success()) {
        eprintln!("SKIP: sha256sum absent");
        return;
    }
    let data: Vec<u8> = (0..10_000u32).map(|value| (value % 251) as u8).collect();
    let path = temp_file("stream", &data);
    let output = Command::new("sha256sum").arg(&path).output().expect("run");
    let _ = std::fs::remove_file(&path);
    let text = String::from_utf8_lossy(&output.stdout);
    let expected = text.split_whitespace().next().expect("digest");
    assert_eq!(hex_digest(&data), expected);
}
