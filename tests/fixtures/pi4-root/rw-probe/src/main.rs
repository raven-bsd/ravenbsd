// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot read-write UFS root probe for the Pi 4 EMMC2 fake-card qtest.
//!
//! `svcd` mounts the UFS root at `/root` during boot. The probe writes a file, unmounts the root,
//! remounts it through vfsd and verifies the data and a known image file. This exercises the
//! write-back, umount and remount paths on the same fake card the read-only `pi4-root` case uses.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::String;

    use raven_abi::errno::Errno;
    use raven_proto_fs::{O_CREAT, O_RDWR, O_TRUNC};
    use raven_rt::entry;
    use raven_rt::fs;
    use raven_rt::println;

    entry!(main);

    const ROOT: &str = "/root";
    const DATA_PATH: &str = "file:/root/rw-persist.txt";
    const KNOWN_PATH: &str = "file:/root/ten.txt";
    const PAYLOAD: &[u8] = b"pi4-rw-persist\n";

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                println!("pi4-root-rw: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("pi4-root-rw: probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        mount_root()?;
        println!("pi4-root-rw: mount ok");

        let mut file = fs::open(DATA_PATH, O_RDWR | O_CREAT | O_TRUNC)
            .map_err(|error| format!("create: {}", error.description()))?;
        file.write(PAYLOAD)
            .map_err(|error| format!("write: {}", error.description()))?;
        file.close()
            .map_err(|error| format!("close: {}", error.description()))?;
        let data = fs::read_to_vec(DATA_PATH)
            .map_err(|error| format!("read back: {}", error.description()))?;
        if data.as_slice() != PAYLOAD {
            return Err(format!("read back returned {} bytes", data.len()));
        }
        println!("pi4-root-rw: write ok");

        fs::sync().map_err(|error| format!("sync: {}", error.description()))?;
        fs::umount(ROOT).map_err(|error| format!("umount: {}", error.description()))?;
        println!("pi4-root-rw: umount ok");

        mount_root()?;
        println!("pi4-root-rw: remount ok");

        let data = fs::read_to_vec(DATA_PATH)
            .map_err(|error| format!("read after remount: {}", error.description()))?;
        if data.as_slice() != PAYLOAD {
            return Err(format!("remount read returned {} bytes", data.len()));
        }
        println!("pi4-root-rw: read back ok");

        let known = fs::read_to_vec(KNOWN_PATH)
            .map_err(|error| format!("known file: {}", error.description()))?;
        if known.as_slice() != b"0123456789" {
            return Err(format!("ten.txt has {} bytes", known.len()));
        }
        println!("pi4-root-rw: known file ok");

        // Leave the root mounted for the remaining service graph; svcd owns the mount.
        Ok(())
    }

    fn mount_root() -> Result<(), String> {
        match fs::mount(ROOT, "ufs:") {
            Ok(()) | Err(Errno::EEXIST) => Ok(()),
            Err(error) => Err(format!("mount: {}", error.description())),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
