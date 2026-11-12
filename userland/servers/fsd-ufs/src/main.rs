// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD read-only UFS filesystem server (`ufs:` scheme).
//!
//! The server selects one GPT partition from a `block:` device, mounts it
//! read-only through `ufs-core`, and serves the tree to `vfsd`.

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

    use fsd_ufs::{FsHandler, UfsView};
    use raven_abi::types::Handle;
    use raven_proto_block::{
        BlockInfo, INFO_LEN, LIST_HEADER_LEN, MAX_PARTITIONS, MAX_SECTORS, Op, PARTITION_LEN,
        REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request, SECTOR_SIZE, decode_partition_list,
        decode_reply,
    };
    use raven_rt::entry;
    use raven_scheme::{Client, Server};
    use ufs_core::{BlockIo, IoError};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    const DEFAULT_DEVICE: &str = "block:0";
    const DEFAULT_SCHEME: &str = "ufs";
    const DEFAULT_PARTITION: u32 = 1;
    const REPLY_BUFFER: usize = REPLY_HEADER_LEN + MAX_SECTORS as usize * SECTOR_SIZE as usize;

    /// A `BlockIo` over one `block:` open, optionally offset to a partition.
    struct RemoteBlock {
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

        fn write_at(&mut self, _offset: u64, _buffer: &[u8]) -> Result<(), IoError> {
            Err(IoError)
        }

        fn flush(&mut self) -> Result<(), IoError> {
            Ok(())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let device = args.env_get("FSD_UFS_DEVICE").unwrap_or(DEFAULT_DEVICE);
        let scheme = args
            .env_get("FSD_UFS_SCHEME")
            .unwrap_or(DEFAULT_SCHEME)
            .trim_end_matches(':');
        let partition = match args.env_get("FSD_UFS_PARTITION") {
            Some(value) => match value.parse::<u32>() {
                Ok(partition) => partition,
                Err(_) => {
                    raven_rt::eprintln!("fsd-ufs: invalid FSD_UFS_PARTITION={value}");
                    return 1;
                }
            },
            None => DEFAULT_PARTITION,
        };
        let disk = match select_device(device, partition) {
            Ok(disk) => disk,
            Err(message) => {
                raven_rt::eprintln!("fsd-ufs: {message}");
                return 1;
            }
        };
        let size = disk.size_bytes();
        let view = match UfsView::mount(disk, true) {
            Ok(view) => view,
            Err(error) => {
                raven_rt::eprintln!("fsd-ufs: mount {device}: {error}");
                return 1;
            }
        };
        raven_rt::println!(
            "fsd-ufs: mounted {device} partition {partition} as {scheme}: ({size} bytes)"
        );
        let mut handler = FsHandler::new(view);
        let mut server = match Server::register(scheme) {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("fsd-ufs: {}", error.description());
                return 1;
            }
        };
        log_started("fsd-ufs");
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == OPEN_OPCODE
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.expect_open(packet.packet.file_id, path);
            }
            let _ = server.dispatch(packet, &mut handler);
        }
    }

    fn select_device(device: &str, partition: u32) -> Result<RemoteBlock, String> {
        let client = Client::new();
        let handle = client
            .open(device, 1)
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

    fn block_partitions(
        client: &Client,
        handle: Handle,
    ) -> Result<Vec<raven_proto_block::PartitionRecord>, String> {
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

    fn log_started(name: &str) {
        let client = Client::new();
        let path = format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
