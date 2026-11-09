// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `uname` utility.
//!
//! The real `no_std` program is compiled for `target_os = "none"`; on the host the same manifest
//! builds a tiny stub so `cargo test`/clippy can check the library without a freestanding
//! toolchain.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_rt::entry;
    use raven_uname::{format, parse_args};

    entry!(main);

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                raven_rt::eprintln!("usage: uname [-amnoprsv]");
                return 2;
            }
        };
        let uname = match raven_rt::uname() {
            Ok(uname) => uname,
            Err(error) => {
                raven_rt::eprintln!("uname: {}", error.description());
                return 1;
            }
        };
        let mut writer = Writer;
        if format(&options, &uname, &mut writer).is_err() {
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
