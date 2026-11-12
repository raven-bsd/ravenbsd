// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable replay of every committed corpus file.

use std::fs;
use std::path::PathBuf;

use raven_tcpip_fuzz_replay::{TARGETS, replay};

fn corpus_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../fuzz/corpus")
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
            if let Err(panic) = std::panic::catch_unwind(|| replay(target, &data)) {
                panic!("{}: {panic:?}", path.display());
            }
            count += 1;
        }
        assert!(count >= 20, "target {target} has only {count} seeds");
    }
}

#[test]
fn unknown_target_is_rejected() {
    assert!(replay("not_a_target", &[]).is_err());
}

#[test]
fn crash_seed_from_the_segment_sequence_run_replays_clean() {
    let path = corpus_root()
        .join("segment_sequence")
        .join("crash-6155ea7fb68fea620ebfae71395ba13fe91f635d");
    let data = fs::read(&path).expect("crash seed");
    replay("segment_sequence", &data).expect("replay");
}
