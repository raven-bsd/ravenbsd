// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot rootfs client used by the K33 `hw-boot` qtest.
//!
//! It runs from `/root/sbin` and reads only through vfsd: the directory listing and the hostname
//! come from the UFS root mounted at `/root`, never from the initfs.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};

    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("k33: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let entries = raven_rt::fs::read_dir("file:/root/etc")
            .map_err(|error| alloc::format!("read_dir: {}", error.description()))?;
        let names: alloc::vec::Vec<String> = entries.into_iter().map(|entry| entry.name).collect();
        raven_rt::println!("k33: etc={}", names.join(","));

        let hostname = raven_rt::fs::read_to_vec("file:/root/etc/hostname")
            .map_err(|error| alloc::format!("hostname: {}", error.description()))?;
        let hostname =
            String::from_utf8(hostname).map_err(|_| "hostname is not UTF-8".to_string())?;
        raven_rt::println!("k33: hostname={}", hostname.trim());
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
