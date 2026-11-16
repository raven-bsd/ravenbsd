// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ustar archive model.

use super::{
    Entry, Operation, TarError, UsageError, encode_archive, parse_archive, parse_args, safe_path,
    split_name,
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
fn parses_create_list_extract() {
    let options = parse_args(["-c".into(), "-f".into(), "a.tar".into(), "src".into()]).unwrap();
    assert_eq!(options.operation, Operation::Create);
    assert_eq!(options.archive, "a.tar");
    assert_eq!(options.operands, vec!["src"]);
    assert_eq!(
        parse_args(["-tf".into(), "a.tar".into()])
            .unwrap()
            .operation,
        Operation::List
    );
    assert_eq!(
        parse_args(["-xvf".into(), "a.tar".into()])
            .unwrap()
            .operation,
        Operation::Extract
    );
    assert_eq!(
        parse_args(["-czf".into(), "a.tar".into()]).unwrap_err(),
        UsageError::UnsupportedCompression
    );
}

#[test]
fn regular_directory_and_symlink_round_trip() {
    let mut link = entry("tree/link", b'2', b"");
    link.linkname = "target".into();
    let entries = vec![
        entry("tree/", b'5', b""),
        entry("tree/file.txt", b'0', b"hello\n"),
        link,
    ];
    let archive = encode_archive(&entries).unwrap();
    assert_eq!(parse_archive(&archive).unwrap(), entries);
}

#[test]
fn checksum_and_magic_are_checked() {
    let mut archive = encode_archive(&[entry("file", b'0', b"x")]).unwrap();
    archive[0] ^= 1;
    assert_eq!(parse_archive(&archive), Err(TarError::Checksum));
    let mut archive = encode_archive(&[entry("file", b'0', b"x")]).unwrap();
    archive[257] = b'X';
    fix_checksum(&mut archive);
    assert_eq!(parse_archive(&archive), Err(TarError::BadMagic));
}

#[test]
fn name_prefix_splitting_and_limits() {
    let long = format!("{}/file", "a".repeat(100));
    let (name, prefix) = split_name(&long).unwrap();
    assert_eq!(name, "file");
    assert_eq!(prefix, "a".repeat(100));
    let too_long = "a".repeat(300);
    assert_eq!(split_name(&too_long), Err(TarError::NameTooLong));
    assert!(safe_path("tree/file"));
    assert!(!safe_path("/absolute"));
    assert!(!safe_path("tree/../escape"));
}

#[test]
fn unsupported_types_are_rejected() {
    let mut archive = encode_archive(&[entry("file", b'0', b"x")]).unwrap();
    archive[156] = b'9';
    fix_checksum(&mut archive);
    assert_eq!(
        parse_archive(&archive),
        Err(TarError::UnsupportedType(b'9'))
    );
}

fn fix_checksum(archive: &mut [u8]) {
    let header = &mut archive[..512];
    for byte in &mut header[148..156] {
        *byte = b' ';
    }
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let text = format!("{checksum:06o}");
    header[148..154].copy_from_slice(text.as_bytes());
    header[154] = 0;
    header[155] = b' ';
}
