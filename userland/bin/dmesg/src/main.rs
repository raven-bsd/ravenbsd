// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `dmesg` utility: read `log:` from the oldest retained record and write it to the
//! debug console.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_dmesg::{Action, parse_args};
    use raven_proto_fs::O_RDONLY;
    use raven_rt::entry;

    entry!(main);

    const TOOL: &str = "dmesg";

    fn read_log() -> Result<alloc::vec::Vec<u8>, raven_abi::errno::Errno> {
        let mut file = raven_rt::fs::open("log:", O_RDONLY)?;
        file.read_to_vec()
    }

    fn main(args: raven_rt::Args) -> i32 {
        let action = match parse_args(args.iter().skip(1)) {
            Ok(action) => action,
            Err(error) => {
                raven_rt::eprintln!("{error}");
                return 2;
            }
        };
        let data = match read_log() {
            Ok(data) => data,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                return 1;
            }
        };
        match action {
            Action::Print => {
                let _ = raven_rt::sys::debug_write(&data);
                0
            }
            Action::Selftest => {
                // An empty ring is a valid early-boot state; the check is that the real log:
                // server opened and returned a well-formed read (which it did above).
                raven_rt::println!("{TOOL}: selftest ok");
                0
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
