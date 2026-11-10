// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden bytes taken from the tables in `README.md`.
//!
//! These are the literals a second implementation (`audiod` or a driver) is written against. If
//! an encoder changes intentionally, the README table and these tests change together.

use raven_proto_audio::{
    BufferInfo, Call, Format, Handle, RingLayout, SampleFormat, Status, StreamRequest,
    StreamResponse, StreamState, format,
};

fn encode_request(request: StreamRequest<'_>) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let length = request.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

fn encode_response(response: StreamResponse) -> Vec<u8> {
    let mut buffer = [0u8; 128];
    let length = response.encode(&mut buffer).unwrap();
    buffer[..length].to_vec()
}

#[test]
fn open_message_matches_the_readme() {
    assert_eq!(
        encode_request(StreamRequest::Open { path: "audio:out0" }),
        [
            0x01, 0x00, // version
            0x01, 0x00, // OPEN
            0x0a, 0x00, // 10-byte path
            b'a', b'u', b'd', b'i', b'o', b':', b'o', b'u', b't', b'0',
        ]
    );
}

#[test]
fn set_format_message_matches_the_readme() {
    assert_eq!(
        encode_request(StreamRequest::Call(Call::SetFormat {
            format: Format {
                rate: 48_000,
                channels: 2,
                sample: SampleFormat::S16Le,
            },
        })),
        [
            0x01, 0x00, // version
            0x03, 0x00, // CALL
            0x01, 0x00, // SET_FORMAT
            0x80, 0xbb, 0x00, 0x00, // rate 48000
            0x02, // channels
            0x01, // S16LE
        ]
    );
}

#[test]
fn set_buffer_message_matches_the_readme() {
    assert_eq!(
        encode_request(StreamRequest::Call(Call::SetBuffer {
            handle: Handle(7),
            period_frames: 1024,
            period_count: 4,
        })),
        [
            0x01, 0x00, // version
            0x03, 0x00, // CALL
            0x02, 0x00, // SET_BUFFER
            0x07, 0x00, 0x00, 0x00, // destination-valid handle
            0x00, 0x04, 0x00, 0x00, // period frames 1024
            0x04, 0x00, // period count 4
        ]
    );
}

#[test]
fn set_volume_message_matches_the_readme() {
    assert_eq!(
        encode_request(StreamRequest::Call(Call::SetVolume {
            left: 32_768,
            right: 65_535,
        })),
        [
            0x01, 0x00, // version
            0x03, 0x00, // CALL
            0x06, 0x00, // SET_VOLUME
            0x00, 0x80, // left
            0xff, 0xff, // right
        ]
    );
}

#[test]
fn status_response_matches_the_readme() {
    assert_eq!(
        encode_response(StreamResponse::Status(Status {
            position_frames: 1_000,
            underruns: 2,
            state: StreamState::Running,
        })),
        [
            0x01, 0x00, // version
            0x00, 0x00, // status ok
            0x03, 0x00, // STATUS
            0xe8, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, // position 1000
            0x02, 0x00, 0x00, 0x00, // underruns 2
            0x01, // running
        ]
    );
}

#[test]
fn buffer_response_matches_the_readme() {
    assert_eq!(
        encode_response(StreamResponse::Buffer(BufferInfo {
            period_frames: 1024,
            period_count: 4,
            region_len: 16_512,
        })),
        [
            0x01, 0x00, // version
            0x00, 0x00, // status ok
            0x02, 0x00, // BUFFER
            0x00, 0x04, 0x00, 0x00, // period frames 1024
            0x04, 0x00, // period count 4
            0x80, 0x40, 0x00, 0x00, // region length 16512
        ]
    );
}

#[test]
fn unsupported_format_response_matches_the_readme() {
    assert_eq!(
        encode_response(StreamResponse::error(format::ENOTSUP)),
        [
            0x01, 0x00, // version
            0x5f, 0x00, // status ENOTSUP (95)
            0x04, 0x00, // ERR
        ]
    );
}

#[test]
fn ring_header_matches_the_readme() {
    let format = Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    };
    let layout = RingLayout::new(128 + 4096 * 4, format, 1024, 4).expect("layout");
    let mut region = vec![0u8; layout.region_len()];
    layout
        .write_header(&mut region, 0x0102_0304_0506_0708)
        .expect("header");
    assert_eq!(
        &region[..80],
        &[
            b'R', b'A', b'V', b'E', b'N', b'A', b'U', b'D', // magic
            0x01, 0x00, // version
            0x50, 0x00, // header length 80
            0x00, 0x10, 0x00, 0x00, // capacity 4096
            0x00, 0x04, 0x00, 0x00, // period 1024
            0x04, 0x00, // period count 4
            0x04, 0x00, // frame bytes 4
            0x80, 0xbb, 0x00, 0x00, // rate 48000
            0x02, // channels
            0x01, // S16LE
            0x00, // stopped
            0x00, // reserved
            0, 0, 0, 0, 0, 0, 0, 0, // read index
            0, 0, 0, 0, 0, 0, 0, 0, // write index
            0, 0, 0, 0, 0, 0, 0, 0, // start index
            0, 0, 0, 0, // underruns
            0, 0, 0, 0, // reserved
            0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // doorbell
            0x80, 0x00, 0x00, 0x00, // data offset 128
            0x00, 0x40, 0x00, 0x00, // data length 16384
        ]
    );
}
