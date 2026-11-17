// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Minimal shell used by the `newgrp` target case.
//!
//! It prints the credentials the kernel installed before it was started, so the qtest can prove
//! `newgrp` switched the effective group before exec. It ignores its arguments.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::println;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match raven_rt::proc::credentials() {
            Ok(credentials) => println!(
                "required-utils: shell uid={} gid={}",
                credentials.euid, credentials.egid
            ),
            Err(error) => println!("required-utils: shell error {}", error.description()),
        }
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
