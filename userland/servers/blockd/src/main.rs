// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `block:` block-device server.
//!
//! Two backends: the K14 deterministic RAM image loaded from `initfs:/etc/blockd.img`, and a
//! remote device behind a driver's `disk:` scheme (`BLOCKD_DISK=disk:0`).  In both cases the GPT
//! is scanned before clients are accepted; the remote path reads at most a 1 MiB prefix, never the
//! whole disk.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

mod device;
mod handler;
#[cfg(target_os = "none")]
mod remote;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_proto_block::{Gpt, GptError, MAX_PARTITIONS, MAX_SECTORS, PartitionRecord, gpt};
    use raven_rt::entry;
    use raven_scheme::{Client, Server};

    use crate::device::{Device, RamDisk};
    use crate::handler::{BlockHandler, partition_record};
    use crate::remote::RemoteDisk;

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// Largest image the RAM backend will load.
    const MAX_IMAGE_BYTES: usize = 64 * 1024 * 1024;
    /// Default image when the service manifest does not override it.
    const DEFAULT_IMAGE: &str = "initfs:/etc/blockd.img";
    /// GPT prefix read from a remote device: MBR, headers, entry arrays and the backup copy.
    const GPT_PREFIX_SECTORS: u64 = 2048;

    fn main(args: raven_rt::Args) -> i32 {
        let result = match args.env_get("BLOCKD_DISK") {
            Some(path) if !path.is_empty() => run_remote(path),
            _ => run_image(args.env_get("BLOCKD_IMAGE").unwrap_or(DEFAULT_IMAGE)),
        };
        match result {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("blockd: {message}");
                1
            }
        }
    }

    fn run_image(path: &str) -> Result<(), String> {
        let image = load_image(path)?;
        let gpt = gpt::scan(&image).map_err(|error| format!("gpt: {}", error_name(error)))?;
        let records = partition_records(&gpt)?;
        report(&gpt, &records);
        let disk = RamDisk::new(image, false)
            .map_err(|error| format!("ram disk: {}", error.description()))?;
        let handler = BlockHandler::new(disk, records)
            .map_err(|error| format!("handler: {}", error.description()))?;
        serve(handler)
    }

    fn run_remote(path: &str) -> Result<(), String> {
        let mut disk = open_remote(path)?;
        let info = disk.info();
        raven_rt::println!(
            "blockd: remote disk={path} sectors={} ro={}",
            info.sectors,
            info.read_only
        );
        let sectors = info.sectors.min(GPT_PREFIX_SECTORS);
        if sectors < 34 {
            return Err(format!("disk has only {sectors} sectors"));
        }
        let mut prefix = vec![0u8; sectors as usize * 512];
        let mut done = 0u64;
        while done < sectors {
            let count = (sectors - done).min(u64::from(MAX_SECTORS));
            let start = usize::try_from(done)
                .map_err(|_| "gpt prefix offset".to_string())?
                .saturating_mul(512);
            let end = start.saturating_add(count as usize * 512);
            let target = prefix
                .get_mut(start..end)
                .ok_or_else(|| "gpt prefix range".to_string())?;
            disk.read_sectors(done, target)
                .map_err(|error| format!("gpt prefix: {}", error.description()))?;
            done += count;
        }
        let gpt = gpt::scan_prefix(&prefix, info.sectors)
            .map_err(|error| format!("gpt: {}", error_name(error)))?;
        let records = partition_records(&gpt)?;
        report(&gpt, &records);
        let handler = BlockHandler::new(disk, records)
            .map_err(|error| format!("handler: {}", error.description()))?;
        serve(handler)
    }

    /// Open the driver scheme, waiting for it to appear.
    ///
    /// The driver is launched asynchronously by `devd` and registers `disk:` after its own start-up
    /// work, so `blockd` polls for up to five minutes instead of failing the first attempt.
    fn open_remote(path: &str) -> Result<RemoteDisk, String> {
        let mut attempts = 0u32;
        loop {
            match RemoteDisk::open(path) {
                Ok(disk) => return Ok(disk),
                Err(error) => {
                    if attempts == 0 {
                        raven_rt::println!("blockd: waiting for {path}: {}", error.description());
                    }
                    attempts = attempts.saturating_add(1);
                    if attempts.is_multiple_of(50) {
                        raven_rt::println!(
                            "blockd: still waiting for {path} after {attempts}: {}",
                            error.description()
                        );
                    }
                    if attempts >= 3000 {
                        return Err(format!("open {path}: {}", error.description()));
                    }
                    let _ = raven_rt::nanosleep(100_000_000);
                }
            }
        }
    }

    fn partition_records(gpt: &Gpt) -> Result<Vec<PartitionRecord>, String> {
        if gpt.partitions.len() > MAX_PARTITIONS {
            return Err(format!(
                "gpt: {} partitions exceed the protocol limit {}",
                gpt.partitions.len(),
                MAX_PARTITIONS
            ));
        }
        Ok(gpt.partitions.iter().map(partition_record).collect())
    }

    fn report(gpt: &Gpt, records: &[PartitionRecord]) {
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
    }

    fn serve<D: Device>(mut handler: BlockHandler<D>) -> Result<(), String> {
        let mut server = match Server::register("block") {
            Ok(server) => server,
            Err(error) => return Err(error.description().to_string()),
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
