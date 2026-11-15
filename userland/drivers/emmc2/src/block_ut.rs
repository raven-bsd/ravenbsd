// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `block:` mapping and the memory backend.

use super::*;
use raven_pi4::command::SdCommand;
use raven_proto_block::{BlockInfo, INFO_LEN, MAX_SECTORS, Op, Request, SECTOR_SIZE, decode_reply};

fn encode_request(op: Op, sector: u64, count: u32, data: &[u8], buffer: &mut [u8]) -> usize {
    Request {
        op,
        sector,
        count,
        data,
    }
    .encode(buffer)
    .expect("request encodes")
}

fn reply_status(response: &[u8], length: usize) -> i32 {
    decode_reply(&response[..length]).expect("reply decodes").0
}

#[test]
fn a_single_sector_read_maps_to_cmd17_and_copies_the_pattern() {
    let mut data = [0u8; 4096];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = index as u8;
    }
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");

    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 1, 1, &[], &mut request);
    let mut response = [0u8; 8 + 512];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("handle");
    assert_eq!(written, 8 + 512);
    let (status, payload) = decode_reply(&response[..written]).expect("reply");
    assert_eq!(status, 0);
    assert_eq!(payload.len(), 512);
    for (index, byte) in payload.iter().enumerate() {
        assert_eq!(*byte, (512 + index) as u8);
    }
}

#[test]
fn two_sector_reads_map_to_cmd18_through_the_backend() {
    let mut data = [0u8; 4096];
    let disk = ProbeDisk::new(&mut data);
    let mut service = BlockService::new(disk).expect("service");

    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 2, 2, &[], &mut request);
    let mut response = [0u8; 8 + 1024];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("handle");
    assert_eq!(reply_status(&response, written), 0);
    assert_eq!(service.device.command, Some(SdCommand::Cmd18));
    assert_eq!(service.device.direction, Some(DataDirection::Read));
    assert_eq!(service.device.bytes, 1024);
}

