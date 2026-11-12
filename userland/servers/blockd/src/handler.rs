// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `block:` request handler.
//!
//! The handler is independent of the target runtime so the validation matrix
//! runs under `cargo test`.  Every request is decoded into a bounded structure,
//! its sector range is checked before the backend is touched, and a bad
//! request gets a wire reply with `-EINVAL` instead of a partial operation.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_block::{
    BlockInfo, INFO_LEN, LIST_HEADER_LEN, MAX_PARTITIONS, MAX_SECTORS, Op, PARTITION_LEN,
    Partition, PartitionRecord, ProtoError as BlockProtoError, REPLY_HEADER_LEN, Request,
    SECTOR_SIZE, encode_partition_list, encode_reply, write_reply_header,
};
use raven_proto_fs::OpenFlags;
use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened};

use crate::device::Device;

/// Aggregate open-file-description budget.
pub const MAX_OPEN_FILES: usize = 64;

/// The single device path served in this milestone.
pub const DEVICE_PATH: &str = "0";

/// Build the stable wire record for one GPT partition.
///
/// GPT names are 36 UTF-16 code units; the wire field holds 40 UTF-8 bytes.
/// A name longer than the field is shortened on a character boundary and its
/// stored length is reduced accordingly.
pub fn partition_record(partition: &Partition) -> PartitionRecord {
    let mut name = [0u8; 40];
    let mut length = partition.name.len().min(name.len());
    while !partition.name.is_char_boundary(length) {
        length -= 1;
    }
    name[..length].copy_from_slice(&partition.name.as_bytes()[..length]);
    PartitionRecord {
        index: partition.index,
        start_lba: partition.start_lba,
        sectors: partition.sectors(),
        name,
        name_len: length as u8,
    }
}

/// A validated block request handler.
pub struct BlockHandler<D: Device> {
    device: D,
    partitions: Vec<PartitionRecord>,
    opens: OpenTable<()>,
    pending: Option<(u64, String)>,
}

impl<D: Device> BlockHandler<D> {
    /// Create a handler over `device` and its scanned partition list.
    pub fn new(device: D, partitions: Vec<PartitionRecord>) -> Result<Self, Errno> {
        if partitions.len() > MAX_PARTITIONS {
            return Err(Errno::EINVAL);
        }
        Ok(Self {
            device,
            partitions,
            opens: OpenTable::new(),
            pending: None,
        })
    }

    /// Record the kernel file id and path of the next `OPEN`.
    ///
    /// `raven-scheme` does not pass the kernel file id to the handler, so the
    /// server loop and host tests set this before dispatch.
    pub fn expect_open(&mut self, file_id: u64, path: &str) {
        self.pending = Some((file_id, path.to_string()));
    }

    fn reply_ok(&self, response: &mut [u8]) -> HandlerResult<usize> {
        encode_reply(response, 0, &[]).map_err(|_| Errno::EPROTO.into())
    }

    fn reply_error(&self, response: &mut [u8], error: Errno) -> HandlerResult<usize> {
        encode_reply(response, -i32::from(error.0), &[]).map_err(|_| Errno::EPROTO.into())
    }
}

fn checked_bytes(info: BlockInfo, sector: u64, count: u32) -> Result<usize, Errno> {
    if info.sector_size != SECTOR_SIZE || count == 0 || count > MAX_SECTORS {
        return Err(Errno::EINVAL);
    }
    let end = sector.checked_add(u64::from(count)).ok_or(Errno::EINVAL)?;
    if end > info.sectors {
        return Err(Errno::EINVAL);
    }
    usize::try_from(count)
        .map_err(|_| Errno::EINVAL)?
        .checked_mul(SECTOR_SIZE as usize)
        .ok_or(Errno::EINVAL)
}

