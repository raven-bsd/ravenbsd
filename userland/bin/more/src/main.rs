// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `more` utility.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};

    use more::{Pager, page, parse_args};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_tool_core::error::{EXIT_ERROR, EXIT_OK, EXIT_USAGE, line, usage};
    use raven_tool_core::path::with_file_scheme;

    entry!(main);

    struct ConsolePager;

    impl Pager for ConsolePager {
        fn is_tty(&self) -> bool {
            true
        }

        fn write(&mut self, text: &str) {
            let bytes = text.as_bytes();
            let mut offset = 0usize;
            while offset < bytes.len() {
                match raven_rt::debug_write(bytes.get(offset..).unwrap_or(&[])) {
                    Ok(0) | Err(_) => return,
                    Ok(count) => offset = offset.saturating_add(count),
                }
            }
        }

        fn read_command(&mut self) -> Option<char> {
            let mut buffer = [0u8; 1];
            match raven_rt::debug_read(&mut buffer, 1_000_000_000) {
                Ok(0) | Err(_) => None,
                Ok(_) => Some(char::from(buffer[0])),
            }
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1).map(ToString::to_string)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("{}", usage("more", "[-n lines] file ..."));
                return EXIT_USAGE;
            }
        };
        let mut text = String::new();
        for path in &options.files {
            match fs::read_to_vec(&with_file_scheme(&fs::resolve(path))) {
                Ok(data) => text.push_str(&String::from_utf8_lossy(&data)),
                Err(error) => {
                    raven_rt::eprintln!("{}", line("more", path, error));
                    return EXIT_ERROR;
                }
            }
        }
        let mut pager = ConsolePager;
        if page(&text, options.page, &mut pager) {
            EXIT_OK
        } else {
            EXIT_OK
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
