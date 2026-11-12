// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests and golden bytes for the terminal control protocol.

use super::*;

#[test]
fn get_mode_request_golden_bytes() {
    let mut bytes = [0xffu8; CALL_LEN];
    encode_request(Request::GetMode, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn set_mode_request_golden_bytes() {
    let mut bytes = [0u8; CALL_LEN];
    encode_request(
        Request::SetMode {
            settings: Settings {
                mode: Mode::Raw,
                echo: false,
            },
        },
        &mut bytes,
    );
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn flush_request_golden_bytes() {
    let mut bytes = [0u8; CALL_LEN];
    encode_request(Request::Flush, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn mode_response_golden_bytes() {
    let mut bytes = [0u8; CALL_LEN];
    encode_response(
        Response::Mode(Settings {
            mode: Mode::Canonical,
            echo: true,
        }),
        &mut bytes,
    );
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn flushed_response_golden_bytes() {
    let mut bytes = [0u8; CALL_LEN];
    encode_response(Response::Flushed { bytes: 0x0102_0304 }, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x03, 0x00, 0x04, 0x03, 0x02, 0x01, 0x00, 0x00, 0x00, 0x00
        ]
    );
}

#[test]
fn requests_round_trip() {
    for request in [
        Request::GetMode,
        Request::Flush,
        Request::SetMode {
            settings: Settings {
                mode: Mode::Raw,
                echo: true,
            },
        },
        Request::SetMode {
            settings: Settings {
                mode: Mode::Canonical,
                echo: false,
            },
        },
    ] {
        let mut bytes = [0u8; CALL_LEN];
        encode_request(request, &mut bytes);
        assert_eq!(decode_request(&bytes), Ok(request));
    }
}

#[test]
fn responses_round_trip() {
    for response in [
        Response::Mode(Settings::default()),
        Response::Mode(Settings {
            mode: Mode::Raw,
            echo: false,
        }),
        Response::Flushed { bytes: 1024 },
    ] {
        let mut bytes = [0u8; CALL_LEN];
        encode_response(response, &mut bytes);
        assert_eq!(decode_response(&bytes), Ok(response));
    }
}

#[test]
fn short_messages_are_rejected() {
    assert_eq!(
        decode_request(&[0u8; CALL_LEN - 1]),
        Err(TtyError::Truncated)
    );
    assert_eq!(decode_response(&[]), Err(TtyError::Truncated));
}

#[test]
fn a_foreign_version_is_rejected() {
    let mut bytes = [0u8; CALL_LEN];
    bytes[0] = 2;
    assert_eq!(decode_request(&bytes), Err(TtyError::Version(2)));
    assert_eq!(decode_response(&bytes), Err(TtyError::Version(2)));
}

#[test]
fn unknown_operations_are_rejected() {
    let mut bytes = [0u8; CALL_LEN];
    encode_request(Request::GetMode, &mut bytes);
    bytes[2] = 9;
    assert_eq!(decode_request(&bytes), Err(TtyError::UnknownOp(9)));
}

#[test]
fn invalid_mode_and_echo_are_rejected() {
    let mut bytes = [0u8; CALL_LEN];
    encode_request(
        Request::SetMode {
            settings: Settings::default(),
        },
        &mut bytes,
    );
    bytes[4] = 7;
    assert_eq!(decode_request(&bytes), Err(TtyError::InvalidMode(7)));
    bytes[4] = 0;
    bytes[8] = 2;
    assert_eq!(decode_request(&bytes), Err(TtyError::InvalidEcho(2)));
}

#[test]
fn trailing_bytes_are_ignored() {
    let mut bytes = [0u8; CALL_LEN + 4];
    let mut fixed = [0u8; CALL_LEN];
    encode_request(Request::Flush, &mut fixed);
    bytes[..CALL_LEN].copy_from_slice(&fixed);
    assert_eq!(decode_request(&bytes), Ok(Request::Flush));
}

#[test]
fn errors_have_descriptions() {
    for error in [
        TtyError::Truncated,
        TtyError::Version(1),
        TtyError::UnknownOp(1),
        TtyError::InvalidMode(1),
        TtyError::InvalidEcho(1),
    ] {
        assert!(!error.describe().is_empty());
    }
}