impl<D: Device> Handler for BlockHandler<D> {
    fn open_with(
        &mut self,
        path: &str,
        flags: OpenFlags,
        _mode: u32,
        _ctx: &Ctx,
    ) -> HandlerResult<Opened> {
        let Some((file_id, expected)) = self.pending.take() else {
            return Err(Errno::EINVAL.into());
        };
        // The scheme root (`block:`) is an alias for device 0; svcd opens it
        // while waiting for `readiness = "scheme"`.
        if expected != path || (!path.is_empty() && path != DEVICE_PATH) {
            return Err(Errno::ENOENT.into());
        }
        if !matches!(
            flags,
            OpenFlags::READ | OpenFlags::WRITE | OpenFlags::READ_WRITE
        ) {
            return Err(Errno::EINVAL.into());
        }
        if self.device.info().read_only != 0 && flags.writable() {
            return Err(Errno::EROFS.into());
        }
        if self.opens.len() >= MAX_OPEN_FILES {
            return Err(Errno::EMFILE.into());
        }
        self.opens.insert_with_id(file_id, (), flags)?;
        Ok(Opened::File(file_id))
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        self.opens.close(file)?;
        Ok(())
    }

    fn dup(&mut self, file: u64) -> HandlerResult<u64> {
        Ok(self.opens.dup(file)?)
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        let flags = self.opens.get(file).ok_or(Errno::EBADF)?.flags;
        let decoded = match Request::decode(request) {
            Ok(decoded) => decoded,
            Err(_) => return self.reply_error(response, Errno::EINVAL),
        };
        let info = self.device.info();
        match decoded.op {
            Op::Read => {
                if !flags.readable() {
                    return self.reply_error(response, Errno::EBADF);
                }
                let bytes = match checked_bytes(info, decoded.sector, decoded.count) {
                    Ok(bytes) => bytes,
                    Err(error) => return self.reply_error(response, error),
                };
                let total = REPLY_HEADER_LEN + bytes;
                if response.len() < total {
                    return self.reply_error(response, Errno::EINVAL);
                }
                if let Err(error) = self
                    .device
                    .read_sectors(decoded.sector, &mut response[REPLY_HEADER_LEN..total])
                {
                    return self.reply_error(response, error);
                }
                write_reply_header(response, 0, bytes).map_err(|_| Errno::EPROTO)?;
                Ok(total)
            }
            Op::Write => {
                if !flags.writable() {
                    return self.reply_error(response, Errno::EBADF);
                }
                if let Err(error) = checked_bytes(info, decoded.sector, decoded.count) {
                    return self.reply_error(response, error);
                }
                if let Err(error) = self.device.write_sectors(decoded.sector, decoded.data) {
                    return self.reply_error(response, error);
                }
                self.reply_ok(response)
            }
            Op::Flush => {
                if let Err(error) = self.device.flush() {
                    return self.reply_error(response, error);
                }
                self.reply_ok(response)
            }
            Op::Info => {
                let mut payload = [0u8; INFO_LEN];
                if info.encode(&mut payload).is_err() {
                    return self.reply_error(response, Errno::EPROTO);
                }
                encode_reply(response, 0, &payload).map_err(|_| Errno::EPROTO.into())
            }
            Op::List => {
                let payload_len = LIST_HEADER_LEN + self.partitions.len() * PARTITION_LEN;
                let total = REPLY_HEADER_LEN + payload_len;
                if response.len() < total {
                    return self.reply_error(response, Errno::EINVAL);
                }
                let written = match encode_partition_list(
                    &mut response[REPLY_HEADER_LEN..total],
                    &self.partitions,
                ) {
                    Ok(written) => written,
                    Err(BlockProtoError::BadLength) => {
                        return self.reply_error(response, Errno::EINVAL);
                    }
                    Err(_) => return self.reply_error(response, Errno::EPROTO),
                };
                write_reply_header(response, 0, written).map_err(|_| Errno::EPROTO)?;
                Ok(total)
            }
        }
    }
}

#[cfg(test)]
#[path = "handler_ut.rs"]
mod tests;
