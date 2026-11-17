// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `uustat` parsing and queue output.

use super::*;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use uucp::config::Config;
use uucp::job::JobKind;
use uucp::queue;
use uucp::spool::{Spool, SpoolError};

#[derive(Default)]
struct MemSpool {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
}

impl Spool for MemSpool {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, SpoolError> {
        self.files.get(path).cloned().ok_or(SpoolError::NotFound)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), SpoolError> {
        self.files.insert(path.to_string(), data.to_vec());
        Ok(())
    }

    fn remove(&mut self, path: &str) -> Result<(), SpoolError> {
        self.files
            .remove(path)
            .map(|_| ())
            .ok_or(SpoolError::NotFound)
    }

    fn list(&mut self, path: &str) -> Result<Vec<String>, SpoolError> {
        if !self.dirs.contains(path) {
            return Err(SpoolError::NotFound);
        }
        let prefix = if path == "/" {
            String::from("/")
        } else {
            format!("{}/", path.trim_end_matches('/'))
        };
        let mut names = BTreeSet::new();
        for candidate in self.files.keys().chain(self.dirs.iter()) {
            if let Some(rest) = candidate.strip_prefix(&prefix)
                && let Some(name) = rest.split('/').next()
                && !name.is_empty()
            {
                names.insert(name.to_string());
            }
        }
        Ok(names.into_iter().collect())
    }

    fn create_dir_all(&mut self, path: &str) -> Result<(), SpoolError> {
        let mut current = String::new();
        for part in path.split('/') {
            if part.is_empty() {
                continue;
            }
            if current.is_empty() {
                if path.starts_with('/') {
                    current.push('/');
                }
            } else if !current.ends_with('/') {
                current.push('/');
            }
            current.push_str(part);
            self.dirs.insert(current.clone());
        }
        Ok(())
    }

    fn is_dir(&mut self, path: &str) -> Result<bool, SpoolError> {
        if self.dirs.contains(path) {
            Ok(true)
        } else if self.files.contains_key(path) {
            Ok(false)
        } else {
            Err(SpoolError::NotFound)
        }
    }

    fn exists(&mut self, path: &str) -> bool {
        self.files.contains_key(path) || self.dirs.contains(path)
    }

    fn mode(&mut self, path: &str) -> Result<u32, SpoolError> {
        if self.files.contains_key(path) {
            Ok(0o644)
        } else if self.dirs.contains(path) {
            Ok(0o755)
        } else {
            Err(SpoolError::NotFound)
        }
    }

    fn set_mode(&mut self, _path: &str, _mode: u32) -> Result<(), SpoolError> {
        Ok(())
    }
}

fn config() -> Config {
    Config {
        local_system: String::from("localhost"),
        spool: String::from("/spool"),
        peers: vec![String::from("peer1"), String::from("peer2")],
    }
}

fn seed(spool: &mut MemSpool) -> (u64, u64) {
    let first = queue::enqueue(
        spool,
        &config(),
        "alice",
        "peer1",
        b'n',
        JobKind::Copy {
            source: String::from("/a"),
            destination: String::from("/b"),
            mode: 0o644,
            data: vec![1],
        },
    )
    .expect("first");
    let second = queue::enqueue(
        spool,
        &config(),
        "bob",
        "peer2",
        b'x',
        JobKind::Fetch {
            source: String::from("/c"),
            destination: String::from("/d"),
        },
    )
    .expect("second");
    (first.id, second.id)
}

#[test]
fn parse_filters_and_cancel() {
    let parsed = parse_args(["-a", "-q", "-s", "peer1"].into_iter()).expect("parsed");
    assert!(parsed.all);
    assert!(parsed.summary);
    assert_eq!(parsed.system.as_deref(), Some("peer1"));
    let cancel = parse_args(["-k", "12"].into_iter()).expect("cancel");
    assert_eq!(cancel.cancel, Some(12));
}

#[test]
fn parse_rejects_extra_operands_and_all_with_user() {
    assert_eq!(parse_args(["x"].into_iter()), Err(UsageError::ExtraOperand));
    assert_eq!(
        parse_args(["-a", "-u", "alice"].into_iter()),
        Err(UsageError::AllAndUser)
    );
    assert_eq!(parse_args(["-k", "x"].into_iter()), Err(UsageError::BadJob));
}

#[test]
fn default_lists_only_the_current_user() {
    let mut spool = MemSpool::default();
    seed(&mut spool);
    let options = parse_args([].into_iter()).expect("options");
    let mut out = String::new();
    run(&options, &config(), &mut spool, "alice", &mut out).expect("run");
    assert!(out.contains("alice"), "{out}");
    assert!(!out.contains("bob"), "{out}");
}

#[test]
fn all_lists_every_user_and_summary_counts() {
    let mut spool = MemSpool::default();
    seed(&mut spool);
    let options = parse_args(["-a"].into_iter()).expect("options");
    let mut out = String::new();
    run(&options, &config(), &mut spool, "alice", &mut out).expect("run");
    assert!(out.contains("alice"), "{out}");
    assert!(out.contains("bob"), "{out}");

    let summary = parse_args(["-a", "-q"].into_iter()).expect("summary");
    let mut text = String::new();
    run(&summary, &config(), &mut spool, "alice", &mut text).expect("summary run");
    assert_eq!(text, "jobs 2 systems 2 users 2\n");
}

#[test]
fn system_filter_selects_one_peer() {
    let mut spool = MemSpool::default();
    seed(&mut spool);
    let options = parse_args(["-a", "-s", "peer2"].into_iter()).expect("options");
    let mut out = String::new();
    run(&options, &config(), &mut spool, "alice", &mut out).expect("run");
    assert!(out.contains("peer2"), "{out}");
    assert!(!out.contains("peer1"), "{out}");
}

#[test]
fn cancel_removes_the_job() {
    let mut spool = MemSpool::default();
    let (first, _) = seed(&mut spool);
    let options = parse_args(["-k", &first.to_string()].into_iter()).expect("options");
    let mut out = String::new();
    run(&options, &config(), &mut spool, "alice", &mut out).expect("cancel");
    assert_eq!(out, format!("job {first} cancelled\n"));
    assert_eq!(queue::jobs(&mut spool, &config()).expect("jobs").len(), 1);
}

#[test]
fn cancel_missing_job_is_an_error() {
    let mut spool = MemSpool::default();
    let options = parse_args(["-k", "99"].into_iter()).expect("options");
    let mut out = String::new();
    assert_eq!(
        run(&options, &config(), &mut spool, "alice", &mut out),
        Err(UustatError::Queue(uucp::queue::QueueError::NotFound))
    );
}
