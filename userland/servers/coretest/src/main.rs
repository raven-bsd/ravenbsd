// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One-shot client used by the `core-servers` qtest.
//!
//! It exercises `null:`, `zero:`, `rand:`, `sys:` and `log:` through the K30 client and prints
//! exactly `coretest: ok` on success. Any failure prints a diagnostic and exits 1.

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

    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const READ: u32 = 1;
    const WRITE: u32 = 2;
    const READ_WRITE: u32 = 3;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => {
                raven_rt::println!("coretest: ok");
                0
            }
            Err(message) => {
                raven_rt::eprintln!("coretest: {}", message);
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();

        // null: read is EOF, writes are accepted and discarded.
        let mut buffer = [0u8; 16];
        let handle = open(&client, "null:", READ)?;
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("null read: {}", error.description()))?;
        let _ = client.close(handle);
        if read != 0 {
            return Err(format!("null read returned {read} bytes"));
        }
        write(&client, "null:", WRITE, b"discard")?;

        // zero: reads are filled with zeros.
        let handle = open(&client, "zero:", READ)?;
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("zero read: {}", error.description()))?;
        let _ = client.close(handle);
        if read != buffer.len() || buffer.iter().any(|byte| *byte != 0) {
            return Err("zero read did not return zeroes".to_string());
        }

        // rand: a read succeeds; the content is not asserted (it is pseudo-random).
        let handle = open(&client, "rand:", READ)?;
        let read = client
            .read(handle, 0, &mut buffer)
            .map_err(|error| format!("rand read: {}", error.description()))?;
        let _ = client.close(handle);
        if read != buffer.len() {
            return Err(format!("rand read returned {read} bytes"));
        }
        write(&client, "rand:", WRITE, b"stir")?;

        // sys: the uname file starts with the system name.
        let uname = read_all(&client, "sys:uname")?;
        if !contains(&uname, b"RavenBSD") {
            return Err("sys:uname does not contain RavenBSD".to_string());
        }
        let hostname = read_all(&client, "sys:hostname")?;
        if !contains(&hostname, b"raven") {
            return Err("sys:hostname default is not raven".to_string());
        }
        write(&client, "sys:hostname", READ_WRITE, b"coretest\n")?;
        let hostname = read_all(&client, "sys:hostname")?;
        if !contains(&hostname, b"coretest") {
            return Err("sys:hostname write was not visible".to_string());
        }
        write(&client, "sys:hostname", READ_WRITE, b"raven\n")?;
        let uptime = read_all(&client, "sys:uptime")?;
        if uptime.is_empty() {
            return Err("sys:uptime is empty".to_string());
        }

        // log: a record written by coretest is readable from the read-all handle.
        write(&client, "log:coretest", WRITE, b"hello\n")?;
        let log = read_all(&client, "log:")?;
        if !contains(&log, b"coretest: hello") {
            return Err("log: does not contain coretest: hello".to_string());
        }
        Ok(())
    }

    fn open(client: &Client, path: &str, flags: u32) -> Result<raven_abi::types::Handle, String> {
        client
            .open(path, flags)
            .map_err(|error| format!("open {path}: {}", error.description()))
    }

    fn write(client: &Client, path: &str, flags: u32, data: &[u8]) -> Result<(), String> {
        let handle = open(client, path, flags)?;
        let written = client
            .write(handle, 0, data)
            .map_err(|error| format!("write {path}: {}", error.description()))?;
        let _ = client.close(handle);
        if written != data.len() {
            return Err(format!("write {path} accepted {written} of {}", data.len()));
        }
        Ok(())
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
            let chunk = buffer
                .get(..read)
                .ok_or_else(|| format!("read {path}: short buffer"))?;
            data.extend_from_slice(chunk);
            if data.len() > 16 * 1024 {
                let _ = client.close(handle);
                return Err(format!("read {path}: more than 16 KiB"));
            }
        }
        let _ = client.close(handle);
        Ok(data)
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
