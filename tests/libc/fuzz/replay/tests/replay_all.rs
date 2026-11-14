// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Replay every corpus and regression file, then the deterministic 2,000-input sweep.
//!
//! The sweep size is `RL_FUZZ_SWEEP` (default 2000) so a Miri run can lower it. A panic in a body
//! is a finding: it fails this test and is reported, never silenced.

use std::fs;
use std::path::{Path, PathBuf};

use raven_libc_fuzz_replay::rng::Rng;
use raven_libc_fuzz_replay::{TARGETS, alloc_probe, replay_file, run};

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn sweep_size() -> usize {
    std::env::var("RL_FUZZ_SWEEP")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2000)
}

fn files(directory: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(directory) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

#[test]
fn every_target_has_a_corpus_and_replays_clean() {
    let root = root();
    let corpus_root = root.join("corpus");
    let regression_root = root.join("regressions");
    for (name, _) in TARGETS {
        let directory = corpus_root.join(name);
        assert!(directory.is_dir(), "missing corpus {}", directory.display());
        let seeds = files(&directory);
        assert!(
            seeds.len() >= 10,
            "target {name} has only {} seeds",
            seeds.len()
        );
        assert!(seeds.len() <= 300, "target {name} has more than 300 seeds");
        let mut total = 0usize;
        for path in &seeds {
            let data = fs::read(path).expect("read seed");
            assert!(
                data.len() <= 1024,
                "{} is larger than 1 KiB",
                path.display()
            );
            total = total.saturating_add(data.len());
            if let Err(error) = replay_file(name, path) {
                panic!("{}: {error}", path.display());
            }
        }
        assert!(total < 2 << 20, "target {name} corpus is larger than 2 MiB");

        let regressions = regression_root.join(name);
        for path in files(&regressions) {
            if path.extension().and_then(|value| value.to_str()) == Some("bin") {
                let note = path.with_extension("md");
                assert!(note.is_file(), "{} has no .md note", path.display());
                if let Err(error) = replay_file(name, &path) {
                    panic!("{}: {error}", path.display());
                }
            }
        }
    }
}

#[test]
fn the_deterministic_sweep_is_clean_and_reproducible() {
    let sweep = sweep_size();
    let mut rng = Rng::seed_from_u64(0x5241_564e);
    for (name, _) in TARGETS {
        for _ in 0..sweep {
            let length = usize::try_from(rng.below(513)).unwrap_or(0);
            let data = rng.bytes(length);
            let first = std::panic::catch_unwind(|| {
                alloc_probe::clear_note();
                run(name, &data);
                alloc_probe::last_note()
            })
            .unwrap_or_else(|payload| {
                let message = payload
                    .downcast_ref::<&str>()
                    .copied()
                    .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
                    .unwrap_or("non-string panic");
                panic!("{name}: {message}");
            });
            let second = std::panic::catch_unwind(|| {
                alloc_probe::clear_note();
                run(name, &data);
                alloc_probe::last_note()
            })
            .unwrap_or_else(|_| panic!("{name}: the second run panicked"));
            assert_eq!(
                first, second,
                "P-determinism: {name} produced different notes"
            );
        }
    }
}

#[test]
fn four_byte_flips_of_every_seed_are_clean() {
    let root = root().join("corpus");
    let mut rng = Rng::seed_from_u64(0x5241_564f);
    for (name, _) in TARGETS {
        for path in files(&root.join(name)) {
            let data = fs::read(&path).expect("read seed");
            for _ in 0..4 {
                let mut mutated = data.clone();
                if !mutated.is_empty() {
                    let index = usize::try_from(rng.below(mutated.len() as u64)).unwrap_or(0);
                    if let Some(slot) = mutated.get_mut(index) {
                        *slot ^= 1 << (rng.below(8) as u8);
                    }
                }
                if let Err(payload) = std::panic::catch_unwind(|| run(name, &mutated)) {
                    let message = payload
                        .downcast_ref::<&str>()
                        .copied()
                        .unwrap_or("non-string panic");
                    panic!("{}: {message}", path.display());
                }
            }
        }
    }
}

#[test]
fn an_unknown_target_is_a_harness_bug() {
    let result = std::panic::catch_unwind(|| run("not_a_target", &[]));
    assert!(result.is_err());
}

#[test]
fn replay_file_reports_a_missing_file() {
    let path = Path::new("/nonexistent/c70/seed");
    assert!(replay_file("printf_format", path).is_err());
}
