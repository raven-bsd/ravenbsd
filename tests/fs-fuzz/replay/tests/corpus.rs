// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable replay of every committed filesystem-fuzz corpus file.

use std::fs;
use std::path::PathBuf;

use raven_fs_fuzz_replay::{TARGETS, replay};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../corpus")
}

#[test]
fn every_target_has_a_corpus_and_replays_clean() {
    let root = corpus_root();
    for target in TARGETS {
        let directory = root.join(target);
        assert!(directory.is_dir(), "missing corpus {}", directory.display());
        let mut count = 0;
        for entry in fs::read_dir(&directory).expect("read corpus") {
            let path = entry.expect("entry").path();
            if !path.is_file() {
                continue;
            }
            let data = fs::read(&path).expect("read seed");
            if let Err(error) = replay(target, &data) {
                panic!("{}: {error}", path.display());
            }
            count += 1;
        }
        assert!(count >= 8, "target {target} has only {count} seeds");
    }
}

#[test]
fn unknown_target_is_rejected() {
    assert!(replay("not_a_target", &[]).is_err());
}

#[test]
fn a_one_byte_superblock_mutation_replays_clean() {
    let mut data = vec![0u8; 1376];
    data[0] = 0xff;
    assert!(replay("ufs_superblock", &data).is_ok());
}
