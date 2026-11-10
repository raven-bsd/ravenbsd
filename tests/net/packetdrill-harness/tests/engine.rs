// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end checks of the harness engine.

use std::fs;
use std::path::Path;
use std::time::Duration;

use raven_netcore_packetdrill_harness::{Runner, parse};

const TIMEOUT: Duration = Duration::from_secs(10);

fn run(text: &str) -> anyhow::Result<()> {
    let script = parse("inline.pkt", text)?;
    Runner::new(script).run(TIMEOUT)
}

#[test]
fn every_checked_in_script_passes() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../scripts");
    let mut entries: Vec<_> = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", directory.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "pkt"))
        .collect();
    entries.sort();
    assert!(
        entries.len() >= 6,
        "expected the six script subset, found {} in {}",
        entries.len(),
        directory.display()
    );
    for path in entries {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = fs::read_to_string(&path).expect("script readable");
        let script = parse(&name, &text).unwrap_or_else(|error| panic!("{name}: {error:#}"));
        Runner::new(script)
            .run(TIMEOUT)
            .unwrap_or_else(|error| panic!("{name}: {error:#}"));
    }
}

#[test]
fn a_wrong_expected_ack_fails() {
    let error = run("0.000 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
         +0.000 bind(3, ..., ...) = 0\n\
         +0.000 listen(3, 1) = 0\n\
         +0.000 < S 0:0(0) win 65535\n\
         +0.000 > S. 0:0(0) ack 2\n")
    .expect_err("the wrong ack must not match");
    let text = format!("{error:#}");
    assert!(text.contains("expected outgoing"), "{text}");
}

#[test]
fn an_unexpected_segment_fails() {
    let error = run("0.000 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
         +0.000 bind(3, ..., ...) = 0\n\
         +0.000 listen(3, 1) = 0\n\
         +0.100 < S 0:0(0) win 65535\n\
         +0.000 > S. 0:0(0) ack 1\n\
         +0.100 < . 1:1(0) ack 1 win 65535\n\
         +0.000 accept(3, ..., ...) = 4\n\
         +0.100 write(4, ..., 10) = 10\n\
         +0.000 > . 1:11(10) ack 1\n")
    .expect_err("a data expectation with no ACK flag must fail");
    let text = format!("{error:#}");
    assert!(text.contains("expected outgoing"), "{text}");
}

#[test]
fn parser_rejects_unknown_syscalls() {
    assert!(parse("x", "0.000 frobnicate(1) = 0\n").is_err());
}

#[test]
fn parser_rejects_bad_time() {
    assert!(parse("x", "+-1 < S 0:0(0) win 1\n").is_err());
}
