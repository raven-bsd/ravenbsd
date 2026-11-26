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
fn write_permission_helpers_round_trip_and_have_stable_bytes() {
    let mut bytes = [0u8; CALL_LEN];
    encode_write_request(WriteRequest::Get, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(decode_write_request(&bytes), Ok(WriteRequest::Get));
    encode_write_request(WriteRequest::Set { allowed: false }, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x05, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(
        decode_write_request(&bytes),
        Ok(WriteRequest::Set { allowed: false })
    );
    encode_write_response(true, &mut bytes);
    assert_eq!(
        bytes,
        [
            0x01, 0x00, 0x04, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
        ]
    );
    assert_eq!(decode_write_response(&bytes), Ok(true));
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
fn the_termios_body_is_69_bytes() {
    assert_eq!(TERMIOS_LEN, 69);
}

#[test]
fn get_termios_golden_bytes() {
    let mut bytes = [0xffu8; TERMIOS_LEN];
    encode_get_termios_request(&mut bytes);
    assert_eq!(&bytes[..12], &[1, 0, 6, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(bytes[12..].iter().all(|byte| *byte == 0));
    assert_eq!(decode_termios_request(&bytes), Ok(TermiosRequest::Get));
}

#[test]
fn set_termios_round_trips_every_field() {
    let termios = Termios {
        iflag: 0o000641,
        oflag: 0o000005,
        cflag: 0o000277,
        lflag: 0o102013,
        line: 0,
        cc: [
            3, 28, 127, 21, 4, 0, 1, 0, 17, 19, 26, 0, 18, 15, 23, 22, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0, 0,
        ],
        ispeed: 0o15,
        ospeed: 0o16,
    };
    let mut bytes = [0u8; TERMIOS_LEN];
    encode_set_termios_request(&termios, &mut bytes);
    assert_eq!(&bytes[..4], &[1, 0, 7, 0]);
    assert_eq!(&bytes[12..16], &0o000641u32.to_le_bytes());
    assert_eq!(&bytes[16..20], &0o000005u32.to_le_bytes());
    assert_eq!(&bytes[20..24], &0o000277u32.to_le_bytes());
    assert_eq!(&bytes[24..28], &0o102013u32.to_le_bytes());
    assert_eq!(bytes[28], 0);
    assert_eq!(&bytes[29..61], &termios.cc);
    assert_eq!(&bytes[61..65], &0o15u32.to_le_bytes());
    assert_eq!(&bytes[65..69], &0o16u32.to_le_bytes());
    assert_eq!(
        decode_termios_request(&bytes),
        Ok(TermiosRequest::Set { termios })
    );
    let mut response = [0u8; TERMIOS_LEN];
    encode_termios_response(&termios, &mut response);
    assert_eq!(decode_termios_response(&response), Ok(termios));
}

#[test]
fn a_short_termios_body_is_truncated() {
    let mut bytes = [0u8; CALL_LEN];
    bytes[..4].copy_from_slice(&[1, 0, 6, 0]);
    assert_eq!(decode_termios_request(&bytes), Err(TtyError::Truncated));
    assert_eq!(decode_termios_response(&bytes), Err(TtyError::Truncated));
}

#[test]
fn the_winsize_body_is_20_bytes() {
    assert_eq!(WINSIZE_LEN, 20);
}

#[test]
fn get_winsize_golden_bytes() {
    let mut bytes = [0xffu8; WINSIZE_LEN];
    encode_get_winsize_request(&mut bytes);
    assert_eq!(&bytes[..12], &[1, 0, 8, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    assert!(bytes[12..].iter().all(|byte| *byte == 0));
    assert_eq!(decode_winsize_request(&bytes), Ok(WinsizeRequest::Get));
}

#[test]
fn set_winsize_round_trips_all_four_fields() {
    for winsize in [
        Winsize::default(),
        Winsize {
            row: 43,
            col: 120,
            xpixel: 0,
            ypixel: 0,
        },
        Winsize {
            row: 24,
            col: 80,
            xpixel: 640,
            ypixel: 480,
        },
        Winsize {
            row: u16::MAX,
            col: 1,
            xpixel: 2,
            ypixel: u16::MAX,
        },
    ] {
        let mut bytes = [0u8; WINSIZE_LEN];
        encode_set_winsize_request(&winsize, &mut bytes);
        assert_eq!(&bytes[..4], &[1, 0, 9, 0]);
        assert_eq!(&bytes[12..14], &winsize.row.to_le_bytes());
        assert_eq!(&bytes[14..16], &winsize.col.to_le_bytes());
        assert_eq!(&bytes[16..18], &winsize.xpixel.to_le_bytes());
        assert_eq!(&bytes[18..20], &winsize.ypixel.to_le_bytes());
        assert_eq!(
            decode_winsize_request(&bytes),
            Ok(WinsizeRequest::Set { winsize })
        );
        let mut response = [0u8; WINSIZE_LEN];
        encode_winsize_response(&winsize, &mut response);
        assert_eq!(decode_winsize_response(&response), Ok(winsize));
    }
}

#[test]
fn a_short_winsize_body_is_truncated() {
    let mut bytes = [0u8; CALL_LEN];
    bytes[..4].copy_from_slice(&[1, 0, 8, 0]);
    assert_eq!(decode_winsize_request(&bytes), Err(TtyError::Truncated));
    assert_eq!(decode_winsize_response(&bytes), Err(TtyError::Truncated));
}

#[test]
fn errors_have_descriptions() {
    for error in [
        TtyError::Truncated,
        TtyError::Version(1),
        TtyError::UnknownOp(1),
        TtyError::InvalidMode(1),
        TtyError::InvalidEcho(1),
        TtyError::InvalidWrite(1),
    ] {
        assert!(!error.describe().is_empty());
    }
}
