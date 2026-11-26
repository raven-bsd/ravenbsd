// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD UFS filesystem server (`ufs:` scheme).
//!
//! K54 serves a read-only mount; K55 adds create/write/rename and the
//! sync/fsync path.  The server selects one GPT partition from a `block:`
//! device and serves the mounted tree to `vfsd`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod trace;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;

    use fsd_ufs::UfsView;
    use fsd_ufs::block::open_partition;
    use raven_rt::entry;
    use raven_scheme::Server;
    use ufs_core::BlockIo;

    entry!(main);

    /// S11 `Opcode::Open` wire value.
    const OPEN_OPCODE: u16 = 1;
    /// S11 `Opcode::Read` wire value.
    const READ_OPCODE: u16 = 2;
    const DEFAULT_DEVICE: &str = "block:0";
    const DEFAULT_SCHEME: &str = "ufs";
    const DEFAULT_PARTITION: u32 = 1;
    const DEFAULT_READ_ONLY: bool = false;

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
        let read_only = match args.env_get("FSD_UFS_READ_ONLY") {
            Some(value) => match parse_bool(value) {
                Some(read_only) => read_only,
                None => {
                    raven_rt::eprintln!("fsd-ufs: invalid FSD_UFS_READ_ONLY={value}");
                    return 1;
                }
            },
            None => DEFAULT_READ_ONLY,
        };
        let mut disk = match open_partition(device, partition) {
            Ok(disk) => disk,
            Err(message) => {
                raven_rt::eprintln!("fsd-ufs: {message}");
                return 1;
            }
        };
        let size = disk.size_bytes();
        let recovery = match ufs_core::check::recover(&mut disk, read_only) {
            Ok(recovery) => recovery,
            Err(error) => {
                raven_rt::eprintln!("fsd-ufs: mount {device}: {error}");
                return 1;
            }
        };
        if let ufs_core::check::Recovery::Refused { findings } = recovery {
            raven_rt::eprintln!(
                "fsd-ufs: mount {device}: refused to serve a dirty image ({findings} findings)"
            );
            return 1;
        }
        let view = match UfsView::mount(disk, read_only) {
            Ok(view) => view,
            Err(error) => {
                raven_rt::eprintln!("fsd-ufs: mount {device}: {error}");
                return 1;
            }
        };
        raven_rt::println!(
            "fsd-ufs: mounted {device} partition {partition} as {scheme}: ({size} bytes)"
        );
        match recovery {
            ufs_core::check::Recovery::Repaired { fixed } => {
                raven_rt::println!("fsd-ufs: repaired {fixed} findings on {device}");
            }
            ufs_core::check::Recovery::Replayed { repaired } => {
                raven_rt::println!(
                    "fsd-ufs: journal replayed on {device}; repaired {repaired} findings"
                );
            }
            ufs_core::check::Recovery::Clean | ufs_core::check::Recovery::Refused { .. } => {}
        }
        raven_rt::println!(
            "fsd-ufs: mode={} journal={}",
            if read_only { "ro" } else { "rw" },
            if view.is_journaled() {
                "active"
            } else {
                "none"
            }
        );
        let mut handler = fsd_ufs::FsHandler::with_clock(view, wall_clock);
        let mut server = match Server::register(scheme) {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("fsd-ufs: {}", error.description());
                return 1;
            }
        };
        log_started("fsd-ufs");
        let mut probe = crate::trace::Probe::server();
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            let opcode = packet.packet.opcode;
            if opcode == OPEN_OPCODE || opcode == READ_OPCODE {
                probe.record(packet.packet.trace_id, opcode);
            }
            if opcode == OPEN_OPCODE {
                handler.expect_open(packet.packet.file_id, packet.packet.path_bytes());
            }
            let _ = server.dispatch(packet, &mut handler);
        }
    }

    fn wall_clock() -> i64 {
        raven_rt::sys::clock_get(0).map_or(0, |time| time.tv_sec)
    }

    fn parse_bool(value: &str) -> Option<bool> {
        match value {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        }
    }

    fn log_started(name: &str) {
        let client = raven_scheme::Client::new();
        let path = format!("log:{name}");
        if let Ok(handle) = client.open(&path, 2) {
            let _ = client.write(handle, 0, b"started\n");
            let _ = client.close(handle);
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
