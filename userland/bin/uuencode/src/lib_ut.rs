// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the uuencode codec.

use super::{Decoded, Error, MAX_LINE, decode, encode, parse};

#[test]
fn parses_input_and_name() {
    let options = parse(["file", "archive-name"]).unwrap();
    assert_eq!(options.input, Some("file"));
    assert_eq!(options.name, "archive-name");
    let options = parse(["name"]).unwrap();
    assert_eq!(options.input, None);
    assert_eq!(options.name, "name");
    assert_eq!(parse(Vec::<&str>::new()), Err(Error::Usage));
}

#[test]
fn encodes_the_begin_and_end_wrapper() {
    let encoded = encode(b"", "empty", 0o644);
    assert_eq!(encoded, b"begin 644 empty\n`\nend\n");
}

#[test]
fn encodes_and_decodes_a_short_input() {
    let encoded = encode(b"hello\n", "hello.txt", 0o600);
    assert!(encoded.starts_with(b"begin 600 hello.txt\n"));
    assert!(encoded.ends_with(b"`\nend\n"));
    let decoded = decode(&encoded).unwrap();
    assert_eq!(
        decoded,
        Decoded {
            mode: 0o600,
            name: "hello.txt".into(),
            data: b"hello\n".to_vec(),
        }
    );
}

#[test]
fn round_trips_every_length_up_to_the_line_limit() {
    let data: Vec<u8> = (0..=MAX_LINE as u8).collect();
    let decoded = decode(&encode(&data, "bytes", 0o755)).unwrap();
    assert_eq!(decoded.data, data);
    assert_eq!(decoded.mode, 0o755);
}

#[test]
fn rejects_a_missing_header_or_end() {
    assert_eq!(decode(b"not encoded\n"), Err(Error::MissingBegin));
    assert_eq!(decode(b"begin 644 file\n"), Err(Error::MissingEnd));
}

#[test]
fn rejects_a_bad_mode_and_short_line() {
    assert_eq!(decode(b"begin 8x file\n`\nend\n"), Err(Error::BadMode));
    assert_eq!(decode(b"begin 644 file\n#\nend\n"), Err(Error::BadLine));
}
