// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Stable replay of the committed S20 corpus (ticket S20).
//!
//! Every target is run over its committed corpus files, its built-in seed, and a deterministic
//! mutation sweep, all on stable Rust with no fuzzer runtime. `cargo test -p raven-fuzz-replay`
//! is the gate; the `raven-fuzz` tool runs the longer soak.

use std::fs;
use std::path::{Path, PathBuf};

use raven_fuzz_model::{Target, from_hex, fuzz, run, seed};

/// The committed corpus, next to this package (`tests/fuzz/corpus`).
fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus")
}

fn corpus_files(target: Target) -> Vec<PathBuf> {
    let folder = corpus_root().join(target.name());
    let mut paths: Vec<PathBuf> = match fs::read_dir(&folder) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().and_then(|value| value.to_str()) == Some("hex"))
            .collect(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    paths
}

#[test]
fn every_corpus_directory_is_present() {
    for target in Target::ALL {
        let folder = corpus_root().join(target.name());
        assert!(
            folder.is_dir(),
            "missing corpus directory {}",
            folder.display()
        );
    }
}

#[test]
fn every_corpus_file_decodes_as_hex() {
    for target in Target::ALL {
        let files = corpus_files(target);
        assert!(!files.is_empty(), "no corpus for {}", target.name());
        for path in files {
            let text = fs::read_to_string(&path).expect("corpus file is readable");
            let bytes = from_hex(&text)
                .unwrap_or_else(|error| panic!("{}: bad hex: {error:?}", path.display()));
            assert!(!bytes.is_empty(), "{} is an empty input", path.display());
        }
    }
}

#[test]
fn every_corpus_input_runs_on_every_target() {
    for target in Target::ALL {
        for path in corpus_files(target) {
            let text = fs::read_to_string(&path).expect("corpus file is readable");
            let bytes = from_hex(&text).expect("corpus file is valid hex");
            run(target, &bytes);
        }
    }
}

#[test]
fn the_corpus_and_seed_survive_a_deterministic_sweep() {
    for target in Target::ALL {
        run(target, &seed(target));
        for path in corpus_files(target) {
            let text = fs::read_to_string(&path).expect("corpus file is readable");
            run(target, &from_hex(&text).expect("corpus file is valid hex"));
        }
        assert_eq!(fuzz(target, 0x520, 1_000), 1_000, "{}", target.name());
    }
}

#[test]
fn replay_is_reproducible() {
    // The same seed and iteration count must produce the same sequence of inputs.
    for target in Target::ALL {
        assert_eq!(fuzz(target, 99, 500), 500);
    }
}
