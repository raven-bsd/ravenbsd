// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for the uuencode codec.

use std::os::unix::fs::PermissionsExt;

use uuencode::{decode, encode};

#[test]
fn real_files_round_trip_with_mode() {
    let path = std::env::temp_dir().join(format!("raven-uuencode-{}", std::process::id()));
    let data = b"the quick brown fox\njumps over the lazy dog\n";
    std::fs::write(&path, data).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o7777;
    let encoded = encode(data, "fox.txt", mode);
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded.data, data);
    assert_eq!(decoded.name, "fox.txt");
    let _ = std::fs::remove_file(path);
}

#[test]
fn a_45_byte_line_is_the_maximum_chunk() {
    let data = vec![b'x'; 45];
    let encoded = encode(&data, "line", 0o644);
    let body = encoded
        .split(|byte| *byte == b'\n')
        .nth(1)
        .expect("encoded body");
    assert_eq!(body[0], 32 + 45);
    assert_eq!(decode(&encoded).unwrap().data, data);
}
