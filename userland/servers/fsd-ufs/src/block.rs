// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `block:` client and partition selection shared by the server and `ufs-fsck`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::types::Handle;
use raven_proto_block::{
    BlockInfo, INFO_LEN, LIST_HEADER_LEN, MAX_PARTITIONS, MAX_SECTORS, Op, PARTITION_LEN,
    PartitionRecord, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE,
    decode_partition_list, decode_reply,
};
use raven_scheme::Client;
use ufs_core::{BlockIo, IoError};

const REPLY_BUFFER: usize = REPLY_HEADER_LEN + MAX_SECTORS as usize * SECTOR_SIZE as usize;

/// A `BlockIo` over one `block:` open, optionally offset to a partition.
pub struct RemoteBlock {
    handle: Handle,
    base: u64,
    size: u64,
    response: Vec<u8>,
}

impl RemoteBlock {
    fn new(handle: Handle, base: u64, size: u64) -> Self {
        Self {
            handle,
            base,
            size,
            response: vec![0u8; REPLY_BUFFER],
        }
    }
}

impl BlockIo for RemoteBlock {
    fn size_bytes(&self) -> u64 {
        self.size
    }

    fn read_at(&mut self, offset: u64, buffer: &mut [u8]) -> Result<(), IoError> {
        let end = offset
            .checked_add(u64::try_from(buffer.len()).map_err(|_| IoError)?)
            .ok_or(IoError)?;
        if end > self.size {
            return Err(IoError);
        }
        let mut done = 0usize;
        while done < buffer.len() {
            let absolute = self
                .base
                .checked_add(offset)
                .and_then(|value| value.checked_add(done as u64))
                .ok_or(IoError)?;
            let sector = absolute / u64::from(SECTOR_SIZE);
            let in_sector =
                usize::try_from(absolute % u64::from(SECTOR_SIZE)).map_err(|_| IoError)?;
            let chunk = (buffer.len() - done).min(REPLY_BUFFER - REPLY_HEADER_LEN - in_sector);
            if chunk == 0 {
                return Err(IoError);
            }
            let count = u32::try_from((in_sector + chunk).div_ceil(SECTOR_SIZE as usize))
                .map_err(|_| IoError)?;
            if count == 0 || count > MAX_SECTORS {
                return Err(IoError);
            }
            let request = Request {
                op: Op::Read,
                sector,
                count,
                data: &[],
            };
            let mut encoded = [0u8; REQUEST_HEADER_LEN];
            let request_len = request.encode(&mut encoded).map_err(|_| IoError)?;
            let client = Client::new();
            let reply_len = client
                .call(self.handle, &encoded[..request_len], &mut self.response)
                .map_err(|_| IoError)?;
            let (status, payload) =
                decode_reply(&self.response[..reply_len]).map_err(|_| IoError)?;
            if status != 0 {
                return Err(IoError);
            }
            let expected = usize::try_from(count)
                .map_err(|_| IoError)?
                .checked_mul(SECTOR_SIZE as usize)
                .ok_or(IoError)?;
            if payload.len() != expected {
                return Err(IoError);
            }
            let source = payload.get(in_sector..in_sector + chunk).ok_or(IoError)?;
            buffer
                .get_mut(done..done + chunk)
                .ok_or(IoError)?
                .copy_from_slice(source);
            done += chunk;
        }
        Ok(())
    }

