// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `block:` client used by the `blockd` qtest.
//!
//! It exercises the happy path (INFO, READ, WRITE, FLUSH, LIST), the malformed
//! request matrix, and proves a rejected multi-sector write never touches the
//! first sector it names.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_block::{
        BlockInfo, MAX_SECTORS, Op, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE,
        decode_partition_list, decode_reply,
    };
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const EINVAL_STATUS: i32 = -(Errno::EINVAL.0 as i32);
    const RESPONSE_LEN: usize = REPLY_HEADER_LEN + 4 + 128 * 64;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("blockd: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("blockd: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let handle = client
            .open("block:0", READ_WRITE)
            .map_err(|error| format!("open block:0: {}", error.description()))?;
        let mut response = vec![0u8; RESPONSE_LEN];
        let result = checks(&client, handle, &mut response);
        let _ = client.close(handle);
        result
    }

    fn checks(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        info(client, handle, response)?;
        read_mbr(client, handle, response)?;
        write_read_flush(client, handle, response)?;
        list(client, handle, response)?;
        malformed(client, handle, response)?;
        no_partial_write(client, handle, response)?;
        Ok(())
    }

    fn info(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let request = encoded(Op::Info, 0, 0, &[])?;
        let (status, length) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("INFO status {status}"));
        }
        let info = BlockInfo::decode(&response[REPLY_HEADER_LEN..length])
            .map_err(|_| "decode INFO payload".to_string())?;
        if info.sector_size != SECTOR_SIZE {
            return Err(format!("sector size is {}", info.sector_size));
        }
        if info.read_only != 0 {
            return Err("fixture disk is read-only".to_string());
        }
        if info.sectors != 2048 {
            return Err(format!("sector count is {}", info.sectors));
        }
        raven_rt::println!(
            "blockd: probe info sectors={} ro={}",
            info.sectors,
            info.read_only
        );
        Ok(())
    }

    fn read_mbr(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let mbr = read_sectors(client, handle, 0, 1, response)?;
        if mbr[510] != 0x55 || mbr[511] != 0xaa {
            return Err("sector 0 has no MBR signature".to_string());
        }
        if mbr[450] != 0xee {
            return Err("sector 0 has no protective GPT entry".to_string());
        }
        raven_rt::println!("blockd: probe mbr ok");
        Ok(())
    }

    fn write_read_flush(
        client: &Client,
        handle: Handle,
        response: &mut [u8],
    ) -> Result<(), String> {
        let pattern: Vec<u8> = (0..1024).map(|index| (index as u8) ^ 0xa5).collect();
        write_sectors(client, handle, 300, &pattern, response)?;
        let read = read_sectors(client, handle, 300, 2, response)?;
        if read != pattern {
            return Err("WRITE/READ round trip mismatch".to_string());
        }
        let request = encoded(Op::Flush, 0, 0, &[])?;
        let (status, length) = call(client, handle, &request, response)?;
        if status != 0 || length != REPLY_HEADER_LEN {
            return Err(format!("FLUSH status {status} length {length}"));
        }
        raven_rt::println!("blockd: probe write-read-flush ok");
        Ok(())
    }

    fn list(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let request = encoded(Op::List, 0, 0, &[])?;
        let (status, length) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("LIST status {status}"));
        }
        let partitions = decode_partition_list(&response[REPLY_HEADER_LEN..length])
            .map_err(|_| "decode LIST payload".to_string())?;
        if partitions.len() != 2 {
            return Err(format!("LIST returned {} partitions", partitions.len()));
        }
        let expected = [(1u32, 34u64, 67u64, "first"), (2, 101, 100, "second")];
        for (record, (index, start, sectors, name)) in partitions.iter().zip(expected) {
            let actual = core::str::from_utf8(&record.name[..record.name_len as usize])
                .map_err(|_| "partition name is not UTF-8".to_string())?;
            if record.index != index || record.start_lba != start || record.sectors != sectors {
                return Err(format!("LIST record {index} has the wrong geometry"));
            }
            if actual != name {
                return Err(format!(
                    "LIST record {index} is named {actual}, expected {name}"
                ));
            }
            raven_rt::println!(
                "blockd: probe part {} {} {}..{}",
                record.index,
                actual,
                record.start_lba,
                record.start_lba + record.sectors - 1
            );
        }
        Ok(())
    }

    fn malformed(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let mut cases: Vec<(&str, Vec<u8>)> = Vec::new();

        cases.push(("short header", vec![0u8; REQUEST_HEADER_LEN - 1]));

        let mut bad = encoded(Op::Info, 0, 0, &[])?;
        bad[0] = 2;
        cases.push(("bad version", bad));

        let mut bad = encoded(Op::Read, 0, 1, &[])?;
        bad[2..4].copy_from_slice(&99u16.to_le_bytes());
        cases.push(("unknown op", bad));

        let mut bad = encoded(Op::Read, 0, 1, &[])?;
        bad[16] = 1;
        cases.push(("nonzero flags", bad));

        let mut bad = encoded(Op::Read, 0, 1, &[])?;
        bad[12..16].copy_from_slice(&0u32.to_le_bytes());
        cases.push(("zero read count", bad));

        let mut bad = encoded(Op::Read, 0, 1, &[])?;
        bad[12..16].copy_from_slice(&(MAX_SECTORS + 1).to_le_bytes());
        cases.push(("oversized read count", bad));

        let mut bad = encoded(Op::Write, 0, 1, &[0u8; 512])?;
        bad[20..24].copy_from_slice(&0u32.to_le_bytes());
        cases.push(("write length zero with payload", bad));

        let mut bad = encoded(Op::Write, 0, 1, &[0u8; 512])?;
        bad.truncate(REQUEST_HEADER_LEN + 256);
        cases.push(("short write payload", bad));

        let mut bad = encoded(Op::Flush, 0, 0, &[])?;
        bad[4..12].copy_from_slice(&1u64.to_le_bytes());
        cases.push(("control request with sector", bad));

        let mut bad = encoded(Op::Info, 0, 0, &[])?;
        bad.push(0);
        cases.push(("trailing byte", bad));

        cases.push(("read past the end", encoded(Op::Read, 2048, 1, &[])?));
        cases.push(("read sector overflow", encoded(Op::Read, u64::MAX, 2, &[])?));
        cases.push((
            "write past the end",
            encoded(Op::Write, 2048, 1, &[0x11; 512])?,
        ));

        for (label, request) in cases {
            let (status, length) = call(client, handle, &request, response)?;
            if status != EINVAL_STATUS {
                return Err(format!("{label}: status {status}, expected -EINVAL"));
            }
            if length != REPLY_HEADER_LEN {
                return Err(format!("{label}: error reply carried {} bytes", length));
            }
        }
        raven_rt::println!("blockd: probe malformed ok");
        Ok(())
    }

    fn no_partial_write(
        client: &Client,
        handle: Handle,
        response: &mut [u8],
    ) -> Result<(), String> {
        // The image has 2048 sectors.  A two-sector write starting at 2047 is
        // one sector too long: the whole request must be rejected and the
        // backup header at 2047 left byte-for-byte unchanged.
        let before = read_sectors(client, handle, 2047, 1, response)?;
        let request = encoded(Op::Write, 2047, 2, &[0x5a; 1024])?;
        let (status, _) = call(client, handle, &request, response)?;
        if status != EINVAL_STATUS {
            return Err(format!("partial write status {status}"));
        }
        let after = read_sectors(client, handle, 2047, 1, response)?;
        if before != after {
            return Err("rejected write modified sector 2047".to_string());
        }
        let pattern = read_sectors(client, handle, 300, 1, response)?;
        if !pattern.iter().any(|byte| *byte != 0) {
            return Err("earlier write disappeared after rejected requests".to_string());
        }
        raven_rt::println!("blockd: probe no-partial-write ok");
        Ok(())
    }

    fn read_sectors(
        client: &Client,
        handle: Handle,
        sector: u64,
        count: u32,
        response: &mut [u8],
    ) -> Result<Vec<u8>, String> {
        let request = encoded(Op::Read, sector, count, &[])?;
        let (status, length) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("READ sector {sector} status {status}"));
        }
        let bytes = usize::try_from(count)
            .map_err(|_| "sector count overflow".to_string())?
            .checked_mul(SECTOR_SIZE as usize)
            .ok_or_else(|| "sector count overflow".to_string())?;
        if length != REPLY_HEADER_LEN + bytes {
            return Err(format!(
                "READ sector {sector}: {} payload bytes, expected {bytes}",
                length - REPLY_HEADER_LEN
            ));
        }
        Ok(response[REPLY_HEADER_LEN..length].to_vec())
    }

    fn write_sectors(
        client: &Client,
        handle: Handle,
        sector: u64,
        data: &[u8],
        response: &mut [u8],
    ) -> Result<(), String> {
        let count = u32::try_from(data.len() / SECTOR_SIZE as usize)
            .map_err(|_| "sector count overflow".to_string())?;
        let request = encoded(Op::Write, sector, count, data)?;
        let (status, length) = call(client, handle, &request, response)?;
        if status != 0 || length != REPLY_HEADER_LEN {
            return Err(format!(
                "WRITE sector {sector} status {status} length {length}"
            ));
        }
        Ok(())
    }

    fn encoded(op: Op, sector: u64, count: u32, data: &[u8]) -> Result<Vec<u8>, String> {
        let request = Request {
            op,
            sector,
            count,
            data,
        };
        let mut bytes = vec![0u8; REQUEST_HEADER_LEN + data.len()];
        let length = request
            .encode(&mut bytes)
            .map_err(|_| "encode block request".to_string())?;
        bytes.truncate(length);
        Ok(bytes)
    }

    fn call(
        client: &Client,
        handle: Handle,
        request: &[u8],
        response: &mut [u8],
    ) -> Result<(i32, usize), String> {
        let length = client
            .call(handle, request, response)
            .map_err(|error| format!("CALL: {}", error.description()))?;
        let (status, _) =
            decode_reply(&response[..length]).map_err(|_| "decode block reply".to_string())?;
        Ok((status, length))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
