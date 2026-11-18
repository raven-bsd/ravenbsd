// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the VideoCore property codec.

use super::*;

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Turn an encoded request into the firmware's success response.
fn respond(buffer: &mut [u8], len: usize, info: &DisplayInfo) {
    put_u32(buffer, 4, RESPONSE_SUCCESS);
    let mut cursor = HEADER_BYTES;
    while cursor < len {
        let tag = read_u32(buffer, cursor);
        if tag == 0 {
            break;
        }
        let buffer_bytes = read_u32(buffer, cursor + 4) as usize;
        let (response_len, values): (u32, &[u32]) = match tag {
            TAG_SET_PHYSICAL_WH => (8, &[info.width, info.height]),
            TAG_SET_VIRTUAL_WH => (8, &[info.width, info.height]),
            TAG_SET_DEPTH => (4, &[info.depth]),
            TAG_SET_PIXEL_ORDER => (4, &[info.pixel_order]),
            TAG_ALLOCATE_BUFFER => (8, &[info.address, info.size]),
            TAG_GET_PITCH => (4, &[info.pitch]),
            _ => (0, &[]),
        };
        put_u32(buffer, cursor + 8, RESPONSE_BIT | response_len);
        for (index, value) in values.iter().enumerate() {
            put_u32(buffer, cursor + TAG_HEADER_BYTES + index * 4, *value);
        }
        cursor += TAG_HEADER_BYTES + buffer_bytes;
    }
}

fn info() -> DisplayInfo {
    DisplayInfo {
        address: 0xc000_0000,
        size: 1024 * 768 * 4,
        pitch: 4096,
        width: 1024,
        height: 768,
        depth: 32,
        pixel_order: PIXEL_ORDER_RGB,
    }
}

#[test]
fn encodes_the_documented_tag_sequence() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    assert_eq!(len, 120);
    assert_eq!(read_u32(&buffer, 0), 120);
    assert_eq!(read_u32(&buffer, 4), 0);
    assert_eq!(read_u32(&buffer, 8), TAG_SET_PHYSICAL_WH);
    assert_eq!(read_u32(&buffer, 12), 8);
    assert_eq!(read_u32(&buffer, 16), 8);
    assert_eq!(read_u32(&buffer, 20), 1024);
    assert_eq!(read_u32(&buffer, 24), 768);
    assert_eq!(read_u32(&buffer, len - 4), 0);
}

#[test]
fn decodes_a_successful_response() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    let expected = info();
    respond(&mut buffer, len, &expected);
    assert_eq!(decode_response(&buffer[..len]), Ok(expected));
}

#[test]
fn rejects_a_firmware_error_code() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    let expected = info();
    respond(&mut buffer, len, &expected);
    put_u32(&mut buffer, 4, 0x8000_0001);
    assert_eq!(
        decode_response(&buffer[..len]),
        Err(MailboxError::FirmwareError)
    );
}

#[test]
fn rejects_a_tag_without_the_response_bit() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    let expected = info();
    respond(&mut buffer, len, &expected);
    put_u32(&mut buffer, 16, 8);
    assert_eq!(
        decode_response(&buffer[..len]),
        Err(MailboxError::TagFailed(TAG_SET_PHYSICAL_WH))
    );
}

#[test]
fn rejects_a_missing_tag() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    let expected = info();
    respond(&mut buffer, len, &expected);
    // Overwrite the first tag with the zero terminator.
    put_u32(&mut buffer, 8, 0);
    assert_eq!(
        decode_response(&buffer[..len]),
        Err(MailboxError::MalformedResponse)
    );
}

#[test]
fn rejects_an_allocation_smaller_than_pitch_times_height() {
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let len = encode_request(&mut buffer, &DisplayRequest::default()).expect("encode");
    let mut expected = info();
    expected.size = 4096;
    respond(&mut buffer, len, &expected);
    assert_eq!(
        decode_response(&buffer[..len]),
        Err(MailboxError::AllocationTooSmall)
    );
}

#[test]
fn rejects_a_short_buffer_and_unsupported_depth() {
    let mut small = [0u8; 16];
    assert_eq!(
        encode_request(&mut small, &DisplayRequest::default()),
        Err(MailboxError::BufferTooSmall)
    );
    let mut buffer = [0u8; MAX_PROPERTY_BYTES];
    let request = DisplayRequest {
        depth: 24,
        ..DisplayRequest::default()
    };
    assert_eq!(
        encode_request(&mut buffer, &request),
        Err(MailboxError::InvalidRequest)
    );
}
