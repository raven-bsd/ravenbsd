// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `block:` block-device server.
//!
//! K14 serves one deterministic RAM disk loaded from `initfs:/etc/blockd.img`
//! and scans its GPT before accepting clients.  A real storage driver supplies
//! the backend in K15.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

mod device;
mod handler;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_proto_block::{GptError, MAX_PARTITIONS, PartitionRecord, gpt};
    use raven_rt::entry;
    use raven_scheme::{Client, Server};

    use crate::device::RamDisk;
    use crate::handler::{BlockHandler, partition_record};

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// Largest image the RAM backend will load.
    const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
    /// Default image when the service manifest does not override it.
    const DEFAULT_IMAGE: &str = "initfs:/etc/blockd.img";

    fn main(args: raven_rt::Args) -> i32 {
        let path = args.env_get("BLOCKD_IMAGE").unwrap_or(DEFAULT_IMAGE);
        let image = match load_image(path) {
            Ok(image) => image,
            Err(message) => {
                raven_rt::eprintln!("blockd: {message}");
                return 1;
            }
        };
        let gpt = match gpt::scan(&image) {
            Ok(gpt) => gpt,
            Err(error) => {
                raven_rt::eprintln!("blockd: gpt: {}", error_name(error));
                return 1;
            }
        };
        if gpt.partitions.len() > MAX_PARTITIONS {
            raven_rt::eprintln!(
                "blockd: gpt: {} partitions exceed the protocol limit {}",
                gpt.partitions.len(),
                MAX_PARTITIONS
            );
            return 1;
        }
        let records: Vec<PartitionRecord> = gpt.partitions.iter().map(partition_record).collect();
        raven_rt::println!("blockd: gpt ok partitions={}", records.len());
        for partition in &gpt.partitions {
            raven_rt::println!(
                "blockd: part {} {} {}..{}",
                partition.index,
                partition.name,
                partition.start_lba,
                partition.last_lba
            );
        }

        let disk = match RamDisk::new(image, false) {
            Ok(disk) => disk,
            Err(error) => {
                raven_rt::eprintln!("blockd: ram disk: {}", error.description());
                return 1;
            }
        };
        let mut handler = match BlockHandler::new(disk, records) {
            Ok(handler) => handler,
            Err(error) => {
                raven_rt::eprintln!("blockd: handler: {}", error.description());
                return 1;
            }
        };
        let mut server = match Server::register("block") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("blockd: {}", error.description());
                return 1;
            }
        };
        log_started("blockd");
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

    fn load_image(path: &str) -> Result<Vec<u8>, String> {
        let client = Client::new();
        let handle = client
            .open(path, 1)
            .map_err(|error| format!("open {path}: {}", error.description()))?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = match client.read(handle, 0, &mut buffer) {
                Ok(read) => read,
                Err(error) => {
                    let _ = client.close(handle);
                    return Err(format!("read {path}: {}", error.description()));
                }
            };
            if read == 0 {
                break;
            }
            let next = data
                .len()
                .checked_add(read)
                .ok_or_else(|| format!("read {path}: size overflow"))?;
            if next > MAX_IMAGE_BYTES {
                let _ = client.close(handle);
                return Err(format!(
                    "read {path}: image exceeds {MAX_IMAGE_BYTES} bytes"
                ));
            }
            data.extend_from_slice(&buffer[..read]);
        }
        let _ = client.close(handle);
        if data.is_empty() {
            return Err(format!("read {path}: empty image"));
        }
        Ok(data)
    }

    fn error_name(error: GptError) -> String {
        match error {
            GptError::TooShort => "too short".to_string(),
            GptError::NoProtectiveMbr => "no protective MBR".to_string(),
            GptError::BadHeader => "bad header".to_string(),
            GptError::BadHeaderCrc => "bad header CRC".to_string(),
            GptError::BadEntryCrc => "bad entry CRC".to_string(),
            GptError::BadEntry => "bad entry".to_string(),
            GptError::BadName => "bad name".to_string(),
            GptError::BackupMismatch => "backup mismatch".to_string(),
        }
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
