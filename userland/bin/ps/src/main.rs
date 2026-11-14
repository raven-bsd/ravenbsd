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

    use raven_output::{ErrorEnvelope, JsonWriter};
    use raven_proto_fs::O_RDONLY;
    use raven_ps::{Action, format_json_status, format_status, parse_args};
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

    struct StderrConsole;

    impl fmt::Write for StderrConsole {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::eprint!("{text}");
            Ok(())
        }
    }

    fn read_status(name: &str) -> Result<alloc::string::String, raven_abi::errno::Errno> {
        let path = format!("svc:/{name}");
        let mut file = raven_rt::fs::open(&path, O_RDONLY)?;
        let data = file.read_to_vec()?;
        let _ = file.close();
        core::str::from_utf8(&data)
            .map(alloc::string::String::from)
            .map_err(|_| raven_abi::errno::Errno::EPROTO)
    }

    fn status(name: &str) -> Result<alloc::string::String, raven_abi::errno::Errno> {
        let text = read_status(name)?;
        let mut formatted = alloc::string::String::new();
        format_status(&text, &mut formatted).map_err(|_| raven_abi::errno::Errno::EPROTO)?;
        Ok(formatted)
    }

    fn json_error(code: &str, message: &str, name: Option<&str>) {
        let mut envelope = ErrorEnvelope::new(code, message);
        if let Some(name) = name {
            envelope = envelope.with_context("name", name);
        }
        let mut writer = JsonWriter::new(StderrConsole);
        let _ = writer.write_error(&envelope);
    }

    fn list(json: bool) -> i32 {
        let entries = match raven_rt::fs::read_dir("svc:") {
            Ok(entries) => entries,
            Err(error) => {
                if json {
                    json_error("EIO", error.description(), None);
                } else {
                    raven_rt::eprintln!("{TOOL}: svc: {}", error.description());
                }
                return 1;
            }
        };
        let mut console = Console;
        let mut failed = false;
        for entry in entries {
            let text = match read_status(&entry.name) {
                Ok(text) => text,
                Err(error) => {
                    if json {
                        json_error("EIO", error.description(), Some(&entry.name));
                    } else {
                        raven_rt::eprintln!("{TOOL}: {}: {}", entry.name, error.description());
                    }
                    failed = true;
                    continue;
                }
            };
            let result = if json {
                format_json_status(&text, &mut console)
            } else {
                format_status(&text, &mut console)
            };
            if result.is_err() {
                if json {
                    json_error("EPROTO", "bad service status line", Some(&entry.name));
                } else {
                    raven_rt::eprintln!("{TOOL}: {}: bad service status", entry.name);
                }
                failed = true;
            }
        }
        i32::from(failed)
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
            Ok(Action::List) => list(false),
            Ok(Action::ListJson) => list(true),
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
