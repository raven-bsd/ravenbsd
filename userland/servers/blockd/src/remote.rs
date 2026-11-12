// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Remote block backend: a `Device` over a driver's `disk:` scheme.
//!
//! The driver serves the same `raven-proto-block` framing as `blockd` itself. Every operation opens
//! `disk:0`, issues exactly one `CALL`, closes the handle and decodes the reply, so a driver
//! restart between operations cannot leave a stale handle behind.

use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_proto_block::{
    BlockInfo, MAX_SECTORS, Op, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE,
    decode_reply,
};
use raven_scheme::Client;

use crate::device::Device;

/// Wire open flags.
const OPEN_READ: u32 = 1;
const OPEN_READ_WRITE: u32 = 3;

/// A block device behind a driver scheme.
pub struct RemoteDisk {
    client: Client,
    path: String,
    info: BlockInfo,
    request: Vec<u8>,
    response: Vec<u8>,
}

impl RemoteDisk {
    /// Open `path` (for example `disk:0`) and query its geometry.
    pub fn open(path: &str) -> Result<Self, Errno> {
        let request_len = REQUEST_HEADER_LEN + MAX_SECTORS as usize * SECTOR_SIZE as usize;
        let response_len = REPLY_HEADER_LEN + MAX_SECTORS as usize * SECTOR_SIZE as usize;
        let mut disk = Self {
            client: Client::new(),
            path: path.to_string(),
            info: BlockInfo::default(),
            request: vec![0u8; request_len],
            response: vec![0u8; response_len],
        };
        let length = disk.exchange(Op::Info, 0, 0, &[], false)?;
        let payload = disk
            .response
            .get(REPLY_HEADER_LEN..REPLY_HEADER_LEN + length)
            .ok_or(Errno::EPROTO)?;
        let info = BlockInfo::decode(payload).map_err(|_| Errno::EPROTO)?;
        if info.sector_size != SECTOR_SIZE || info.sectors == 0 {
            return Err(Errno::EINVAL);
        }
        disk.info = info;
        Ok(disk)
    }

    fn exchange(
        &mut self,
        op: Op,
        sector: u64,
        count: u32,
        data: &[u8],
        writable: bool,
    ) -> Result<usize, Errno> {
        let request = Request {
            op,
            sector,
            count,
            data,
        };
        let end = REQUEST_HEADER_LEN
            .checked_add(data.len())
            .ok_or(Errno::EINVAL)?;
        let length = request
            .encode(self.request.get_mut(..end).ok_or(Errno::EINVAL)?)
            .map_err(|_| Errno::EINVAL)?;
        let flags = if writable { OPEN_READ_WRITE } else { OPEN_READ };
        let handle = self.client.open(&self.path, flags)?;
        let result = self
            .client
            .call(handle, &self.request[..length], &mut self.response);
        let _ = self.client.close(handle);
        let length = result?;
        let (status, payload) = decode_reply(self.response.get(..length).ok_or(Errno::EPROTO)?)
            .map_err(|_| Errno::EPROTO)?;
        if status != 0 {
            if (-4095..0).contains(&status) {
                return Err(Errno((-status) as u16));
            }
            return Err(Errno::EPROTO);
        }
        Ok(payload.len())
    }
}

impl Device for RemoteDisk {
    fn info(&self) -> BlockInfo {
        self.info
    }

    fn read_sectors(&mut self, sector: u64, buffer: &mut [u8]) -> Result<(), Errno> {
        if buffer.is_empty() || !buffer.len().is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        let count =
            u32::try_from(buffer.len() / SECTOR_SIZE as usize).map_err(|_| Errno::EINVAL)?;
        if count == 0 || count > MAX_SECTORS {
            return Err(Errno::EINVAL);
        }
        let length = self.exchange(Op::Read, sector, count, &[], false)?;
        if length != buffer.len() {
            return Err(Errno::EIO);
        }
        let payload = self
            .response
            .get(REPLY_HEADER_LEN..REPLY_HEADER_LEN + length)
            .ok_or(Errno::EIO)?;
        buffer.copy_from_slice(payload);
        Ok(())
    }

    fn write_sectors(&mut self, sector: u64, data: &[u8]) -> Result<(), Errno> {
        if self.info.read_only != 0 {
            return Err(Errno::EROFS);
        }
        if data.is_empty() || !data.len().is_multiple_of(SECTOR_SIZE as usize) {
            return Err(Errno::EINVAL);
        }
        let count = u32::try_from(data.len() / SECTOR_SIZE as usize).map_err(|_| Errno::EINVAL)?;
        if count == 0 || count > MAX_SECTORS {
            return Err(Errno::EINVAL);
        }
        if self.exchange(Op::Write, sector, count, data, true)? != 0 {
            return Err(Errno::EIO);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), Errno> {
        if self.exchange(Op::Flush, 0, 0, &[], false)? != 0 {
            return Err(Errno::EIO);
        }
        Ok(())
    }
}
