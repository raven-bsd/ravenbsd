// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the .Z LZW codec.

use super::{Error, MAGIC, Options, decode, encode, encode_bits, parse_args};

#[test]
fn header_is_block_mode_16_bits() {
    let encoded = encode(b"");
    assert_eq!(&encoded[..3], &[MAGIC[0], MAGIC[1], 0x90]);
    assert_eq!(decode(&encoded).unwrap(), b"");
}

#[test]
fn round_trips_empty_text_and_binary() {
    for data in [
        Vec::new(),
        b"hello\n".to_vec(),
        (0u8..=255).collect::<Vec<_>>(),
        vec![b'x'; 10_000],
    ] {
        let encoded = encode(&data);
        assert_eq!(decode(&encoded).unwrap(), data);
    }
}

#[test]
fn explicit_widths_round_trip() {
    let data = vec![b'a'; 1000];
    for bits in 9..=16 {
        let encoded = encode_bits(&data, bits);
        assert_eq!(encoded[2], 0x80 | bits);
        assert_eq!(decode(&encoded).unwrap(), data);
    }
}

#[test]
fn corrupt_input_is_rejected() {
    assert_eq!(decode(b"\x1f"), Err(Error::Truncated));
    assert_eq!(decode(b"\x1f\x9d"), Err(Error::Truncated));
    assert_eq!(decode(b"\x1f\x9e\x90"), Err(Error::BadMagic));
    assert_eq!(decode(b"\x1f\x9d\x00"), Err(Error::BadFlags));
    assert_eq!(decode(b"\x1f\x9d\x98"), Err(Error::BadFlags));
    assert_eq!(decode(b"\x1f\x9d\x90\xff\x01"), Err(Error::Corrupt));
}

#[test]
fn parses_options() {
    assert_eq!(
        parse_args(["-c".into(), "-b".into(), "12".into(), "a".into()]).unwrap(),
        Options {
            stdout: true,
            decompress: false,
            bits: 12,
            files: vec!["a".into()],
        }
    );
    assert!(parse_args(["-b".into(), "8".into(), "a".into()]).is_err());
    assert!(parse_args(["-x".into(), "a".into()]).is_err());
    assert!(parse_args(Vec::<String>::new()).is_err());
}
