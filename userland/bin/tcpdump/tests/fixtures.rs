// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K277 fixture-oracle tests for the offline decoder.

use std::fs;
use std::path::{Path, PathBuf};

use raven_tcpdump::cli::Options;
use raven_tcpdump::{RunError, Timestamps, execute, pcap};

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../tests/fixtures/tcpdump")
}

fn read_tsv(name: &str) -> Vec<Vec<String>> {
    let text = fs::read_to_string(fixture_dir().join(name))
        .unwrap_or_else(|error| panic!("{name}: {error}"));
    text.lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('|').map(str::to_string).collect())
        .collect()
}

fn read(name: &str) -> Vec<u8> {
    fs::read(fixture_dir().join(name)).unwrap_or_else(|error| panic!("{name}: {error}"))
}

fn options_for(decoder: &str) -> Options {
    Options {
        read: Some(String::from("fixture.pcap")),
        numeric: true,
        verbose: u8::from(decoder.contains("-v")),
        timestamps: Timestamps::Unix,
        ..Options::default()
    }
}

fn decode(name: &str, decoder: &str) -> Vec<String> {
    let bytes = read(name);
    let output =
        execute(&bytes, &options_for(decoder)).unwrap_or_else(|error| panic!("{name}: {error}"));
    output.text.lines().map(str::to_string).collect()
}

#[test]
fn every_manifest_entry_has_the_declared_packet_count_and_link_type() {
    for row in read_tsv("manifest.tsv") {
        let name = &row[0];
        let packets: usize = row[1].parse().expect("packet count");
        let link_type: u32 = match row[2].as_str() {
            "ethernet" => 1,
            other => panic!("{name}: unknown manifest link type {other}"),
        };
        let bytes = read(name);
        let file = pcap::parse(&bytes).unwrap_or_else(|error| panic!("{name}: {error}"));
        assert_eq!(file.link_type, link_type, "{name}");
        assert_eq!(file.packets.len(), packets, "{name}");
    }
}

#[test]
fn every_decode_row_matches_the_reference_output() {
    for row in read_tsv("decode.tsv") {
        let (name, line, expected) = (&row[0], &row[1], &row[2]);
        let decoder = read_tsv("manifest.tsv")
            .into_iter()
            .find(|manifest| &manifest[0] == name)
            .map(|manifest| manifest[4].clone())
            .unwrap_or_default();
        let output = decode(name, &decoder);
        if line == "*" {
            assert!(
                output.iter().any(|actual| actual.contains(expected)),
                "{name}: missing anywhere {expected:?} in {output:#?}"
            );
        } else {
            let index: usize = line.parse().expect("line number");
            let actual = output
                .get(index.saturating_sub(1))
                .unwrap_or_else(|| panic!("{name}: no line {index} in {output:#?}"));
            assert!(
                actual.contains(expected),
                "{name} line {index}: expected {expected:?}, got {actual:?}"
            );
        }
    }
}

#[test]
fn every_filter_row_selects_the_reference_packet_count() {
    for row in read_tsv("filters.tsv") {
        let (name, expression, expected) = (&row[0], &row[1], &row[2]);
        let expected: usize = expected.parse().expect("filter count");
        let bytes = read(name);
        let options = Options {
            read: Some(String::from("fixture.pcap")),
            numeric: true,
            timestamps: Timestamps::Unix,
            filter: Some(expression.clone()),
            ..Options::default()
        };
        let output = execute(&bytes, &options)
            .unwrap_or_else(|error| panic!("{name} {expression}: {error}"));
        assert_eq!(
            output.selected, expected,
            "{name}: filter {expression:?} selected {} packets",
            output.selected
        );
    }
}

#[test]
fn malformed_fixtures_are_rejected_with_the_declared_reason() {
    for row in read_tsv("malformed.tsv") {
        let (name, expected) = (&row[0], &row[1]);
        let error = pcap::parse(&read(name)).unwrap_err();
        assert!(
            error.to_string().contains(expected),
            "{name}: expected {expected:?}, got {error}"
        );
    }
}

#[test]
fn added_negative_fixtures_are_rejected_or_marked() {
    for row in read_tsv("negative/negatives.tsv") {
        let (name, mode, expected) = (&row[0], &row[1], &row[2]);
        if mode == "error" {
            let error = execute(
                &read(&format!("negative/{name}")),
                &options_for("tcpdump -v"),
            )
            .unwrap_err();
            assert!(
                error.to_string().contains(expected),
                "{name}: expected {expected:?}, got {error}"
            );
        } else {
            let output = execute(
                &read(&format!("negative/{name}")),
                &options_for("tcpdump -v"),
            )
            .expect("verbose decode");
            assert!(
                output.text.contains(expected),
                "{name}: expected {expected:?} in {:?}",
                output.text
            );
        }
    }
}

#[test]
fn count_limits_the_selected_packets() {
    let mut options = options_for("tcpdump -nn -tt");
    options.count = Some(3);
    let output = execute(&read("ipv4-tcp-handshake.pcap"), &options).expect("decode");
    assert_eq!(output.selected, 3);
    assert_eq!(output.text.lines().count(), 3);
}

#[test]
fn write_round_trips_the_filtered_packets() {
    let mut options = options_for("tcpdump -nn -tt");
    options.filter = Some(String::from("tcp port 80"));
    options.write = Some(String::from("out.pcap"));
    let output = execute(&read("ipv4-tcp-handshake.pcap"), &options).expect("decode");
    let dump = output.dump.expect("dump requested");
    let file = pcap::parse(&dump).expect("written pcap");
    assert_eq!(file.link_type, pcap::LINKTYPE_ETHERNET);
    assert_eq!(file.packets.len(), output.selected);
    assert_eq!(file.packets.len(), 8);
}

#[test]
fn a_filter_that_must_reject_selects_nothing() {
    let mut options = options_for("tcpdump -nn -tt");
    options.filter = Some(String::from("tcp port 9999"));
    let output = execute(&read("ipv4-tcp-handshake.pcap"), &options).expect("decode");
    assert_eq!(output.selected, 0);
    assert!(output.text.is_empty());
}

#[test]
fn a_bad_filter_is_a_usage_error() {
    let mut options = options_for("tcpdump -nn -tt");
    options.filter = Some(String::from("tcp port nope"));
    assert!(matches!(
        execute(&read("ipv4-tcp-handshake.pcap"), &options),
        Err(RunError::Filter(_))
    ));
}

#[test]
fn live_capture_is_an_explicit_unsupported_path() {
    let options = Options {
        interface: Some(String::from("lo")),
        ..Options::default()
    };
    assert_eq!(
        execute(&[], &options),
        Err(RunError::LiveCaptureUnsupported(String::from("lo")))
    );
}

#[test]
fn fixture_hashes_are_recorded_in_the_manifest() {
    let manifest = read_tsv("manifest.tsv");
    assert!(!manifest.is_empty());
    for row in manifest {
        assert_eq!(row[3].len(), 64, "{} sha256", row[0]);
        assert!(
            row[3]
                .chars()
                .all(|character| character.is_ascii_hexdigit())
        );
    }
}

#[test]
fn fixture_paths_exist() {
    assert!(Path::new(&fixture_dir()).is_dir());
}
