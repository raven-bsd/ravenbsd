// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the uucp spool and queue model.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use uucp::config::Config;
use uucp::job::JobKind;
use uucp::queue;
use uucp::spool::{Spool, SpoolError};
use uucp::{parse_args, run};

struct StdSpool;

fn map(error: io::Error) -> SpoolError {
    if error.kind() == io::ErrorKind::NotFound {
        SpoolError::NotFound
    } else {
        SpoolError::Io
    }
}

impl Spool for StdSpool {
    fn read(&mut self, path: &str) -> Result<Vec<u8>, SpoolError> {
        fs::read(path).map_err(map)
    }

    fn write(&mut self, path: &str, data: &[u8]) -> Result<(), SpoolError> {
        fs::write(path, data).map_err(map)
    }

    fn remove(&mut self, path: &str) -> Result<(), SpoolError> {
        fs::remove_file(path).map_err(map)
    }

    fn list(&mut self, path: &str) -> Result<Vec<String>, SpoolError> {
        let entries = fs::read_dir(path).map_err(map)?;
        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(map)?;
            names.push(entry.file_name().to_string_lossy().into_owned());
        }
        Ok(names)
    }

    fn create_dir_all(&mut self, path: &str) -> Result<(), SpoolError> {
        fs::create_dir_all(path).map_err(map)
    }

    fn is_dir(&mut self, path: &str) -> Result<bool, SpoolError> {
        fs::metadata(path)
            .map(|metadata| metadata.is_dir())
            .map_err(map)
    }

    fn exists(&mut self, path: &str) -> bool {
        Path::new(path).exists()
    }

    fn mode(&mut self, path: &str) -> Result<u32, SpoolError> {
        fs::metadata(path)
            .map(|metadata| metadata.permissions().mode() & 0o7777)
            .map_err(map)
    }

    fn set_mode(&mut self, path: &str, mode: u32) -> Result<(), SpoolError> {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o7777)).map_err(map)
    }
}

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-uucp-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("temp");
    path
}

fn config(root: &Path) -> Config {
    Config {
        local_system: "localhost".to_string(),
        spool: root.join("spool").to_string_lossy().into_owned(),
        peers: vec!["peer1".to_string()],
    }
}

#[test]
fn local_copy_and_remote_queue_round_trip() {
    let root = temp("round-trip");
    let sources = root.join("src");
    let target = root.join("dst");
    fs::create_dir_all(&sources).expect("sources");
    fs::create_dir_all(&target).expect("target");
    let source = sources.join("a");
    fs::write(&source, b"alpha").expect("source");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o640)).expect("mode");
    let config = config(&root);
    let mut spool = StdSpool;

    let local_source = source.to_string_lossy().into_owned();
    let local_target = target.join("a").to_string_lossy().into_owned();
    let local = parse_args([local_source.as_str(), local_target.as_str()].into_iter())
        .expect("local options");
    let ids = run(&local, &config, &mut spool, "alice").expect("local copy");
    assert!(ids.is_empty());
    assert_eq!(fs::read(target.join("a")).expect("copied"), b"alpha");
    assert_eq!(
        fs::metadata(target.join("a"))
            .expect("mode")
            .permissions()
            .mode()
            & 0o777,
        0o640
    );

    let remote_source = source.to_string_lossy().into_owned();
    let remote = parse_args([remote_source.as_str(), "peer1!/remote/a"].into_iter())
        .expect("remote options");
    let ids = run(&remote, &config, &mut spool, "alice").expect("queue");
    assert_eq!(ids.len(), 1);
    let jobs = queue::jobs(&mut spool, &config).expect("jobs");
    assert_eq!(jobs.len(), 1);
    assert_eq!(
        jobs[0].kind,
        JobKind::Copy {
            source: remote_source,
            destination: "/remote/a".to_string(),
            mode: 0o640,
            data: b"alpha".to_vec(),
        }
    );
    assert!(root.join("spool/queue/1.job").is_file());
    queue::cancel(&mut spool, &config, ids[0]).expect("cancel");
    assert!(queue::jobs(&mut spool, &config).expect("jobs").is_empty());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn unknown_peer_is_reported_without_a_job() {
    let root = temp("unknown");
    let config = config(&root);
    let mut spool = StdSpool;
    let options = parse_args(["peer2!/missing", "/dst"].into_iter()).expect("options");
    assert_eq!(
        run(&options, &config, &mut spool, "alice"),
        Err(uucp::UucpError::UnknownSystem("peer2".to_string()))
    );
    let _ = fs::remove_dir_all(root);
}
