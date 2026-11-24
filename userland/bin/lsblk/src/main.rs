// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `lsblk` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use lsblk::{parse_args, render};
    use raven_abi::types::Handle;
    use raven_proto_block::{
        BlockInfo, INFO_LEN, Op, REPLY_HEADER_LEN, REQUEST_HEADER_LEN, Request,
        decode_partition_list, decode_reply,
    };
    use raven_rt::entry;
    use raven_scheme::Client;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, usage};

    entry!(main);

    const OPEN_READ: u32 = 1;
    const RESPONSE: usize = REPLY_HEADER_LEN + 4096;

    fn main(args: raven_rt::Args) -> i32 {
        let (no_header, devices) = match parse_args(args.iter().skip(1)) {
            Ok(parsed) => parsed,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("lsblk", "[-n] [device ...]"));
                return EXIT_USAGE;
            }
        };
        let explicit = !devices.is_empty();
        let names: Vec<&str> = if explicit {
            devices
        } else {
            alloc::vec![
                "block:0", "block:1", "block:2", "block:3", "block:4", "block:5", "block:6",
                "block:7"
            ]
        };
        let mut printed = 0usize;
        for device in names {
            match query(device) {
                Ok((info, parts)) => {
                    let name = device.trim_start_matches("block:").replace(':', "0");
                    let name = if name.is_empty() {
                        "block0".to_string()
                    } else {
                        format!("block{name}")
                    };
                    raven_rt::print!("{}", render(&name, &info, &parts, !no_header));
                    printed += 1;
                }
                Err(error) => {
                    if explicit {
                        raven_rt::eprintln!("lsblk: {device}: {error}");
                        return EXIT_ERROR;
                    }
                }
            }
        }
        if printed == 0 {
            raven_rt::eprintln!("lsblk: no block devices");
            return EXIT_ERROR;
        }
        EXIT_OK
    }

    fn query(device: &str) -> Result<(BlockInfo, Vec<raven_proto_block::PartitionRecord>), String> {
        let client = Client::new();
        let handle = client
            .open(device, OPEN_READ)
            .map_err(|error| error.description().to_string())?;
        let info = call(&client, handle, Op::Info, INFO_LEN)?;
        let info = BlockInfo::decode(&info).map_err(|_| "block info decode".to_string())?;
        let list = call(&client, handle, Op::List, 4096)?;
        let parts =
            decode_partition_list(&list).map_err(|_| "partition list decode".to_string())?;
        let _ = client.close(handle);
        Ok((info, parts))
    }

    fn call(client: &Client, handle: Handle, op: Op, limit: usize) -> Result<Vec<u8>, String> {
        let request = Request {
            op,
            sector: 0,
            count: 0,
            data: &[],
        };
        let mut encoded = [0u8; REQUEST_HEADER_LEN];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "request encode".to_string())?;
        let mut response = alloc::vec![0u8; RESPONSE.max(limit + 8)];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        let (status, payload) = decode_reply(response.get(..read).ok_or("short reply")?)
            .map_err(|_| "reply decode".to_string())?;
        if status != 0 {
            return Err(format!("status {}", status));
        }
        Ok(payload.to_vec())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
