// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `gpart` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};

    use gpart::{parse_args, show};
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
        let device = match parse_args(args.iter().skip(1)) {
            Ok(device) => device.unwrap_or("block:0"),
            Err(_) => {
                raven_rt::eprintln!("{}", usage("gpart", "show [block:0]"));
                return EXIT_USAGE;
            }
        };
        match query(device) {
            Ok((info, parts)) => {
                let name = device.trim_start_matches("block:").replace(':', "0");
                let name = if name.is_empty() {
                    String::from("block0")
                } else {
                    alloc::format!("block{name}")
                };
                raven_rt::print!("{}", show(&name, &info, &parts));
                EXIT_OK
            }
            Err(error) => {
                raven_rt::eprintln!("gpart: {device}: {error}");
                EXIT_ERROR
            }
        }
    }

    fn query(
        device: &str,
    ) -> Result<
        (
            BlockInfo,
            alloc::vec::Vec<raven_proto_block::PartitionRecord>,
        ),
        String,
    > {
        let client = Client::new();
        let handle = client
            .open(device, OPEN_READ)
            .map_err(|error| error.description().to_string())?;
        let info = call(&client, handle, Op::Info)?;
        let info = BlockInfo::decode(info.get(..INFO_LEN).ok_or("short info")?)
            .map_err(|_| "block info decode".to_string())?;
        let list = call(&client, handle, Op::List)?;
        let parts =
            decode_partition_list(&list).map_err(|_| "partition list decode".to_string())?;
        let _ = client.close(handle);
        Ok((info, parts))
    }

    fn call(client: &Client, handle: Handle, op: Op) -> Result<alloc::vec::Vec<u8>, String> {
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
        let mut response = alloc::vec![0u8; RESPONSE];
        let read = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| error.description().to_string())?;
        let (status, payload) = decode_reply(response.get(..read).ok_or("short reply")?)
            .map_err(|_| "reply decode".to_string())?;
        if status != 0 {
            return Err(alloc::format!("status {status}"));
        }
        Ok(payload.to_vec())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
