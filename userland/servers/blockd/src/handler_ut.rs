// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Handler validation and protocol tests.

use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_proto_block::{
    INFO_LEN, REQUEST_HEADER_LEN, Request, decode_partition_list, decode_reply,
};
use raven_scheme::HandlerError;

use crate::device::RamDisk;

use super::*;

fn ctx() -> Ctx {
    Ctx {
        request: Handle(1),
        token: 1,
        buffer_len: 0,
        response_len: 0,
    }
}

fn record(index: u32, start_lba: u64, sectors: u64, name: &str) -> PartitionRecord {
    let mut record = PartitionRecord {
        index,
        start_lba,
        sectors,
        name: [0; 40],
        name_len: name.len() as u8,
    };
    record.name[..name.len()].copy_from_slice(name.as_bytes());
    record
}

fn new_handler() -> BlockHandler<RamDisk> {
    let disk = RamDisk::new(vec![0u8; 8 * SECTOR_SIZE as usize], false).expect("disk");
    BlockHandler::new(
        disk,
        vec![record(1, 34, 67, "first"), record(2, 101, 100, "second")],
    )
    .expect("handler")
}

fn opened() -> BlockHandler<RamDisk> {
    let mut handler = new_handler();
    handler.expect_open(7, DEVICE_PATH);
    match handler
        .open_with(DEVICE_PATH, OpenFlags::READ_WRITE, 0, &ctx())
        .expect("open")
    {
        Opened::File(7) => {}
        other => panic!("unexpected open result: {other:?}"),
    }
    handler
}

fn request(op: Op, sector: u64, count: u32, data: &[u8]) -> Vec<u8> {
    let request = Request {
        op,
        sector,
        count,
        data,
    };
    let mut bytes = vec![0u8; REQUEST_HEADER_LEN + data.len()];
    let length = request.encode(&mut bytes).expect("encode");
    bytes.truncate(length);
    bytes
}

fn reply_status(response: &[u8], length: usize) -> i32 {
    let (status, payload) = decode_reply(&response[..length]).expect("reply");
    assert!(payload.is_empty(), "error replies carry no payload");
    status
}

#[test]
fn opens_the_single_device_path() {
    let mut handler = new_handler();
    handler.expect_open(7, DEVICE_PATH);
    assert!(matches!(
        handler.open_with(DEVICE_PATH, OpenFlags::READ, 0, &ctx()),
        Ok(Opened::File(7))
    ));

    let mut handler = new_handler();
    handler.expect_open(8, "1");
    assert_eq!(
        handler
            .open_with("1", OpenFlags::READ, 0, &ctx())
            .expect_err("wrong path"),
        HandlerError::Errno(Errno::ENOENT)
    );

    let mut handler = new_handler();
    handler.expect_open(9, "");
    assert!(matches!(
        handler.open_with("", OpenFlags::READ, 0, &ctx()),
        Ok(Opened::File(9))
    ));

    let mut handler = new_handler();
    assert_eq!(
        handler
            .open_with(DEVICE_PATH, OpenFlags::READ, 0, &ctx())
            .expect_err("no pending open"),
        HandlerError::Errno(Errno::EINVAL)
    );
}

#[test]
fn open_flags_are_restricted_to_access_mode() {
    let mut handler = new_handler();
    handler.expect_open(7, DEVICE_PATH);
    assert_eq!(
        handler
            .open_with(
                DEVICE_PATH,
                OpenFlags::from_linux(1 | 64).unwrap(),
                0,
                &ctx()
            )
            .expect_err("create is not supported"),
        HandlerError::Errno(Errno::EINVAL)
    );
}

#[test]
fn a_read_only_device_rejects_a_writable_open() {
    let mut handler = BlockHandler::new(
        RamDisk::new(vec![0u8; SECTOR_SIZE as usize], true).expect("disk"),
        Vec::new(),
    )
    .expect("handler");
    handler.expect_open(7, DEVICE_PATH);
    assert_eq!(
        handler
            .open_with(DEVICE_PATH, OpenFlags::READ_WRITE, 0, &ctx())
            .expect_err("read-only"),
        HandlerError::Errno(Errno::EROFS)
    );
}

#[test]
fn info_reports_the_ram_disk_geometry() {
    let mut handler = opened();
    let mut response = [0u8; 64];
    let length = handler
        .call(7, &request(Op::Info, 0, 0, &[]), &mut response)
        .expect("info");
    let (status, payload) = decode_reply(&response[..length]).expect("reply");
    assert_eq!(status, 0);
    let info = BlockInfo::decode(payload).expect("info payload");
    assert_eq!(info.sector_size, SECTOR_SIZE);
    assert_eq!(info.read_only, 0);
    assert_eq!(info.sectors, 8);
    assert_eq!(info.reserved, 0);
    assert_eq!(payload.len(), INFO_LEN);
}

#[test]
fn write_read_and_flush_round_trip() {
    let mut handler = opened();
    let pattern: Vec<u8> = (0..1024).map(|index| index as u8).collect();
    let mut response = [0u8; 4096];

    let length = handler
        .call(7, &request(Op::Write, 2, 2, &pattern), &mut response)
        .expect("write");
    assert_eq!(reply_status(&response, length), 0);

    let length = handler
        .call(7, &request(Op::Read, 2, 2, &[]), &mut response)
        .expect("read");
    let (status, payload) = decode_reply(&response[..length]).expect("reply");
    assert_eq!(status, 0);
    assert_eq!(payload, pattern);

    let length = handler
        .call(7, &request(Op::Flush, 0, 0, &[]), &mut response)
        .expect("flush");
    assert_eq!(reply_status(&response, length), 0);
}