#[test]
fn a_write_is_mapped_and_read_back() {
    let mut data = [0u8; 4096];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let payload = [0xa5u8; 512];

    let mut request = [0u8; 24 + 512];
    let length = encode_request(Op::Write, 3, 1, &payload, &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("write");
    assert_eq!(reply_status(&response, written), 0);

    let mut read_request = [0u8; 24];
    let length = encode_request(Op::Read, 3, 1, &[], &mut read_request);
    let mut read_response = [0u8; 8 + 512];
    let written = service
        .handle(&read_request[..length], &mut read_response)
        .expect("read");
    let (status, payload) = decode_reply(&read_response[..written]).expect("reply");
    assert_eq!(status, 0);
    assert_eq!(payload, &[0xa5u8; 512]);
}

#[test]
fn a_multi_sector_write_maps_to_cmd25() {
    let mut data = [0u8; 4096];
    let disk = ProbeDisk::new(&mut data);
    let mut service = BlockService::new(disk).expect("service");
    let payload = [0x5au8; 1024];

    let mut request = [0u8; 24 + 1024];
    let length = encode_request(Op::Write, 1, 2, &payload, &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("write");
    assert_eq!(reply_status(&response, written), 0);
    assert_eq!(service.device.command, Some(SdCommand::Cmd25));
    assert_eq!(service.device.direction, Some(DataDirection::Write));
}

#[test]
fn info_and_flush_reply_with_their_payloads() {
    let mut data = [0u8; 4096];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");

    let mut request = [0u8; 24];
    let length = encode_request(Op::Info, 0, 0, &[], &mut request);
    let mut response = [0u8; 8 + INFO_LEN];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("info");
    let (status, payload) = decode_reply(&response[..written]).expect("reply");
    assert_eq!(status, 0);
    let info = BlockInfo::decode(payload).expect("info");
    assert_eq!(info.sector_size, SECTOR_SIZE);
    assert_eq!(info.sectors, 8);
    assert_eq!(info.read_only, 0);

    let length = encode_request(Op::Flush, 0, 0, &[], &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("flush");
    assert_eq!(reply_status(&response, written), 0);
}

#[test]
fn list_is_rejected_because_blockd_owns_the_partition_table() {
    let mut data = [0u8; 512];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let mut request = [0u8; 24];
    let length = encode_request(Op::List, 0, 0, &[], &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("list");
    assert_eq!(
        reply_status(&response, written),
        -i32::from(Errno::EOPNOTSUPP.0)
    );
}

#[test]
fn malformed_and_out_of_range_requests_get_einval() {
    let mut data = [0u8; 4096];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let mut response = [0u8; 8 + 512];

    let written = service
        .handle(b"short", &mut response)
        .expect("short request");
    assert_eq!(
        reply_status(&response, written),
        -i32::from(Errno::EINVAL.0)
    );

    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 8, 1, &[], &mut request);
    let written = service
        .handle(&request[..length], &mut response)
        .expect("past end");
    assert_eq!(
        reply_status(&response, written),
        -i32::from(Errno::EINVAL.0)
    );

    let length = encode_request(Op::Read, u64::MAX, 2, &[], &mut request);
    let written = service
        .handle(&request[..length], &mut response)
        .expect("overflow");
    assert_eq!(
        reply_status(&response, written),
        -i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn a_read_only_backend_rejects_writes() {
    let mut data = [0u8; 512];
    let disk = MemoryDisk::new(&mut data, true).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let payload = [0u8; 512];
    let mut request = [0u8; 24 + 512];
    let length = encode_request(Op::Write, 0, 1, &payload, &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("write");
    assert_eq!(reply_status(&response, written), -i32::from(Errno::EROFS.0));
}

#[test]
fn a_backend_error_is_reported_on_the_wire() {
    let mut service = BlockService::new(FailingDisk).expect("service");
    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 0, 1, &[], &mut request);
    let mut response = [0u8; 8 + 512];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("read");
    assert_eq!(reply_status(&response, written), -i32::from(Errno::EIO.0));

    let length = encode_request(Op::Flush, 0, 0, &[], &mut request);
    let mut response = [0u8; 8];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("flush");
    assert_eq!(reply_status(&response, written), -i32::from(Errno::EIO.0));
}

#[test]
fn a_small_response_buffer_is_a_caller_error_or_an_error_reply() {
    let mut data = [0u8; 4096];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 0, 2, &[], &mut request);

    assert_eq!(
        service.handle(&request[..length], &mut []),
        Err(BlockError::ReplyTooSmall)
    );

    let mut response = [0u8; 12];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("small payload buffer");
    assert_eq!(written, 8);
    assert_eq!(
        reply_status(&response, written),
        -i32::from(Errno::EINVAL.0)
    );
}

#[test]
fn a_non_standard_geometry_is_rejected() {
    assert_eq!(
        BlockService::new(BadGeometryDisk { sectors: 8 }).err(),
        Some(BlockError::BadGeometry)
    );
    assert_eq!(
        BlockService::new(BadGeometryDisk { sectors: 0 }).err(),
        Some(BlockError::BadGeometry)
    );
}

#[test]
fn a_plan_selects_the_command_and_transfer_for_the_request() {
    let single = BlockPlan::new(DataDirection::Read, 0x100, 1, 1000).expect("plan");
    assert_eq!(single.command().command(), SdCommand::Cmd17);
    assert_eq!(single.transfer_flags().block_count, 1);
    assert_eq!(single.sector(), 0x100);
    assert_eq!(single.blocks(), 1);
    assert_eq!(single.bytes(), 512);
    assert_eq!(single.direction(), DataDirection::Read);

    let multi = BlockPlan::new(DataDirection::Write, 0x200, 3, 1000).expect("plan");
    assert_eq!(multi.command().command(), SdCommand::Cmd25);
    assert_eq!(multi.transfer_flags().block_count, 3);
    assert_eq!(multi.bytes(), 1536);

    assert_eq!(
        BlockPlan::new(DataDirection::Read, 0, 0, 1000).err(),
        Some(BlockError::OutOfRange)
    );
    assert_eq!(
        BlockPlan::new(DataDirection::Read, 0, 129, 1000).err(),
        Some(BlockError::OutOfRange)
    );
    assert_eq!(
        BlockPlan::new(DataDirection::Read, 1000, 1, 1000).err(),
        Some(BlockError::OutOfRange)
    );
    assert_eq!(
        BlockPlan::new(DataDirection::Read, u64::MAX, 2, u64::MAX).err(),
        Some(BlockError::OutOfRange)
    );
}

#[test]
fn a_maximum_request_fits_the_protocol_and_card_limits() {
    let mut data = [0u8; 128 * 512];
    let disk = MemoryDisk::new(&mut data, false).expect("disk");
    let mut service = BlockService::new(disk).expect("service");
    let mut request = [0u8; 24];
    let length = encode_request(Op::Read, 0, MAX_SECTORS, &[], &mut request);
    let mut response = [0u8; 8 + 128 * 512];
    let written = service
        .handle(&request[..length], &mut response)
        .expect("max read");
    assert_eq!(written, 8 + 128 * 512);
    assert_eq!(reply_status(&response, written), 0);
}

#[test]
fn memory_disk_validates_its_buffer() {
    let mut empty = [];
    assert_eq!(
        MemoryDisk::new(&mut empty, false).err(),
        Some(Errno::EINVAL)
    );
    let mut partial = [0u8; 100];
    assert_eq!(
        MemoryDisk::new(&mut partial, false).err(),
        Some(Errno::EINVAL)
    );
    let mut full = [0u8; 1024];
    let disk = MemoryDisk::new(&mut full, false).expect("disk");
    assert_eq!(disk.sectors(), 2);
}

struct ProbeDisk<'a> {
    data: &'a mut [u8],
    command: Option<SdCommand>,
    direction: Option<DataDirection>,
    bytes: usize,
}

impl<'a> ProbeDisk<'a> {
    fn new(data: &'a mut [u8]) -> Self {
        Self {
            data,
            command: None,
            direction: None,
            bytes: 0,
        }
    }
}

impl BlockDevice for ProbeDisk<'_> {
    fn info(&self) -> BlockInfo {
        BlockInfo {
            sector_size: SECTOR_SIZE,
            read_only: 0,
            sectors: self.data.len() as u64 / u64::from(SECTOR_SIZE),
            reserved: 0,
        }
    }

    fn read(&mut self, plan: &BlockPlan, buffer: &mut [u8]) -> Result<(), Errno> {
        self.command = Some(plan.command().command());
        self.direction = Some(plan.direction());
        self.bytes = buffer.len();
        buffer.fill(0);
        Ok(())
    }

    fn write(&mut self, plan: &BlockPlan, data: &[u8]) -> Result<(), Errno> {
        self.command = Some(plan.command().command());
        self.direction = Some(plan.direction());
        self.bytes = data.len();
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}

struct FailingDisk;

impl BlockDevice for FailingDisk {
    fn info(&self) -> BlockInfo {
        BlockInfo {
            sector_size: SECTOR_SIZE,
            read_only: 0,
            sectors: 8,
            reserved: 0,
        }
    }

    fn read(&mut self, _plan: &BlockPlan, _buffer: &mut [u8]) -> Result<(), Errno> {
        Err(Errno::EIO)
    }

    fn write(&mut self, _plan: &BlockPlan, _data: &[u8]) -> Result<(), Errno> {
        Err(Errno::EIO)
    }

    fn flush(&mut self) -> Result<(), Errno> {
        Err(Errno::EIO)
    }
}

struct BadGeometryDisk {
    sectors: u64,
}

impl BlockDevice for BadGeometryDisk {
    fn info(&self) -> BlockInfo {
        BlockInfo {
            sector_size: 1024,
            read_only: 0,
            sectors: self.sectors,
            reserved: 0,
        }
    }

    fn read(&mut self, _plan: &BlockPlan, _buffer: &mut [u8]) -> Result<(), Errno> {
        Err(Errno::EINVAL)
    }

    fn write(&mut self, _plan: &BlockPlan, _data: &[u8]) -> Result<(), Errno> {
        Err(Errno::EINVAL)
    }

    fn flush(&mut self) -> Result<(), Errno> {
        Ok(())
    }
}