    fn write_at(&mut self, offset: u64, buffer: &[u8]) -> Result<(), IoError> {
        let end = offset
            .checked_add(u64::try_from(buffer.len()).map_err(|_| IoError)?)
            .ok_or(IoError)?;
        if end > self.size {
            return Err(IoError);
        }
        let mut done = 0usize;
        while done < buffer.len() {
            let absolute = self
                .base
                .checked_add(offset)
                .and_then(|value| value.checked_add(done as u64))
                .ok_or(IoError)?;
            let sector = absolute / u64::from(SECTOR_SIZE);
            let in_sector =
                usize::try_from(absolute % u64::from(SECTOR_SIZE)).map_err(|_| IoError)?;
            let chunk = (buffer.len() - done).min(REPLY_BUFFER - REPLY_HEADER_LEN - in_sector);
            if chunk == 0 {
                return Err(IoError);
            }
            let count = u32::try_from((in_sector + chunk).div_ceil(SECTOR_SIZE as usize))
                .map_err(|_| IoError)?;
            if count == 0 || count > MAX_SECTORS {
                return Err(IoError);
            }
            let payload_len = usize::try_from(count)
                .map_err(|_| IoError)?
                .checked_mul(SECTOR_SIZE as usize)
                .ok_or(IoError)?;
            let mut payload = vec![0u8; payload_len];
            if in_sector == 0 && chunk.is_multiple_of(SECTOR_SIZE as usize) {
                payload[..chunk].copy_from_slice(&buffer[done..done + chunk]);
            } else {
                let aligned = absolute - in_sector as u64;
                self.read_at(aligned - self.base, &mut payload)?;
                payload[in_sector..in_sector + chunk].copy_from_slice(&buffer[done..done + chunk]);
            }
            let request = Request {
                op: Op::Write,
                sector,
                count,
                data: &payload,
            };
            let mut encoded = vec![0u8; REQUEST_HEADER_LEN + payload_len];
            let request_len = request.encode(&mut encoded).map_err(|_| IoError)?;
            let client = Client::new();
            let reply_len = client
                .call(self.handle, &encoded[..request_len], &mut self.response)
                .map_err(|_| IoError)?;
            let (status, _) = decode_reply(&self.response[..reply_len]).map_err(|_| IoError)?;
            if status != 0 {
                return Err(IoError);
            }
            done += chunk;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), IoError> {
        let request = Request {
            op: Op::Flush,
            sector: 0,
            count: 0,
            data: &[],
        };
        let mut encoded = [0u8; REQUEST_HEADER_LEN];
        let request_len = request.encode(&mut encoded).map_err(|_| IoError)?;
        let client = Client::new();
        let reply_len = client
            .call(self.handle, &encoded[..request_len], &mut self.response)
            .map_err(|_| IoError)?;
        let (status, _) = decode_reply(&self.response[..reply_len]).map_err(|_| IoError)?;
        if status != 0 {
            return Err(IoError);
        }
        Ok(())
    }
}

/// Open `device`, select one GPT partition (zero means the whole device), and
/// return a byte-addressed view of it.
pub fn open_partition(device: &str, partition: u32) -> Result<RemoteBlock, String> {
    let client = Client::new();
    let handle = client
        .open(device, 3)
        .map_err(|error| format!("open {device}: {}", error.description()))?;
    let info = block_info(&client, handle)?;
    if info.sector_size != SECTOR_SIZE {
        return Err(format!(
            "{device}: sector size {}, expected {SECTOR_SIZE}",
            info.sector_size
        ));
    }
    let capacity = info
        .sectors
        .checked_mul(u64::from(SECTOR_SIZE))
        .ok_or_else(|| format!("{device}: capacity overflow"))?;
    let (base, size) = if partition == 0 {
        (0, capacity)
    } else {
        let partitions = block_partitions(&client, handle)?;
        let selected = partitions
            .iter()
            .find(|record| record.index == partition)
            .ok_or_else(|| format!("{device}: partition {partition} not found"))?;
        let base = selected
            .start_lba
            .checked_mul(u64::from(SECTOR_SIZE))
            .ok_or_else(|| format!("{device}: partition offset overflow"))?;
        let size = selected
            .sectors
            .checked_mul(u64::from(SECTOR_SIZE))
            .ok_or_else(|| format!("{device}: partition size overflow"))?;
        if base.checked_add(size).is_none_or(|end| end > capacity) {
            return Err(format!(
                "{device}: partition {partition} exceeds the device"
            ));
        }
        (base, size)
    };
    if size == 0 {
        return Err(format!("{device}: selected partition is empty"));
    }
    Ok(RemoteBlock::new(handle, base, size))
}

fn block_info(client: &Client, handle: Handle) -> Result<BlockInfo, String> {
    let request = Request {
        op: Op::Info,
        sector: 0,
        count: 0,
        data: &[],
    };
    let mut encoded = [0u8; REQUEST_HEADER_LEN];
    let request_len = request
        .encode(&mut encoded)
        .map_err(|_| "encode block INFO".to_string())?;
    let mut response = [0u8; REPLY_HEADER_LEN + INFO_LEN];
    let reply_len = client
        .call(handle, &encoded[..request_len], &mut response)
        .map_err(|error| format!("block INFO: {}", error.description()))?;
    let (status, payload) =
        decode_reply(&response[..reply_len]).map_err(|_| "decode block INFO".to_string())?;
    if status != 0 {
        return Err(format!("block INFO status {status}"));
    }
    BlockInfo::decode(payload).map_err(|_| "decode block INFO payload".to_string())
}

fn block_partitions(client: &Client, handle: Handle) -> Result<Vec<PartitionRecord>, String> {
    let request = Request {
        op: Op::List,
        sector: 0,
        count: 0,
        data: &[],
    };
    let mut encoded = [0u8; REQUEST_HEADER_LEN];
    let request_len = request
        .encode(&mut encoded)
        .map_err(|_| "encode block LIST".to_string())?;
    let mut response =
        vec![0u8; REPLY_HEADER_LEN + LIST_HEADER_LEN + MAX_PARTITIONS * PARTITION_LEN];
    let reply_len = client
        .call(handle, &encoded[..request_len], &mut response)
        .map_err(|error| format!("block LIST: {}", error.description()))?;
    let (status, payload) =
        decode_reply(&response[..reply_len]).map_err(|_| "decode block LIST".to_string())?;
    if status != 0 {
        return Err(format!("block LIST status {status}"));
    }
    decode_partition_list(payload).map_err(|_| "decode block LIST payload".to_string())
}
