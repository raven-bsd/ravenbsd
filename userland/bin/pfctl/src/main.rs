// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `pfctl` command.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(not(target_os = "none"))]
fn main() {
    struct HostLoader;

    impl raven_pfctl::FileLoader for HostLoader {
        fn read(&self, path: &str) -> Result<String, String> {
            std::fs::read_to_string(path).map_err(|error| error.to_string())
        }
    }

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let outcome = raven_pfctl::run(&arguments, &HostLoader);
    use std::io::Write;
    let mut stdout = std::io::stdout().lock();
    let _ = stdout.write_all(outcome.stdout.as_bytes());
    let mut stderr = std::io::stderr().lock();
    let _ = stderr.write_all(outcome.stderr.as_bytes());
    std::process::exit(outcome.exit_code);
}

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    struct RuntimeLoader;

    impl raven_pfctl::FileLoader for RuntimeLoader {
        fn read(&self, path: &str) -> Result<String, String> {
            let full = with_file_scheme(&fs::resolve(path));
            let bytes = fs::read_to_vec(&full).map_err(|error| error.description().to_string())?;
            String::from_utf8(bytes).map_err(|_| String::from("file is not valid UTF-8"))
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let values: Vec<String> = args.iter().skip(1).map(String::from).collect();
        let outcome = raven_pfctl::run(&values, &RuntimeLoader);
        for line in outcome.stdout.lines() {
            raven_rt::println!("{line}");
        }
        for line in outcome.stderr.lines() {
            raven_rt::eprintln!("{line}");
        }
        outcome.exit_code
    }
}
