// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Prints the credentials, supplementary groups and environment the target session installed.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_user_secure_fixture::{print_environment, print_groups, print_ids};

    entry!(main);

    fn main(args: raven_rt::Args) -> i32 {
        if print_ids("probe").is_err() || print_groups("probe").is_err() {
            raven_rt::println!("probe: fail");
            return 1;
        }
        print_environment("probe", &args);
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
