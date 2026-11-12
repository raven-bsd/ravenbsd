// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared client helpers for the K67 conformance fixture.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_proto_fs::{Call, CallRequest, CallResponse, DirEntryIter, StatFs};
use raven_scheme::Client;

/// Read/write access wire flags.
pub const READ: u32 = 1;
/// Write access wire flag.
pub const WRITE: u32 = 2;
/// Create wire flag.
pub const CREATE: u32 = 4;

/// Open a path with raw wire flags.
pub fn open(client: &Client, path: &str, flags: u32) -> Result<Handle, String> {
    client
        .open(path, flags)
        .map_err(|error| alloc::format!("open {path}: {}", error.description()))
}

/// Read a whole file.
pub fn read_all(client: &Client, path: &str) -> Result<Vec<u8>, String> {
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

/// Create/truncate a file and write `data`.
pub fn write_all(client: &Client, path: &str, data: &[u8]) -> Result<(), String> {
    let handle = open(client, path, READ | WRITE | CREATE)?;
    let written = client
        .write(handle, 0, data)
        .map_err(|error| alloc::format!("write {path}: {}", error.description()))?;
    let _ = client.close(handle);
    if written != data.len() {
        return Err(alloc::format!("short write to {path}"));
    }
    Ok(())
}

/// Encode and forward one control call on `handle`.
pub fn call(client: &Client, handle: Handle, request: Call<'_>) -> Result<Vec<u8>, String> {
    let request = CallRequest::new(request);
    let mut encoded = [0u8; 640];
    let length = request
        .encode(&mut encoded)
        .map_err(|_| "encode call".to_string())?;
    let mut response = [0u8; 640];
    let written = client
        .call(handle, &encoded[..length], &mut response)
        .map_err(|error| alloc::format!("call: {}", error.description()))?;
    Ok(response[..written].to_vec())
}

/// Run a status-only control call.
pub fn call_status(client: &Client, handle: Handle, call_value: Call<'_>) -> Result<(), String> {
    let op = call_value.op();
    let response = call(client, handle, call_value)?;
    match CallResponse::decode(op, &response).map_err(|_| "decode call".to_string())? {
        CallResponse::Status(0) => Ok(()),
        CallResponse::Status(status) => Err(alloc::format!("status {status}")),
        _ => Err("unexpected call response".to_string()),
    }
}

/// Read `StatFs` from a scheme root.
pub fn statfs(client: &Client, scheme: &str) -> Result<StatFs, String> {
    let handle = open(client, scheme, READ)?;
    let response = call(client, handle, Call::Statfs)?;
    let _ = client.close(handle);
    match CallResponse::decode(Call::Statfs.op(), &response)
        .map_err(|_| "decode statfs".to_string())?
    {
        CallResponse::StatFs { status: 0, statfs } => Ok(statfs),
        CallResponse::StatFs { status, .. } => Err(alloc::format!("statfs status {status}")),
        _ => Err("unexpected statfs response".to_string()),
    }
}

/// Decode the names of a directory.
pub fn directory(client: &Client, path: &str) -> Result<Vec<String>, String> {
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

/// Map an errno result to a short string.
pub fn errno_name(error: Errno) -> String {
    error.description().to_string()
}
