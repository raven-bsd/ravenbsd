// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD PID 1 entry point.
//!
//! On a real target this starts `svcd`: it maps the initfs, reads `/etc/services.d/*.toml`, starts
//! services in dependency order, serves `svc:` and supervises restarts. Profiles without service
//! manifests keep the M1 `uname` banner. Host builds only compile the library tests.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_init::run()
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
