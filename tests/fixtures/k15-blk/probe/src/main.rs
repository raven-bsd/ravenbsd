// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `block:` client used by the `virtio-blk` qtest.
//!
//! It opens `block:0` (the server on the virtio-blk driver), checks the geometry, reads the
//! `pattern` GPT partition through the block layer and verifies the driver's deterministic
//! pattern, then flushes.  The flush is the qtest's end marker: the driver leaves its test-serving
//! state when a client flushes.

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

    use raven_abi::types::Handle;
    use raven_proto_block::{
        BlockInfo, Op, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE,
        decode_partition_list, decode_reply,
    };
    use raven_rt::entry;
    use raven_scheme::Client;
    use virtio_blk::{Hasher, verify_pattern};

    entry!(main);

    const READ_WRITE: u32 = 3;
    const PARTITION_FILL: u8 = 0x3C;
    const CHUNK_SECTORS: u32 = 64;
    const RESPONSE_LEN: usize =
        REPLY_HEADER_LEN + (CHUNK_SECTORS as usize) * (SECTOR_SIZE as usize);
    const REQUEST_LEN: usize =
        REQUEST_HEADER_LEN + (CHUNK_SECTORS as usize) * (SECTOR_SIZE as usize);
    const EXPECTED_SECTORS: u64 = 2_097_152;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("k15: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("k15: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let handle = open_block(&client)?;
        let mut response = vec![0u8; RESPONSE_LEN];
        let result = checks(&client, handle, &mut response);
        let _ = client.close(handle);
        result
    }

    /// The server may still be scanning the GPT when this one-shot probe starts; poll for up to
    /// five minutes.
    fn open_block(client: &Client) -> Result<Handle, String> {
        let mut attempts = 0u32;
        loop {
            match client.open("block:0", READ_WRITE) {
                Ok(handle) => return Ok(handle),
                Err(error) => {
                    if attempts == 0 {
                        raven_rt::println!("k15: waiting for block:0: {}", error.description());
                    }
                    attempts = attempts.saturating_add(1);
                    if attempts >= 3000 {
                        return Err(format!("open block:0: {}", error.description()));
                    }
                    let _ = raven_rt::nanosleep(100_000_000);
                }
            }
        }
    }

    fn checks(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let info = info(client, handle, response)?;
        let partitions = list(client, handle, response)?;
        let partition = partitions
            .first()
            .ok_or_else(|| "GPT has no partition".to_string())?;
        if partition.name_len as usize != "pattern".len()
            || partition.name.get(..partition.name_len as usize) != Some(b"pattern".as_slice())
        {
            return Err("partition is not named pattern".to_string());
        }
        if partition.start_lba != 34 || partition.sectors != 1767 {
            return Err(format!(
                "partition is {}..{}",
                partition.start_lba,
                partition.start_lba + partition.sectors - 1
            ));
        }
        raven_rt::println!(
            "k15: probe part {} pattern {}..{}",
            partition.index,
            partition.start_lba,
            partition.start_lba + partition.sectors - 1
        );
        let checksum = pattern(
            client,
            handle,
            response,
            partition.start_lba,
            partition.sectors,
        )?;
        raven_rt::println!(
            "k15: probe pattern sectors={} checksum={checksum:#018x}",
            partition.sectors
        );
        flush(client, handle, response)?;
        wait_for_devd(client)?;
        if info.sectors != EXPECTED_SECTORS {
            return Err(format!("disk has {} sectors", info.sectors));
        }
        Ok(())
    }

    /// The probe is a `readiness = "exit"` service, so it keeps svcd from settling while devd runs
    /// its second driver cycle. Wait for devd to stop, then let the boot finish.
    fn wait_for_devd(client: &Client) -> Result<(), String> {
        let mut attempts = 0u32;
        loop {
            if let Ok(line) = devd_state(client)
                && (line.contains(" stopped")
                    || line.contains(" failed")
                    || line.contains(" gave-up"))
            {
                raven_rt::println!("k15: probe devd done");
                return Ok(());
            }
            attempts = attempts.saturating_add(1);
            if attempts >= 3000 {
                return Err("devd did not finish the second cycle".to_string());
            }
            let _ = raven_rt::nanosleep(100_000_000);
        }
    }

    fn devd_state(client: &Client) -> Result<String, String> {
        let handle = client
            .open("svc:/devd", 1)
            .map_err(|error| format!("open svc:/devd: {}", error.description()))?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 128];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("read svc:/devd: {}", error.description()))?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
        }
        let _ = client.close(handle);
        core::str::from_utf8(&data)
            .map(|line| line.to_string())
            .map_err(|_| "svc line is not UTF-8".to_string())
    }

    fn info(client: &Client, handle: Handle, response: &mut [u8]) -> Result<BlockInfo, String> {
        let request = encoded(Op::Info, 0, 0, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("INFO status {status}"));
        }
        let info = BlockInfo::decode(payload).map_err(|_| "decode INFO".to_string())?;
        if info.sector_size != SECTOR_SIZE {
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
            "k15: probe info sectors={} ro={}",
            info.sectors,
            info.read_only
        );
        Ok(info)
    }

    fn list(
        client: &Client,
        handle: Handle,
        response: &mut [u8],
    ) -> Result<Vec<raven_proto_block::PartitionRecord>, String> {
        let request = encoded(Op::List, 0, 0, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 {
            return Err(format!("LIST status {status}"));
        }
        let partitions = decode_partition_list(payload).map_err(|_| "decode LIST".to_string())?;
        raven_rt::println!("k15: probe list partitions={}", partitions.len());
        Ok(partitions)
    }

    fn pattern(
        client: &Client,
        handle: Handle,
        response: &mut [u8],
        first: u64,
        sectors: u64,
    ) -> Result<u64, String> {
        let mut hasher = Hasher::new();
        let mut request = vec![0u8; REQUEST_LEN];
        let mut done = 0u64;
        while done < sectors {
            let count = (sectors - done).min(CHUNK_SECTORS as u64) as u32;
            let length = encoded_into(Op::Read, first + done, count, &[], &mut request)?;
            let (status, payload) = call(client, handle, &request[..length], response)?;
            if status != 0 {
                return Err(format!("READ status {status} at sector {}", first + done));
            }
            if payload.len() != count as usize * SECTOR_SIZE as usize {
                return Err(format!(
                    "READ returned {} bytes for {count} sectors",
                    payload.len()
                ));
            }
            if !verify_pattern(first + done, PARTITION_FILL, payload) {
                return Err(format!("pattern mismatch at sector {}", first + done));
            }
            hasher.update(payload);
            done += u64::from(count);
        }
        Ok(hasher.finish())
    }

    fn flush(client: &Client, handle: Handle, response: &mut [u8]) -> Result<(), String> {
        let request = encoded(Op::Flush, 0, 0, &[])?;
        let (status, payload) = call(client, handle, &request, response)?;
        if status != 0 || !payload.is_empty() {
            return Err(format!("FLUSH status {status}"));
        }
        raven_rt::println!("k15: probe flush ok");
        Ok(())
    }

    fn encoded(op: Op, sector: u64, count: u32, data: &[u8]) -> Result<Vec<u8>, String> {
        let mut request = vec![0u8; REQUEST_HEADER_LEN + data.len()];
        let length = encoded_into(op, sector, count, data, &mut request)?;
        request.truncate(length);
        Ok(request)
    }

    fn encoded_into(
        op: Op,
        sector: u64,
        count: u32,
        data: &[u8],
        out: &mut [u8],
    ) -> Result<usize, String> {
        Request {
            op,
            sector,
            count,
            data,
        }
        .encode(out)
        .map_err(|_| "encode request".to_string())
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
