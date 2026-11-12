// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `hostname` utility.
//!
//! Reads or writes `sys:/hostname` through `raven_rt::fs`; `--selftest` reads the current value,
//! sets a temporary one, verifies it, restores the original and prints
//! `hostname: selftest ok`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_hostname::{Action, parse_args, trim_newline, valid_name};
    use raven_proto_fs::{O_RDONLY, O_RDWR};
    use raven_rt::entry;

    entry!(main);

    const TOOL: &str = "hostname";

    fn read_hostname() -> Result<alloc::vec::Vec<u8>, raven_abi::errno::Errno> {
        let mut file = raven_rt::fs::open("sys:/hostname", O_RDONLY)?;
        file.read_to_vec()
    }

    fn write_hostname(value: &str) -> Result<(), raven_abi::errno::Errno> {
        if !valid_name(value) {
            return Err(raven_abi::errno::Errno::EINVAL);
        }
        let mut file = raven_rt::fs::open("sys:/hostname", O_RDWR)?;
        file.write(value.as_bytes())?;
        file.close()
    }

    fn print_current() -> i32 {
        match read_hostname() {
            Ok(data) => {
                let text = core::str::from_utf8(trim_newline(&data)).unwrap_or("");
                raven_rt::println!("{text}");
                0
            }
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                1
            }
        }
    }

    fn set(value: &str) -> i32 {
        match write_hostname(value) {
            Ok(()) => 0,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {value}: {}", error.description());
                1
            }
        }
    }

    fn selftest() -> i32 {
        let original = match read_hostname() {
            Ok(data) => trim_newline(&data).to_vec(),
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                return 1;
            }
        };
        let result = (|| {
            write_hostname("selftest-host")?;
            let check = read_hostname()?;
            if trim_newline(&check) != b"selftest-host" {
                return Err(raven_abi::errno::Errno::EIO);
            }
            write_hostname(core::str::from_utf8(&original).unwrap_or("raven"))?;
            Ok(())
        })();
        match result {
            Ok(()) => {
                raven_rt::println!("{TOOL}: selftest ok");
                0
            }
            Err(error) => {
                let _ = write_hostname(core::str::from_utf8(&original).unwrap_or("raven"));
                raven_rt::eprintln!("{TOOL}: {}", error.description());
                1
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        match parse_args(args.iter().skip(1)) {
            Ok(Action::Print) => print_current(),
            Ok(Action::Set(value)) => set(value),
            Ok(Action::Selftest) => selftest(),
            Err(error) => {
                raven_rt::eprintln!("{error}");
                2
            }
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
