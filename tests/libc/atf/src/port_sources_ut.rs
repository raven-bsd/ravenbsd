// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-port-sources-{name}-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

#[test]
fn sha256_matches_the_standard_vectors() {
    assert_eq!(
        sha256_hex(b""),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        sha256_hex(b"The quick brown fox jumps over the lazy dog"),
        "d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592"
    );
}

#[test]
fn the_embedded_set_is_the_eighteen_port_sources() {
    assert_eq!(SOURCES.len(), 18);
    let mut paths: Vec<&str> = SOURCES.iter().map(|source| source.path).collect();
    paths.sort_unstable();
    assert!(paths.contains(&"tests/libc/atf/shim/atf_wrapper.c"));
    assert!(paths.contains(&"tests/libc/atf/shim/rl_target_prelude.h"));
    assert!(paths.contains(&"tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_poll.c"));
    assert!(paths.contains(&"tests/libc-oracle/vendor/freebsd/libc-sys/gen/execve/t_execve.c"));
    assert_eq!(
        paths
            .iter()
            .filter(|path| path.starts_with("tests/libc-oracle/vendor/"))
            .count(),
        16
    );
}

#[test]
fn every_embedded_source_matches_its_recorded_hash() {
    for source in SOURCES {
        assert_eq!(
            sha256_hex(source.body.as_bytes()),
            source.sha256,
            "{} does not match its historical blob hash",
            source.path
        );
    }
}

#[test]
fn stage_writes_the_exact_bytes_and_a_build_tree() {
    let out = temp("stage");
    let build = stage(&out).expect("stage");
    let stage_root = out.join(STAGE_DIR);
    for source in SOURCES {
        let exact = stage_root.join(EXACT_DIR).join(source.path);
        assert_eq!(fs::read(&exact).expect("exact"), source.body.as_bytes());
        assert_eq!(sha256_hex(&fs::read(&exact).expect("exact")), source.sha256);
        let built = fs::read(build.join(source.path)).expect("build");
        if source.path == SELECT {
            let text = String::from_utf8(built).expect("utf8");
            assert_eq!(text.matches("pselect child failed").count(), 2);
        } else {
            assert_eq!(built, source.body.as_bytes(), "{} changed", source.path);
        }
    }
    let manifest = fs::read_to_string(stage_root.join("manifest.tsv")).expect("manifest");
    assert_eq!(manifest.lines().count(), SOURCES.len() + 1);
    assert!(manifest.contains(SELECT));
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn stage_refuses_a_source_that_does_not_match_its_hash() {
    let out = temp("tampered");
    let source = Source {
        path: "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_dup.c",
        body: "not the historical blob\n",
        sha256: "0000000000000000000000000000000000000000000000000000000000000000",
    };
    let error = stage_sources(&out, &[source]).expect_err("tampered source");
    assert!(error.to_string().contains("sha256"), "{error}");
    let _ = fs::remove_dir_all(&out);
}

#[test]
fn the_select_transform_adds_only_the_child_status_check() {
    let select = SOURCES
        .iter()
        .find(|source| source.path == SELECT)
        .expect("select source");
    let built = transform(select.path, select.body.as_bytes()).expect("transform");
    let text = String::from_utf8(built).expect("utf8");
    assert_eq!(text.matches(SELECT_CHECKED).count(), 2);
    assert_eq!(text.matches("WIFEXITED(status)").count(), 2);
    assert_eq!(text.matches("atf_tc_fail").count(), 5);
    // Removing the two added checks restores the historical blob byte for byte.
    assert_eq!(text.replace(SELECT_CHECKED, SELECT_DEFAULT), select.body);
}

#[test]
fn a_non_select_source_is_not_transformed() {
    let poll = SOURCES
        .iter()
        .find(|source| source.path == "tests/libc-oracle/vendor/freebsd/libc-sys/sys/t_poll.c")
        .expect("poll source");
    let built = transform(poll.path, poll.body.as_bytes()).expect("transform");
    assert_eq!(built, poll.body.as_bytes());
}