#[test]
fn list_returns_the_scanned_partitions() {
    let mut handler = opened();
    let mut response = [0u8; 8192];
    let length = handler
        .call(7, &request(Op::List, 0, 0, &[]), &mut response)
        .expect("list");
    let (status, payload) = decode_reply(&response[..length]).expect("reply");
    assert_eq!(status, 0);
    let partitions = decode_partition_list(payload).expect("list payload");
    assert_eq!(partitions.len(), 2);
    assert_eq!(partitions[0].index, 1);
    assert_eq!(partitions[0].start_lba, 34);
    assert_eq!(&partitions[0].name[..5], b"first");
    assert_eq!(partitions[1].sectors, 100);
}

#[test]
fn malformed_framing_returns_einval() {
    let mut handler = opened();
    let mut response = [0u8; 4096];
    let expected = -i32::from(Errno::EINVAL.0);
    let mut cases: Vec<Vec<u8>> = Vec::new();

    cases.push(vec![0u8; REQUEST_HEADER_LEN - 1]);

    let mut bad_version = request(Op::Info, 0, 0, &[]);
    bad_version[0] = 2;
    cases.push(bad_version);

    let mut bad_op = request(Op::Info, 0, 0, &[]);
    bad_op[2..4].copy_from_slice(&99u16.to_le_bytes());
    cases.push(bad_op);

    let mut bad_flags = request(Op::Read, 0, 1, &[]);
    bad_flags[16] = 1;
    cases.push(bad_flags);

    let mut zero_count = request(Op::Read, 0, 1, &[]);
    zero_count[12..16].copy_from_slice(&0u32.to_le_bytes());
    cases.push(zero_count);

    let mut too_many = request(Op::Read, 0, 1, &[]);
    too_many[12..16].copy_from_slice(&(MAX_SECTORS + 1).to_le_bytes());
    cases.push(too_many);

    let mut short_write = vec![0u8; REQUEST_HEADER_LEN];
    short_write[..2].copy_from_slice(&1u16.to_le_bytes());
    short_write[2..4].copy_from_slice(&(Op::Write as u16).to_le_bytes());
    short_write[12..16].copy_from_slice(&1u32.to_le_bytes());
    short_write[20..24].copy_from_slice(&512u32.to_le_bytes());
    cases.push(short_write);

    let mut short_payload = request(Op::Write, 0, 1, &[0u8; 512]);
    short_payload[20..24].copy_from_slice(&1024u32.to_le_bytes());
    cases.push(short_payload);

    let mut control_sector = request(Op::Flush, 0, 0, &[]);
    control_sector[4..12].copy_from_slice(&1u64.to_le_bytes());
    cases.push(control_sector);

    let mut trailing = request(Op::Info, 0, 0, &[]);
    trailing.push(0);
    cases.push(trailing);

    for case in cases {
        let length = handler
            .call(7, &case, &mut response)
            .unwrap_or_else(|error| panic!("request {case:?}: {error:?}"));
        assert_eq!(
            reply_status(&response, length),
            expected,
            "request {case:?}"
        );
    }
}

#[test]
fn out_of_range_and_overflow_are_rejected_before_the_device() {
    let mut handler = opened();
    let mut response = [0u8; 4096];
    let expected = -i32::from(Errno::EINVAL.0);
    let cases = [
        request(Op::Read, 8, 1, &[]),
        request(Op::Read, u64::MAX, 1, &[]),
        request(Op::Read, u64::MAX, 2, &[]),
        request(Op::Write, 8, 1, &[0u8; 512]),
        request(Op::Write, u64::MAX, 2, &[0u8; 1024]),
    ];
    for case in cases {
        let length = handler
            .call(7, &case, &mut response)
            .expect("rejected request still gets a reply");
        assert_eq!(reply_status(&response, length), expected);
    }
}

#[test]
fn a_permission_mismatch_returns_ebadf() {
    let mut handler = new_handler();
    handler.expect_open(7, DEVICE_PATH);
    handler
        .open_with(DEVICE_PATH, OpenFlags::READ, 0, &ctx())
        .expect("read-only open");
    let mut response = [0u8; 4096];
    let length = handler
        .call(7, &request(Op::Write, 0, 1, &[0u8; 512]), &mut response)
        .expect("write");
    assert_eq!(reply_status(&response, length), -i32::from(Errno::EBADF.0));
}

#[test]
fn a_closed_or_unknown_file_returns_ebadf() {
    let mut handler = opened();
    let mut response = [0u8; 64];
    assert_eq!(
        handler
            .call(8, &request(Op::Info, 0, 0, &[]), &mut response)
            .expect_err("unknown file"),
        HandlerError::Errno(Errno::EBADF)
    );
    handler.close(7).expect("close");
    assert_eq!(
        handler
            .call(7, &request(Op::Info, 0, 0, &[]), &mut response)
            .expect_err("closed file"),
        HandlerError::Errno(Errno::EBADF)
    );
}

#[test]
fn a_long_partition_name_is_shortened_on_a_boundary() {
    let partition = Partition {
        index: 3,
        start_lba: 10,
        last_lba: 20,
        name: "a".repeat(39) + "é",
    };
    let record = partition_record(&partition);
    assert_eq!(record.name_len, 39);
    assert_eq!(
        &record.name[..record.name_len as usize],
        "a".repeat(39).as_bytes()
    );

    let partition = Partition {
        index: 4,
        start_lba: 10,
        last_lba: 20,
        name: "part-four".to_string(),
    };
    let record = partition_record(&partition);
    assert_eq!(&record.name[..record.name_len as usize], b"part-four");
    assert_eq!(record.sectors, 11);
}
