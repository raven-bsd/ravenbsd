// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the uucp spool, queue and copy model.

use super::*;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use crate::config::Config;
use crate::job::{Job, JobKind, NamedFile};
use crate::queue;
use crate::spool::{Spool, SpoolError};
use crate::uucp::{parse_args, run, split_address};

#[derive(Default)]
struct MemSpool {
    files: BTreeMap<String, Vec<u8>>,
    dirs: BTreeSet<String>,
    modes: BTreeMap<String, u32>,
}

impl MemSpool {
    fn add_file(&mut self, path: &str, data: &[u8], mode: u32) {
        self.files.insert(path.to_string(), data.to_vec());
        self.modes.insert(path.to_string(), mode);
    }

    fn add_dir(&mut self, path: &str) {
        self.dirs.insert(path.to_string());
    }
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
            if candidate == path {
                continue;
            }
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
        if path.is_empty() || path == "/" {
            self.dirs.insert(String::from("/"));
            return Ok(());
        }
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
        if let Some(mode) = self.modes.get(path) {
            return Ok(*mode);
        }
        if self.dirs.contains(path) {
            return Ok(0o755);
        }
        Err(SpoolError::NotFound)
    }

    fn set_mode(&mut self, path: &str, mode: u32) -> Result<(), SpoolError> {
        if self.files.contains_key(path) || self.dirs.contains(path) {
            self.modes.insert(path.to_string(), mode);
            Ok(())
        } else {
            Err(SpoolError::NotFound)
        }
    }
}

fn config() -> Config {
    Config {
        local_system: String::from("localhost"),
        spool: String::from("/spool"),
        peers: vec![String::from("peer1")],
    }
}

fn options(args: &[&str]) -> crate::uucp::Options {
    parse_args(args.iter().copied()).expect("options")
}

#[test]
fn parse_local_copy_defaults() {
    let parsed = options(&["a", "b"]);
    assert_eq!(parsed.grade, b'n');
    assert!(!parsed.create_dirs);
    assert!(!parsed.print_id);
    assert_eq!(parsed.operands, vec!["a".to_string(), "b".to_string()]);
}

#[test]
fn parse_options_and_attached_values() {
    let parsed = options(&["-jdn", "alice", "-gx", "-x", "9", "src", "peer1!dst"]);
    assert!(parsed.print_id);
    assert!(parsed.create_dirs);
    assert_eq!(parsed.user.as_deref(), Some("alice"));
    assert_eq!(parsed.grade, b'x');
    assert_eq!(parsed.debug.as_deref(), Some("9"));
    assert_eq!(
        parsed.operands,
        vec!["src".to_string(), "peer1!dst".to_string()]
    );
}

#[test]
fn bad_grade_is_rejected() {
    assert_eq!(
        parse_args(["-g", "ab", "a", "b"].into_iter()),
        Err(crate::uucp::UsageError::BadGrade)
    );
}

#[test]
fn addresses_split_at_the_first_bang() {
    assert_eq!(
        split_address("peer1!/remote/file"),
        Ok(crate::uucp::Address {
            system: Some("peer1"),
            path: "/remote/file"
        })
    );
    assert_eq!(
        split_address("/local/file"),
        Ok(crate::uucp::Address {
            system: None,
            path: "/local/file"
        })
    );
    assert_eq!(
        split_address("!/remote"),
        Err(crate::uucp::UucpError::InvalidAddress)
    );
}

#[test]
fn config_parses_peers_and_spool() {
    let parsed = Config::parse(
        "# comment\nlocal raven\nspool /var/spool/uucp\npeer alice\npeer bob # trailing\n",
    )
    .expect("config");
    assert_eq!(parsed.local_system, "raven");
    assert_eq!(parsed.spool, "/var/spool/uucp");
    assert!(parsed.peer_known("alice"));
    assert!(parsed.peer_known("bob"));
    assert!(!parsed.peer_known("carol"));
}

#[test]
fn config_rejects_a_relative_spool() {
    assert!(Config::parse("spool relative").is_err());
}

#[test]
fn copy_job_round_trips() {
    let job = Job {
        id: 7,
        user: String::from("alice"),
        system: String::from("peer1"),
        grade: b'n',
        kind: JobKind::Copy {
            source: String::from("/src"),
            destination: String::from("/dst"),
            mode: 0o640,
            data: b"hello".to_vec(),
        },
    };
    let encoded = job.encode().expect("encode");
    assert_eq!(Job::decode(&encoded), Ok(job));
}

