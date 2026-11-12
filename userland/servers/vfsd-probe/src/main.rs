// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `file:` client used by the `vfsd` qtest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::DirEntryIter;
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ: u32 = 1;
    const WRITE: u32 = 2;
    const CREATE: u32 = 4;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("vfsd: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("vfsd: {}", message);
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let root = directory(&client, "file:/")?;
        for expected in ["bin", "etc", "tmp"] {
            if !root.contains(&expected.to_string()) {
                return Err(alloc::format!("root listing missing {expected}"));
            }
        }
        let bin = directory(&client, "file:/bin")?;
        if !bin.contains(&"uname".to_string()) {
            return Err("bin listing missing uname".to_string());
        }

        write_file(&client, "file:/tmp/x", b"hello")?;
        if read_file(&client, "file:/tmp/x")? != b"hello" {
            return Err("tmp/x round trip failed".to_string());
        }

        let mounts = read_file(&client, "file:/etc/mounts")?;
        let mounts = String::from_utf8(mounts).map_err(|_| "mounts is not UTF-8".to_string())?;
        if !mounts.contains("/tmp ramfs:") {
            return Err("etc/mounts does not list /tmp".to_string());
        }

        raven_rt::fs::mkdir("/tmp/d", 0o755)
            .map_err(|error| alloc::format!("mkdir: {}", error.description()))?;
        raven_rt::fs::symlink("/tmp/d/l", "x")
            .map_err(|error| alloc::format!("symlink: {}", error.description()))?;
        let target = raven_rt::fs::readlink("/tmp/d/l")
            .map_err(|error| alloc::format!("readlink: {}", error.description()))?;
        if target != "x" {
            return Err("readlink returned the wrong target".to_string());
        }
        raven_rt::fs::unlink("/tmp/d/l")
            .map_err(|error| alloc::format!("unlink: {}", error.description()))?;
        raven_rt::fs::rmdir("/tmp/d")
            .map_err(|error| alloc::format!("rmdir: {}", error.description()))?;

        let null = client
            .open("file:/dev/null", WRITE)
            .map_err(|error| alloc::format!("open null: {}", error.description()))?;
        client
            .write(null, 0, b"discard")
            .map_err(|error| alloc::format!("write null: {}", error.description()))?;
        let _ = client.close(null);

        let zero = client
            .open("file:/dev/zero", READ)
            .map_err(|error| alloc::format!("open zero: {}", error.description()))?;
        let mut zeros = [0xaa; 16];
        let length = client
            .read(zero, 0, &mut zeros)
            .map_err(|error| alloc::format!("read zero: {}", error.description()))?;
        let _ = client.close(zero);
        if length != zeros.len() || zeros.iter().any(|byte| *byte != 0) {
            return Err("dev/zero is not zero".to_string());
        }

        match raven_rt::fs::rename("/tmp/x", "/etc/x") {
            Err(Errno::EXDEV) => {}
            _ => return Err("cross-mount rename did not return EXDEV".to_string()),
        }

        raven_rt::fs::mount("/tmp/m", "ramfs:")
            .map_err(|error| alloc::format!("mount: {}", error.description()))?;
        write_file(&client, "file:/tmp/m/y", b"mounted")?;
        if read_file(&client, "file:/tmp/m/y")? != b"mounted" {
            return Err("second mount round trip failed".to_string());
        }
        raven_rt::fs::umount("/tmp/m")
            .map_err(|error| alloc::format!("umount: {}", error.description()))?;
        match client.open("file:/tmp/m/y", READ) {
            Err(Errno::ENOENT) => {}
            _ => return Err("unmounted path still resolves".to_string()),
        }
        Ok(())
    }

    fn open(client: &Client, path: &str, flags: u32) -> Result<raven_abi::types::Handle, String> {
        client
            .open(path, flags)
            .map_err(|error| alloc::format!("open {path}: {}", error.description()))
    }

    fn directory(client: &Client, path: &str) -> Result<Vec<String>, String> {
        let handle = open(client, path, READ)?;
        let mut names = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let length = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| alloc::format!("read {path}: {}", error.description()))?;
            if length == 0 {
                break;
            }
            for entry in DirEntryIter::new(&buffer[..length]) {
                let entry = entry.map_err(|_| alloc::format!("bad entry in {path}"))?;
                names.push(
                    String::from_utf8(entry.name.to_vec())
                        .map_err(|_| alloc::format!("non-UTF-8 entry in {path}"))?,
                );
            }
        }
        let _ = client.close(handle);
        Ok(names)
    }

    fn read_file(client: &Client, path: &str) -> Result<Vec<u8>, String> {
        let handle = open(client, path, READ)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 1024];
        loop {
            let length = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| alloc::format!("read {path}: {}", error.description()))?;
            if length == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..length]);
        }
        let _ = client.close(handle);
        Ok(data)
    }

    fn write_file(client: &Client, path: &str, data: &[u8]) -> Result<(), String> {
        let handle = open(client, path, WRITE | CREATE)?;
        let written = client
            .write(handle, 0, data)
            .map_err(|error| alloc::format!("write {path}: {}", error.description()))?;
        let _ = client.close(handle);
        if written != data.len() {
            return Err(alloc::format!("short write to {path}"));
        }
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
