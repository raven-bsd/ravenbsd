// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `echo` utility.
//!
//! The real `no_std` program is compiled for `target_os = "none"`; on the host a stub keeps
//! `cargo test`/clippy usable without a freestanding toolchain.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_echo::write_echo;
    use raven_rt::entry;

    entry!(main);

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn main(args: raven_rt::Args) -> i32 {
        let mut writer = Writer;
        if write_echo(args.iter().skip(1), &mut writer).is_err() {
            return 1;
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
