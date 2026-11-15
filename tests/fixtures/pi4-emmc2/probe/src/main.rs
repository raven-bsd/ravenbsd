// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `disk:` client used by the `pi4-emmc2` qtest.
//!
//! It opens the driver's host-backed disk, checks the geometry, reads the marker and the
//! deterministic pattern through the block framing, writes and verifies one scratch sector, then
//! flushes. The flush ends the driver's test-serving cycle.

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

    use emmc2::pattern::{self, Hasher, SECTOR_SIZE};
    use raven_abi::types::Handle;
    use raven_proto_block::{
        BlockInfo, Op, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE as BLOCK_SECTOR,
        decode_reply,
    };
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ_WRITE: u32 = 3;
    const CHUNK_SECTORS: u32 = 64;
    const RESPONSE_LEN: usize = REPLY_HEADER_LEN + (CHUNK_SECTORS as usize) * BLOCK_SECTOR as usize;
    const REQUEST_LEN: usize =
        REQUEST_HEADER_LEN + (CHUNK_SECTORS as usize) * BLOCK_SECTOR as usize;
    const EXPECTED_SECTORS: u64 = 8192;
    const PATTERN_FIRST: u64 = 2048;
    const PATTERN_SECTORS: u64 = 2048;
    const PATTERN_FILL: u8 = 0xa5;
    const MARKER_LBA: u64 = 1900;
    const SCRATCH_LBA: u64 = 7000;
    const SCRATCH_FILL: u8 = 0x5a;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("pi4-emmc2: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("pi4-emmc2: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let handle = open_disk(&client)?;
        let mut response = vec![0u8; RESPONSE_LEN];
        let result = checks(&client, handle, &mut response);
        let _ = client.close(handle);
        result
    }

    fn open_disk(client: &Client) -> Result<Handle, String> {
        let mut attempts = 0u32;
        loop {
            match client.open("disk:0", READ_WRITE) {
                Ok(handle) => return Ok(handle),
                Err(error) => {
                    if attempts == 0 {
                        raven_rt::println!(
                            "pi4-emmc2: waiting for disk:0: {}",
                            error.description()
                        );
                    }
                    attempts = attempts.saturating_add(1);
                    if attempts >= 3000 {
                        return Err(format!("open disk:0: {}", error.description()));
                    }
                    let _ = raven_rt::nanosleep(100_000_000);
                }
            }
        }
    }

    fn checks(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        info(client, handle, response)?;
        marker(client, handle, response)?;
        pattern(client, handle, response)?;
        scratch(client, handle)?;
        flush(client, handle, response)?;
        Ok(())
    }

    fn info(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let request = encoded(Op::Info, 0, 0, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("INFO status {status}"));
        }
        let info = BlockInfo::decode(payload).map_err(|_| "decode INFO".to_string())?;
        if info.sector_size != BLOCK_SECTOR {
            return Err(format!("sector size is {}", info.sector_size));
        }
        if info.read_only != 0 {
            return Err("disk is read-only".to_string());
        }
        if info.sectors != EXPECTED_SECTORS {
            return Err(format!(
                "INFO sectors={}, expected {EXPECTED_SECTORS}",
                info.sectors
            ));
        }
        raven_rt::println!(
            "pi4-emmc2: probe info sectors={} ro={}",
            info.sectors,
            info.read_only
        );
        Ok(())
    }

    fn marker(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let mut sector = [0u8; SECTOR_SIZE];
        read_sectors(client, handle, MARKER_LBA, &mut sector, response)?;
        let checksum = pattern::marker_decode(&sector).ok_or_else(|| "bad marker".to_string())?;
        raven_rt::println!("pi4-emmc2: probe marker checksum={checksum:#018x}");
        Ok(())
    }

    fn pattern(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let mut hasher = Hasher::new();
        let mut chunk = vec![0u8; CHUNK_SECTORS as usize * SECTOR_SIZE];
        let mut done = 0u64;
        while done < PATTERN_SECTORS {
            let count = (PATTERN_SECTORS - done).min(CHUNK_SECTORS as u64) as usize;
            let bytes = count * SECTOR_SIZE;
            read_sectors(
                client,
                handle,
                PATTERN_FIRST + done,
                &mut chunk[..bytes],
                response,
            )?;
            if !pattern::verify_pattern(PATTERN_FIRST + done, PATTERN_FILL, &chunk[..bytes]) {
                return Err(format!(
                    "pattern mismatch at sector {}",
                    PATTERN_FIRST + done
                ));
            }
            hasher.update(&chunk[..bytes]);
            done += count as u64;
        }
        let checksum = hasher.finish();
        let marker_sector = {
            let mut marker = [0u8; SECTOR_SIZE];
            read_sectors(client, handle, MARKER_LBA, &mut marker, response)?;
            marker
        };
        let expected =
            pattern::marker_decode(&marker_sector).ok_or_else(|| "bad marker".to_string())?;
        if checksum != expected {
            return Err(format!(
                "pattern checksum {checksum:#018x} != marker {expected:#018x}"
            ));
        }
        raven_rt::println!(
            "pi4-emmc2: probe pattern sectors={PATTERN_SECTORS} checksum={checksum:#018x}"
        );
        Ok(())
    }

    fn scratch(client: &Client, handle: Handle) -> Result<(), String> {
        let mut data = [0u8; SECTOR_SIZE];
        pattern::fill_pattern(SCRATCH_LBA, SCRATCH_FILL, &mut data);
        let request = encoded(Op::Write, SCRATCH_LBA, 1, &data)?;
        let mut response = vec![0u8; REPLY_HEADER_LEN];
        let (status, payload) = call(client, handle, &request, &mut response)?;
        if status != 0 || !payload.is_empty() {
            return Err(format!("WRITE status {status}"));
        }
        let mut readback = [0u8; SECTOR_SIZE];
        let mut response = vec![0u8; RESPONSE_LEN];
        read_sectors(client, handle, SCRATCH_LBA, &mut readback, &mut response)?;
        if !pattern::verify_pattern(SCRATCH_LBA, SCRATCH_FILL, &readback) {
            return Err("scratch sector did not read back".to_string());
        }
        raven_rt::println!("pi4-emmc2: probe write ok");
        Ok(())
    }

    fn flush(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let request = encoded(Op::Flush, 0, 0, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 || !payload.is_empty() {
            return Err(format!("FLUSH status {status}"));
        }
        raven_rt::println!("pi4-emmc2: probe flush ok");
        Ok(())
    }

    fn read_sectors(
        client: &Client,
        handle: Handle,
        sector: u64,
        out: &mut [u8],
        response: &mut [u8],
    ) -> Result<(), String> {
        if out.is_empty() || !out.len().is_multiple_of(SECTOR_SIZE) {
            return Err("bad read length".to_string());
        }
        let count = u32::try_from(out.len() / SECTOR_SIZE).map_err(|_| "read count")?;
        let request = encoded(Op::Read, sector, count, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("READ status {status}"));
        }
        if payload.len() != out.len() {
            return Err(format!(
                "READ payload {} bytes, expected {}",
                payload.len(),
                out.len()
            ));
        }
        out.copy_from_slice(payload);
        Ok(())
    }

    fn encoded(
        op: Op,
        sector: u64,
        count: u32,
        data: &[u8],
    ) -> Result<alloc::vec::Vec<u8>, String> {
        let mut request = vec![0u8; REQUEST_LEN];
        let length = Request {
            op,
            sector,
            count,
            data,
        }
        .encode(&mut request)
        .map_err(|_| "encode request".to_string())?;
        request.truncate(length);
        Ok(request)
    }

    fn call<'a>(
        client: &Client,
        handle: Handle,
        request: &[u8],
        response: &'a mut [u8],
    ) -> Result<(i32, &'a [u8]), String> {
        let length = client
            .call(handle, request, response)
            .map_err(|error| format!("call: {}", error.description()))?;
        decode_reply(response.get(..length).ok_or("short reply")?)
            .map_err(|_| "decode reply".to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
