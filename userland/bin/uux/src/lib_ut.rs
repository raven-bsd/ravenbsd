// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `uux` parsing and queueing.

use super::*;

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::{String, ToString};
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
        peers: vec![String::from("peer1")],
    }
}

#[test]
fn parse_options_and_command_words() {
    let parsed = parse_args(["-jz", "-a", "alice", "-g", "x", "peer1!cat", "<", "in"].into_iter())
        .expect("options");
    assert!(parsed.print_id);
    assert!(parsed.mail_on_success);
    assert_eq!(parsed.user.as_deref(), Some("alice"));
    assert_eq!(parsed.grade, b'x');
    assert_eq!(parsed.command, "peer1!cat < in");
}

#[test]
fn command_spec_extracts_files_and_outputs() {
    let spec = parse_command_spec("peer1!cat < in > out >> log").expect("spec");
    assert_eq!(spec.system, "peer1");
    assert_eq!(spec.command, "cat < in > out >> log");
    assert_eq!(
        spec.inputs,
        vec![Input {
            name: String::from("in"),
            stdin: false
        }]
    );
    assert_eq!(spec.outputs, vec!["out".to_string(), "log".to_string()]);
}

#[test]
fn stdin_redirection_is_recognised() {
    assert!(reads_stdin("peer1!cat < -").expect("spec"));
    assert!(!reads_stdin("peer1!cat < in").expect("spec"));
}

#[test]
fn quotes_are_removed_from_words() {
    let spec = parse_command_spec("peer1!sh -c 'cat < in file'").expect("spec");
    assert_eq!(spec.command, "sh -c 'cat < in file'");
    assert_eq!(spec.inputs, Vec::new());
}

#[test]
fn missing_remote_prefix_is_rejected() {
    assert_eq!(parse_command_spec("cat < in"), Err(UuxError::NotRemote));
}

#[test]
fn run_queues_command_with_stdin_and_file() {
    let mut spool = MemSpool::default();
    spool.files.insert(String::from("in"), b"file".to_vec());
    let options = parse_args(["peer1!cat < in > out", "<", "-"].into_iter()).expect("options");
    let id = run(&options, &config(), &mut spool, "alice", b"stdin").expect("queue");
    let jobs = queue::jobs(&mut spool, &config()).expect("jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, id);
    assert_eq!(
        jobs[0].kind,
        JobKind::Command {
            command: String::from("cat < in > out < -"),
            stdin: b"stdin".to_vec(),
            files: vec![uucp::job::NamedFile {
                name: String::from("in"),
                data: b"file".to_vec(),
            }],
            outputs: vec![String::from("out")],
        }
    );
}

#[test]
fn unknown_peer_is_rejected() {
    let options = parse_args(["peer2!cat"].into_iter()).expect("options");
    let mut spool = MemSpool::default();
    assert_eq!(
        run(&options, &config(), &mut spool, "alice", &[]),
        Err(UuxError::UnknownSystem(String::from("peer2")))
    );
}

#[test]
fn missing_input_file_is_rejected() {
    let options = parse_args(["peer1!cat < missing"].into_iter()).expect("options");
    let mut spool = MemSpool::default();
    assert_eq!(
        run(&options, &config(), &mut spool, "alice", &[]),
        Err(UuxError::Spool(SpoolError::NotFound))
    );
}

#[test]
fn missing_command_is_usage() {
    assert_eq!(parse_args([].into_iter()), Err(UsageError::MissingCommand));
}
