// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `newfs` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use newfs::{parse_args, summary};
    use raven_abi::types::clock;
    use raven_rt::entry;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};
    use ufs_core::mkfs::{MkfsOptions, format};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!(
                    "{}",
                    usage(
                        "newfs",
                        "[-O 1|2] [-b block] [-f frag] [-s size] [-L label] [block:0]"
                    )
                );
                return EXIT_USAGE;
            }
        };
        let disk = match fsd_ufs::block::open_partition(options.device, 0) {
            Ok(disk) => disk,
            Err(error) => {
                raven_rt::eprintln!("newfs: {error}");
                return EXIT_ERROR;
            }
        };
        let timestamp = raven_rt::clock_get(clock::REALTIME)
            .map(|time| time.tv_sec)
            .unwrap_or(0);
        let mkfs = MkfsOptions {
            size: options.size,
            version: options.version,
            block_size: options.block_size,
            fragment_size: options.fragment_size,
            bytes_per_inode: 4096,
            minfree: 5,
            label: options.label,
            timestamp,
        };
        // The block scheme writes are synchronous calls; no separate host flush is needed.
        if let Err(error) = format(disk, &mkfs) {
            raven_rt::eprintln!("newfs: format: {error}");
            return EXIT_ERROR;
        }
        raven_rt::println!("{}", summary(&options));
        EXIT_OK
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