#[test]
fn command_job_round_trips() {
    let job = Job {
        id: 8,
        user: String::from("bob"),
        system: String::from("peer1"),
        grade: b'x',
        kind: JobKind::Command {
            command: String::from("cat < in > out"),
            stdin: b"stdin".to_vec(),
            files: vec![NamedFile {
                name: String::from("in"),
                data: b"data".to_vec(),
            }],
            outputs: vec![String::from("out")],
        },
    };
    let encoded = job.encode().expect("encode");
    assert_eq!(Job::decode(&encoded), Ok(job));
}

#[test]
fn local_copy_uses_the_spool_seam_and_preserves_mode() {
    let mut spool = MemSpool::default();
    spool.add_dir("/src");
    spool.add_file("/src/a", b"alpha", 0o640);
    let options = options(&["/src/a", "/dst/a"]);
    let ids = run(&options, &config(), &mut spool, "alice").expect("copy");
    assert!(ids.is_empty());
    assert_eq!(spool.read("/dst/a"), Ok(b"alpha".to_vec()));
    assert_eq!(spool.mode("/dst/a"), Ok(0o640));
}

#[test]
fn remote_copy_queues_the_payload() {
    let mut spool = MemSpool::default();
    spool.add_file("/src/a", b"alpha", 0o644);
    let options = options(&["/src/a", "peer1!/remote/a"]);
    let ids = run(&options, &config(), &mut spool, "alice").expect("queue");
    assert_eq!(ids, vec![1]);
    let jobs = queue::jobs(&mut spool, &config()).expect("jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].system, "peer1");
    assert_eq!(
        jobs[0].kind,
        JobKind::Copy {
            source: String::from("/src/a"),
            destination: String::from("/remote/a"),
            mode: 0o644,
            data: b"alpha".to_vec(),
        }
    );
}

#[test]
fn remote_source_queues_a_fetch() {
    let options = options(&["peer1!/remote/a", "/dst/a"]);
    let mut spool = MemSpool::default();
    spool.add_dir("/dst");
    let ids = run(&options, &config(), &mut spool, "alice").expect("queue");
    assert_eq!(ids, vec![1]);
    let job = queue::jobs(&mut spool, &config()).expect("jobs").remove(0);
    assert_eq!(
        job.kind,
        JobKind::Fetch {
            source: String::from("/remote/a"),
            destination: String::from("/dst/a"),
        }
    );
}

#[test]
fn unknown_peer_is_rejected() {
    let options = options(&["/src/a", "carol!/remote/a"]);
    let mut spool = MemSpool::default();
    assert_eq!(
        run(&options, &config(), &mut spool, "alice"),
        Err(crate::uucp::UucpError::UnknownSystem(String::from("carol")))
    );
}

#[test]
fn multiple_local_sources_use_a_directory() {
    let mut spool = MemSpool::default();
    spool.add_file("/src/a", b"a", 0o644);
    spool.add_file("/src/b", b"b", 0o644);
    spool.add_dir("/dst");
    let options = options(&["/src/a", "/src/b", "/dst"]);
    run(&options, &config(), &mut spool, "alice").expect("copy");
    assert_eq!(spool.read("/dst/a"), Ok(b"a".to_vec()));
    assert_eq!(spool.read("/dst/b"), Ok(b"b".to_vec()));
}

#[test]
fn summary_and_cancel_use_the_queue() {
    let mut spool = MemSpool::default();
    spool.add_file("/src/a", b"a", 0o644);
    let options = options(&["/src/a", "peer1!/a"]);
    let ids = run(&options, &config(), &mut spool, "alice").expect("first");
    run(&options, &config(), &mut spool, "bob").expect("second");
    let jobs = queue::jobs(&mut spool, &config()).expect("jobs");
    let summary = queue::summarize(&jobs);
    assert_eq!(summary.jobs, 2);
    assert_eq!(summary.systems, 1);
    assert_eq!(summary.users, 2);
    queue::cancel(&mut spool, &config(), ids[0]).expect("cancel");
    assert_eq!(queue::jobs(&mut spool, &config()).expect("jobs").len(), 1);
}

#[test]
fn malformed_jobs_are_rejected() {
    assert_eq!(Job::decode(b"nope"), Err(crate::job::JobError::BadMagic));
    assert_eq!(
        Job::decode(b"uucp1\n"),
        Err(crate::job::JobError::Truncated)
    );
}
