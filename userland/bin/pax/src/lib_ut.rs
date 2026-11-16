// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pax extended-header model.

use super::{
    Entry, Error, Operation, UsageError, encode_archive, parse_archive, parse_args, safe_path,
};

fn entry(name: &str, kind: u8, data: &[u8]) -> Entry {
    Entry {
        name: name.into(),
        mode: 0o644,
        uid: 0,
        gid: 0,
        mtime: 1_700_000_000,
        kind,
        linkname: String::new(),
        data: data.to_vec(),
    }
}

#[test]
fn parses_write_append_read_extract_list() {
    let options = parse_args(["-w".into(), "-f".into(), "a.pax".into(), "src".into()]).unwrap();
    assert_eq!(options.operation, Operation::Write);
    assert_eq!(options.archive, "a.pax");
    assert_eq!(options.operands, vec!["src"]);
    assert_eq!(
        parse_args(["-af".into(), "a.pax".into(), "new".into()])
            .unwrap()
            .operation,
        Operation::Append
    );
    assert_eq!(
        parse_args(["-rf".into(), "a.pax".into()])
            .unwrap()
            .operation,
        Operation::Read
    );
    assert_eq!(
        parse_args(["-xf".into(), "a.pax".into()])
            .unwrap()
            .operation,
        Operation::Extract
    );
    assert_eq!(
        parse_args(["-tf".into(), "a.pax".into()])
            .unwrap()
            .operation,
        Operation::List
    );
    assert_eq!(
        parse_args(["-wzf".into(), "a.pax".into(), "src".into()]).unwrap_err(),
        UsageError::UnsupportedCompression
    );
}

#[test]
fn long_paths_and_link_targets_use_extended_headers() {
    let long = format!("{}/file", "a".repeat(300));
    let mut link = entry("tree/link", b'2', b"");
    link.linkname = "b".repeat(150);
    let entries = vec![entry("tree/", b'5', b""), entry(&long, b'0', b"data"), link];
    let archive = encode_archive(&entries).unwrap();
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed, entries);
    assert!(parsed[1].name.len() > 100);
    assert!(parsed[2].linkname.len() > 100);
}

#[test]
fn extended_size_record_supplies_the_data_length() {
    let records = super::pax_record("size", "5");
    let mut archive = Vec::new();
    let header = super::base_header(
        "PaxHeaders/0",
        "",
        0,
        0,
        0,
        0o644,
        records.len() as u64,
        b'x',
        "",
    )
    .unwrap();
    archive.extend_from_slice(&header);
    archive.extend_from_slice(records.as_bytes());
    super::pad(&mut archive, records.len());
    archive
        .extend_from_slice(&super::base_header("file", "", 0, 0, 0, 0o644, 0, b'0', "").unwrap());
    archive.extend_from_slice(b"hello");
    super::pad(&mut archive, 5);
    archive.extend(std::iter::repeat(0u8).take(1024));
    let parsed = parse_archive(&archive).unwrap();
    assert_eq!(parsed[0].name, "file");
    assert_eq!(parsed[0].data, b"hello");
}

#[test]
fn corrupt_records_and_checksums_are_rejected() {
    let archive = encode_archive(&[entry("file", b'0', b"x")]).unwrap();
    let mut bad = archive.clone();
    bad[0] ^= 1;
    assert_eq!(parse_archive(&bad), Err(Error::Checksum));
    let mut bad = archive;
    bad[257] = b'X';
    assert!(parse_archive(&bad).is_err());
}

#[test]
fn safe_paths_are_checked() {
    assert!(safe_path("tree/file"));
    assert!(!safe_path("/absolute"));
    assert!(!safe_path("tree/../escape"));
}
