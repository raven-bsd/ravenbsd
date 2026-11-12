// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `ps` utility: list `svc:` services as `NAME STATE RESTARTS`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use core::fmt;

    use raven_proto_fs::O_RDONLY;
    use raven_ps::{Action, format_status, parse_args};
    use raven_rt::entry;

    entry!(main);

    const TOOL: &str = "ps";

    struct Console;

    impl fmt::Write for Console {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn status(name: &str) -> Result<alloc::string::String, raven_abi::errno::Errno> {
        let path = format!("svc:/{name}");
        let mut file = raven_rt::fs::open(&path, O_RDONLY)?;
        let data = file.read_to_vec()?;
        let _ = file.close();
        let text = core::str::from_utf8(&data).map_err(|_| raven_abi::errno::Errno::EPROTO)?;
        let mut formatted = alloc::string::String::new();
        format_status(text, &mut formatted).map_err(|_| raven_abi::errno::Errno::EPROTO)?;
        Ok(formatted)
    }

    fn list() -> i32 {
        let entries = match raven_rt::fs::read_dir("svc:") {
            Ok(entries) => entries,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: svc: {}", error.description());
                return 1;
            }
        };
        let mut console = Console;
        for entry in entries {
            match status(&entry.name) {
                Ok(line) => {
                    if fmt::Write::write_str(&mut console, &line).is_err() {
                        return 1;
                    }
                }
                Err(error) => {
                    raven_rt::eprintln!("{TOOL}: {}: {}", entry.name, error.description());
                    return 1;
                }
            }
        }
        0
    }

    fn selftest() -> i32 {
        let entries = match raven_rt::fs::read_dir("svc:") {
            Ok(entries) => entries,
            Err(error) => {
                raven_rt::eprintln!("{TOOL}: svc: {}", error.description());
                return 1;
            }
        };
        if entries.is_empty() {
            raven_rt::eprintln!("{TOOL}: no services");
            return 1;
        }
        for entry in entries {
            if status(&entry.name).is_err() {
                raven_rt::eprintln!("{TOOL}: {}: bad status", entry.name);
                return 1;
            }
        }
        raven_rt::println!("{TOOL}: selftest ok");
        0
    }

    fn main(args: raven_rt::Args) -> i32 {
        match parse_args(args.iter().skip(1)) {
            Ok(Action::List) => list(),
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
