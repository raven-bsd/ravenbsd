// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K27a boot-health marker written by `init` after the service set is up.
//!
//! The update planner adds `/etc/boot-health` to the trial initfs. It names the slot, the trial
//! token and the reserved marker LBA. `init` writes one sector through the `block:` scheme; the
//! loader reads that sector on the next boot and confirms the trial only when the token matches.

#[cfg(target_os = "none")]
use alloc::format;
#[cfg(target_os = "none")]
use alloc::string::String;

#[cfg(target_os = "none")]
use raven_initfs::Initfs;

/// Path of the health record in the trial initfs.
pub const HEALTH_PATH: &str = "/etc/boot-health";
/// Marker magic shared with the loader.
pub const MAGIC: [u8; 8] = *b"RAVENHLT";
/// Marker format version.
pub const VERSION: u16 = 1;
/// Marker state: the service set is up.
pub const STATE_HEALTHY: u8 = 1;
/// Marker sector size.
pub const MARKER_BYTES: usize = 512;
/// `block:` open flags: read and write.
#[cfg(target_os = "none")]
const BLOCK_READ_WRITE: u32 = 3;
/// `block:` request header size.
const REQUEST_HEADER_LEN: usize = 24;
/// `block:` reply header size.
#[cfg(target_os = "none")]
const REPLY_HEADER_LEN: usize = 8;
/// `block:` write operation.
const BLOCK_OP_WRITE: u16 = 2;

/// The parsed `/etc/boot-health` record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Health {
    /// Slot name (`a` or `b`).
    pub slot: u8,
    /// Trial token.
    pub token: u32,
    /// Reserved marker LBA.
    pub lba: u64,
}

/// Parse the line-based health record.
pub fn parse(text: &str) -> Option<Health> {
    let mut slot = None;
    let mut token = None;
    let mut lba = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (key, value) = line.split_once('=')?;
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "slot" => {
                slot = Some(match value {
                    "a" => 0,
                    "b" => 1,
                    _ => return None,
                });
            }
            "token" => token = Some(u32::from_str_radix(value, 16).ok()?),
            "lba" => lba = Some(value.parse().ok()?),
            _ => return None,
        }
    }
    Some(Health {
        slot: slot?,
        token: token?,
        lba: lba?,
    })
}

/// Encode the marker sector.
pub fn encode_marker(slot: u8, token: u32, state: u8) -> [u8; MARKER_BYTES] {
    let mut sector = [0u8; MARKER_BYTES];
    sector[..MAGIC.len()].copy_from_slice(&MAGIC);
    sector[8..10].copy_from_slice(&VERSION.to_le_bytes());
    sector[10] = slot;
    sector[11] = state;
    sector[12..16].copy_from_slice(&token.to_le_bytes());
    let checksum = checksum(&sector[..20]);
    sector[20..24].copy_from_slice(&checksum.to_le_bytes());
    sector
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Encode a one-sector `block:` write request.
pub fn encode_write(
    lba: u64,
    sector: &[u8; MARKER_BYTES],
) -> [u8; REQUEST_HEADER_LEN + MARKER_BYTES] {
    let mut request = [0u8; REQUEST_HEADER_LEN + MARKER_BYTES];
    request[0..2].copy_from_slice(&1u16.to_le_bytes());
    request[2..4].copy_from_slice(&BLOCK_OP_WRITE.to_le_bytes());
    request[4..12].copy_from_slice(&lba.to_le_bytes());
    request[12..16].copy_from_slice(&1u32.to_le_bytes());
    request[20..24].copy_from_slice(&(MARKER_BYTES as u32).to_le_bytes());
    request[REQUEST_HEADER_LEN..].copy_from_slice(sector);
    request
}

/// Write the healthy marker when the trial initfs carries `/etc/boot-health`.
#[cfg(target_os = "none")]
pub fn mark(initfs: &Initfs<'_>) -> Result<(), String> {
    let Some(entry) = initfs.get(HEALTH_PATH) else {
        return Ok(());
    };
    let text =
        core::str::from_utf8(entry.data).map_err(|_| String::from("boot-health is not UTF-8"))?;
    let health = parse(text).ok_or_else(|| String::from("boot-health is malformed"))?;
    let sector = encode_marker(health.slot, health.token, STATE_HEALTHY);
    let request = encode_write(health.lba, &sector);
    let client = raven_scheme::Client::new();
    let handle = client
        .open("block:0", BLOCK_READ_WRITE)
        .map_err(|error| format!("open block:0: {}", error.description()))?;
    let mut response = [0u8; REPLY_HEADER_LEN];
    let result = client
        .call(handle, &request, &mut response)
        .map_err(|error| format!("write marker: {}", error.description()));
    let _ = client.close(handle);
    let length = result?;
    if length < REPLY_HEADER_LEN {
        return Err(String::from("short marker reply"));
    }
    let status = i32::from_le_bytes([response[0], response[1], response[2], response[3]]);
    if status != 0 {
        return Err(format!("marker status {status}"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "health_ut.rs"]
mod tests;
