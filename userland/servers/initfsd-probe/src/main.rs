// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot `initfs:` client used by the `initfsd` qtest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::{Call, CallRequest, CallResponse, DirEntryIter};
    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ: u32 = 1;
    const WRITE: u32 = 2;
    const PROBE: &[u8] = b"initfsd probe data\n";

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("initfsd: probe ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("initfsd: {}", message);
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        let probe = read_all(&client, "initfs:/etc/probe")?;
        if probe != PROBE {
            return Err(format!(
                "initfs:/etc/probe content mismatch: {} bytes",
                probe.len()
            ));
        }
        let bin = list(&client, "initfs:/bin")?;
        if !bin.iter().any(|name| name == "uname") {
            return Err("initfs:/bin does not contain uname".to_string());
        }
        match client.open("initfs:/etc/probe", WRITE) {
            Err(Errno::EROFS) => {}
            other => {
                return Err(format!("write open did not return EROFS: {other:?}"));
            }
        }
        let root = client
            .open("initfs:/", READ)
            .map_err(|error| format!("open root: {}", error.description()))?;
        let request = CallRequest::new(Call::Mkdir {
            mode: 0o755,
            path: b"/nope",
        });
        let mut encoded = [0u8; 128];
        let length = request
            .encode(&mut encoded)
            .map_err(|_| "encode MKDIR".to_string())?;
        let mut response = [0u8; 64];
        let length = client
            .call(root, &encoded[..length], &mut response)
            .map_err(|error| format!("MKDIR call: {}", error.description()))?;
        let status = match CallResponse::decode(request.op(), &response[..length])
            .map_err(|_| "decode MKDIR response".to_string())?
        {
            CallResponse::Status(status) => status,
            _ => return Err("MKDIR returned an unexpected response".to_string()),
        };
        let _ = client.close(root);
        if status != -(Errno::EROFS.0 as i32) {
            return Err(format!("MKDIR status was {status}, expected -EROFS"));
        }
        Ok(())
    }

    fn open(client: &Client, path: &str, flags: u32) -> Result<raven_abi::types::Handle, String> {
        client
            .open(path, flags)
            .map_err(|error| format!("open {path}: {}", error.description()))
    }

    fn read_all(client: &Client, path: &str) -> Result<Vec<u8>, String> {
        let handle = open(client, path, READ)?;
        let mut data = Vec::new();
        let mut buffer = [0u8; 512];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("read {path}: {}", error.description()))?;
            if read == 0 {
                break;
            }
            data.extend_from_slice(buffer.get(..read).unwrap_or(&[]));
            if data.len() > 64 * 1024 {
                let _ = client.close(handle);
                return Err(format!("read {path}: more than 64 KiB"));
            }
        }
        let _ = client.close(handle);
        Ok(data)
    }

    fn list(client: &Client, path: &str) -> Result<Vec<String>, String> {
        let handle = open(client, path, READ)?;
        let mut names = Vec::new();
        let mut buffer = [0u8; 2048];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| format!("read {path}: {}", error.description()))?;
            if read == 0 {
                break;
            }
            for entry in DirEntryIter::new(buffer.get(..read).unwrap_or(&[])) {
                let entry = entry.map_err(|_| format!("bad directory entry in {path}"))?;
                names.push(
                    core::str::from_utf8(entry.name)
                        .map_err(|_| format!("bad name in {path}"))?
                        .to_string(),
                );
            }
        }
        let _ = client.close(handle);
        Ok(names)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
