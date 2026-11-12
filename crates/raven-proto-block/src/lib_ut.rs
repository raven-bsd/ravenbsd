// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the block wire protocol.

use super::*;

fn read_request() -> Request<'static> {
    Request {
        op: Op::Read,
        sector: 5,
        count: 2,
        data: &[],
    }
}

#[test]
fn request_header_is_golden() {
    let request = Request {
        op: Op::Read,
        sector: 0x0102_0304_0506_0708,
        count: 3,
        data: &[],
    };
    let mut bytes = [0u8; REQUEST_HEADER_LEN];
    assert_eq!(request.encode(&mut bytes), Ok(REQUEST_HEADER_LEN));
    assert_eq!(
        bytes,
        [
            1, 0, 1, 0, 8, 7, 6, 5, 4, 3, 2, 1, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ]
    );
}

#[test]
fn read_request_round_trips() {
    let request = read_request();
    let mut bytes = [0u8; REQUEST_HEADER_LEN];
    let length = request.encode(&mut bytes).expect("encode");
    assert_eq!(Request::decode(&bytes[..length]), Ok(request));
}

#[test]
fn write_request_carries_exact_sectors() {
    let data = [0x5a; 1024];
    let request = Request {
        op: Op::Write,
        sector: 1,
        count: 2,
        data: &data,
    };
    let mut bytes = [0u8; REQUEST_HEADER_LEN + 1024];
    let length = request.encode(&mut bytes).expect("encode");
    assert_eq!(length, 1048);
    assert_eq!(Request::decode(&bytes[..length]), Ok(request));
}

#[test]
fn control_requests_have_no_data() {
    for op in [Op::Flush, Op::Info, Op::List] {
        let request = Request {
            op,
            sector: 0,
            count: 0,
            data: &[],
        };
        let mut bytes = [0u8; REQUEST_HEADER_LEN];
        let length = request.encode(&mut bytes).expect("encode");
        assert_eq!(Request::decode(&bytes[..length]), Ok(request));
    }
}

#[test]
fn bad_versions_and_ops_are_rejected() {
    let mut bytes = [0u8; REQUEST_HEADER_LEN];
    read_request().encode(&mut bytes).expect("encode");
    bytes[0] = 2;
    assert_eq!(Request::decode(&bytes), Err(ProtoError::BadVersion));
    bytes[0] = 1;
    bytes[2] = 99;
    assert_eq!(Request::decode(&bytes), Err(ProtoError::UnknownOp));
}

#[test]
fn bad_lengths_and_fields_are_rejected() {
    let mut bytes = [0u8; REQUEST_HEADER_LEN];
    read_request().encode(&mut bytes).expect("encode");
    bytes[12..16].copy_from_slice(&0u32.to_le_bytes());
    assert_eq!(Request::decode(&bytes), Err(ProtoError::BadLength));
    read_request().encode(&mut bytes).expect("encode");
    bytes[16] = 1;
    assert_eq!(Request::decode(&bytes), Err(ProtoError::BadField));
    read_request().encode(&mut bytes).expect("encode");
    let mut extra = bytes.to_vec();
    extra.push(0);
    assert_eq!(Request::decode(&extra), Err(ProtoError::TrailingBytes));
    assert_eq!(Request::decode(&[]), Err(ProtoError::TooShort));
}

#[test]
fn write_count_bounds_are_enforced() {
    let data = [0u8; 512];
    let request = Request {
        op: Op::Write,
        sector: 0,
        count: MAX_SECTORS + 1,
        data: &data,
    };
    let mut bytes = [0u8; 4096];
    assert_eq!(request.encode(&mut bytes), Err(ProtoError::BadLength));
}

#[test]
fn info_round_trips() {
    let info = BlockInfo {
        sector_size: SECTOR_SIZE,
        read_only: 0,
        sectors: 2048,
        reserved: 0,
    };
    let mut bytes = [0u8; INFO_LEN];
    info.encode(&mut bytes).expect("encode");
    assert_eq!(BlockInfo::decode(&bytes), Ok(info));
}

#[test]
fn partition_record_round_trips() {
    let mut record = PartitionRecord {
        index: 2,
        start_lba: 101,
        sectors: 100,
        name: [0; 40],
        name_len: 6,
    };
    record.name[..6].copy_from_slice(b"second");
    let mut bytes = [0u8; PARTITION_LEN];
    record.encode(&mut bytes).expect("encode");
    assert_eq!(PartitionRecord::decode(&bytes), Ok(record));
}

#[test]
fn reply_round_trips() {
    let mut bytes = [0u8; 64];
    let length = encode_reply(&mut bytes, -22, b"data").expect("encode");
    assert_eq!(length, 12);
    assert_eq!(decode_reply(&bytes[..length]), Ok((-22, &b"data"[..])));
    assert_eq!(decode_reply(&bytes[..7]), Err(ProtoError::TooShort));
}

#[test]
fn reply_header_can_be_written_after_the_payload() {
    let mut bytes = [0u8; 16];
    bytes[REPLY_HEADER_LEN..12].copy_from_slice(b"data");
    write_reply_header(&mut bytes, 0, 4).expect("header");
    assert_eq!(decode_reply(&bytes[..12]), Ok((0, &b"data"[..])));
    assert_eq!(
        write_reply_header(&mut bytes[..7], 0, 0),
        Err(ProtoError::TooShort)
    );
}

#[test]
fn partition_list_round_trips() {
    let mut first = PartitionRecord {
        index: 1,
        start_lba: 34,
        sectors: 67,
        name: [0; 40],
        name_len: 5,
    };
    first.name[..5].copy_from_slice(b"first");
    let mut second = PartitionRecord {
        index: 2,
        start_lba: 101,
        sectors: 100,
        name: [0; 40],
        name_len: 6,
    };
    second.name[..6].copy_from_slice(b"second");
    let records = [first, second];
    let mut bytes = [0u8; LIST_HEADER_LEN + 2 * PARTITION_LEN];
    let length = encode_partition_list(&mut bytes, &records).expect("encode");
    assert_eq!(length, bytes.len());
    assert_eq!(
        decode_partition_list(&bytes[..length]),
        Ok(records.to_vec())
    );
    assert_eq!(
        decode_partition_list(&bytes[..length - 1]),
        Err(ProtoError::TooShort)
    );
    let mut extra = bytes.to_vec();
    extra.push(0);
    assert_eq!(
        decode_partition_list(&extra),
        Err(ProtoError::TrailingBytes)
    );
}

#[test]
fn partition_list_rejects_an_oversized_count() {
    let mut bytes = [0u8; LIST_HEADER_LEN];
    bytes[..4].copy_from_slice(&((MAX_PARTITIONS + 1) as u32).to_le_bytes());
    assert_eq!(decode_partition_list(&bytes), Err(ProtoError::BadLength));
    let records = alloc::vec![PartitionRecord::default(); MAX_PARTITIONS + 1];
    let mut out = alloc::vec![0u8; LIST_HEADER_LEN + records.len() * PARTITION_LEN];
    assert_eq!(
        encode_partition_list(&mut out, &records),
        Err(ProtoError::BadLength)
    );
}

#[test]
fn crc32_matches_the_standard_check_value() {
    assert_eq!(crate::gpt::crc32(b"123456789"), 0xcbf4_3926);
}
