// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `ramfs:` client used by the `ramfsd` qtest.
//!
//! It creates a directory and a 1 MiB file, reads the file back, renames it, removes the tree,
//! exercises a symlink, fills the filesystem to `ENOSPC`, and prints `ramfsd: probe ok`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::{Call, CallRequest, CallResponse};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ: u32 = 1;
    const READ_WRITE: u32 = 3;
    const CREATE: u32 = 4;
    const CHUNK: usize = 64 * 1024;
    const MIB: usize = 1024 * 1024;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("ramfsd: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("ramfsd: {}", message);
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let root = client
            .open("ramfs:", READ)
            .map_err(|error| format!("open root: {}", error.description()))?;

        call(
            &client,
            root,
            Call::Mkdir {
                mode: 0o755,
                path: b"/d",
            },
        )?;
        write_file(&client, "ramfs:/d/f", MIB)?;
        read_file(&client, "ramfs:/d/f", MIB)?;
        call(
            &client,
            root,
            Call::Rename {
                from: b"/d/f",
                to: b"/d/g",
            },
        )?;
        call(&client, root, Call::Unlink { path: b"/d/g" })?;
        call(&client, root, Call::Rmdir { path: b"/d" })?;

        call(
            &client,
            root,
            Call::Symlink {
                path: b"/link",
                target: b"d",
            },
        )?;
        let target = readlink(&client, root, b"/link")?;
        if target != b"d" {
            return Err("readlink returned the wrong target".to_string());
        }
        call(&client, root, Call::Unlink { path: b"/link" })?;

        fill_to_limit(&client, root)?;
        let _ = client.close(root);
        Ok(())
    }

    fn open(client: &Client, path: &str, flags: u32) -> Result<raven_abi::types::Handle, String> {
        client
            .open(path, flags)
            .map_err(|error| format!("open {path}: {}", error.description()))
    }

    fn write_file(client: &Client, path: &str, length: usize) -> Result<(), String> {
        let handle = open(client, path, READ_WRITE | CREATE)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut written = 0usize;
        while written < length {
            let take = (length - written).min(CHUNK);
            for (index, byte) in buffer.iter_mut().take(take).enumerate() {
                *byte = (written + index) as u8;
            }
            let count = client
                .write(handle, 0, &buffer[..take])
                .map_err(|error| format!("write {path}: {}", error.description()))?;
            if count != take {
                return Err(format!("write {path}: short write {count}/{take}"));
            }
            written += count;
        }
        let _ = client.close(handle);
        Ok(())
    }

    fn read_file(client: &Client, path: &str, length: usize) -> Result<(), String> {
        let handle = open(client, path, READ)?;
        let mut buffer = vec![0u8; CHUNK];
        let mut read_total = 0usize;
        while read_total < length {
            let want = (length - read_total).min(CHUNK);
            let count = client
                .read(handle, 0, &mut buffer[..want])
                .map_err(|error| format!("read {path}: {}", error.description()))?;
            if count == 0 {
                return Err(format!("read {path}: early EOF at {read_total}/{length}"));
            }
            for (index, byte) in buffer[..count].iter().enumerate() {
                let expected = (read_total + index) as u8;
                if *byte != expected {
                    return Err(format!("read {path}: mismatch at {}", read_total + index));
                }
            }
            read_total += count;
        }
        let mut extra = [0u8; 1];
        let count = client
            .read(handle, 0, &mut extra)
            .map_err(|error| format!("read {path}: {}", error.description()))?;
        let _ = client.close(handle);
        if count != 0 {
            return Err(format!("read {path}: extra byte after {length}"));
        }
        Ok(())
    }

    fn fill_to_limit(client: &Client, root: raven_abi::types::Handle) -> Result<(), String> {
        let handle = open(client, "ramfs:/big", READ_WRITE | CREATE)?;
        let buffer = vec![0x5a; CHUNK];
        let mut written = 0usize;
        let mut saw_full = false;
        while written < 20 * MIB {
            match client.write(handle, 0, &buffer) {
                Ok(count) if count == CHUNK => written += count,
                Ok(count) => {
                    let _ = client.close(handle);
                    return Err(format!("fill: short write {count}"));
                }
                Err(error) if error == Errno::ENOSPC => {
                    saw_full = true;
                    break;
                }
                Err(error) => {
                    let _ = client.close(handle);
                    return Err(format!("fill: {}", error.description()));
                }
            }
        }
        let _ = client.close(handle);
        call(client, root, Call::Unlink { path: b"/big" })?;
        if saw_full {
            Ok(())
        } else {
            Err("fill: ENOSPC was not reached".to_string())
        }
    }

    fn call(
        client: &Client,
        handle: raven_abi::types::Handle,
        operation: Call<'_>,
    ) -> Result<(), String> {
        let request = CallRequest::new(operation);
        let mut encoded = [0u8; 640];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "encode CALL request".to_string())?;
        let mut response = [0u8; 128];
        let length = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| format!("CALL: {}", error.description()))?;
        match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "decode CALL response".to_string())?
        {
            CallResponse::Status(status) => status_result(status),
            CallResponse::Readlink { status, .. } | CallResponse::StatFs { status, .. } => {
                status_result(status)
            }
        }
    }

    fn readlink(
        client: &Client,
        handle: raven_abi::types::Handle,
        path: &[u8],
    ) -> Result<Vec<u8>, String> {
        let request = CallRequest::new(Call::Readlink { path });
        let mut encoded = [0u8; 640];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "encode READLINK".to_string())?;
        let mut response = [0u8; 640];
        let length = client
            .call(handle, &encoded[..length], &mut response)
            .map_err(|error| format!("READLINK: {}", error.description()))?;
        match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "decode READLINK".to_string())?
        {
            CallResponse::Readlink { status: 0, target } => Ok(target.to_vec()),
            CallResponse::Status(status) | CallResponse::Readlink { status, .. } => {
                status_result(status).map(|()| Vec::new())
            }
            CallResponse::StatFs { status, .. } => status_result(status).map(|()| Vec::new()),
        }
    }

    fn status_result(status: i32) -> Result<(), String> {
        if status == 0 {
            return Ok(());
        }
        match Errno::from_ret(status as isize) {
            Err(error) => Err(error.description().to_string()),
            Ok(_) => Err("invalid CALL status".to_string()),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
