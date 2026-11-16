// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for `file` classification and parsing.

use super::{Kind, describe, line, parse};

#[test]
fn parse_brief_and_mime() {
    let options = parse(["-b", "-i", "a", "--", "-x"]).unwrap();
    assert!(options.brief);
    assert!(options.mime);
    assert_eq!(options.operands, vec!["a", "-x"]);
}

#[test]
fn unknown_option_is_rejected() {
    assert!(parse(["-m", "a"]).is_err());
}

#[test]
fn directory_is_described() {
    assert_eq!(describe(Kind::Directory, None, &[], false), "directory");
    assert_eq!(
        describe(Kind::Directory, None, &[], true),
        "inode/directory"
    );
}

#[test]
fn symlink_target_is_shown() {
    assert_eq!(
        describe(Kind::Symlink, Some("/tmp/a"), &[], false),
        "symbolic link to /tmp/a"
    );
}

#[test]
fn ascii_text_is_described() {
    assert_eq!(
        describe(Kind::Regular, None, b"hello\n", false),
        "ASCII text"
    );
    assert_eq!(
        describe(Kind::Regular, None, b"hello\n", true),
        "text/plain"
    );
}

#[test]
fn binary_data_is_described() {
    assert_eq!(describe(Kind::Regular, None, b"a\0b", false), "data");
    assert_eq!(
        describe(Kind::Regular, None, b"a\0b", true),
        "application/octet-stream"
    );
}

#[test]
fn elf_executable_is_described() {
    let mut sample = [0u8; 18];
    sample[..4].copy_from_slice(b"\x7fELF");
    sample[4] = 2;
    sample[5] = 1;
    sample[16] = 2;
    assert!(describe(Kind::Regular, None, &sample, false).contains("ELF 64-bit LSB executable"));
    assert_eq!(
        describe(Kind::Regular, None, &sample, true),
        "application/x-executable"
    );
}

#[test]
fn brief_line_omits_path() {
    assert_eq!(line("/tmp/a", "ASCII text", true), "ASCII text");
    assert_eq!(line("/tmp/a", "ASCII text", false), "/tmp/a: ASCII text");
}
