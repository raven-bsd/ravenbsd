// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The K27a boot-health marker.
//!
//! `init` writes one sector at [`MARKER_LBA`] on the boot disk after the service set is up. The
//! loader reads that sector on the next boot and confirms the trial slot only when the marker
//! carries the trial token from `boot.toml`.

use crate::config::Slot;

/// Marker magic.
pub const MAGIC: [u8; 8] = *b"RAVENHLT";
/// Marker format version.
pub const VERSION: u16 = 1;
/// Absolute LBA reserved for the marker (the GPT entries end at LBA 33).
pub const MARKER_LBA: u64 = 34;
/// Marker sector size.
pub const MARKER_BYTES: usize = 512;
/// Marker state: the service set is up.
pub const STATE_HEALTHY: u8 = 1;
/// Marker state: a trial boot started but did not confirm.
pub const STATE_TRIAL: u8 = 2;

/// A decoded health marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Marker {
    /// Slot that wrote the marker.
    pub slot: Slot,
    /// Trial token from `boot.toml`.
    pub token: u32,
    /// One of [`STATE_HEALTHY`] or [`STATE_TRIAL`].
    pub state: u8,
}

impl Marker {
    /// True when this marker confirms `slot` and `token`.
    pub fn confirms(self, slot: Slot, token: u32) -> bool {
        self.slot == slot && self.token == token && self.state == STATE_HEALTHY
    }
}

/// Encode a marker sector.
pub fn encode(slot: Slot, token: u32, state: u8) -> [u8; MARKER_BYTES] {
    let mut sector = [0u8; MARKER_BYTES];
    sector[..MAGIC.len()].copy_from_slice(&MAGIC);
    sector[8..10].copy_from_slice(&VERSION.to_le_bytes());
    sector[10] = match slot {
        Slot::A => 0,
        Slot::B => 1,
    };
    sector[11] = state;
    sector[12..16].copy_from_slice(&token.to_le_bytes());
    let checksum = checksum(&sector[..20]);
    sector[20..24].copy_from_slice(&checksum.to_le_bytes());
    sector
}

/// Decode a marker sector, rejecting stale or malformed data.
pub fn decode(bytes: &[u8]) -> Option<Marker> {
    if bytes.len() < 24 || bytes.get(..MAGIC.len())? != MAGIC {
        return None;
    }
    let version = u16::from_le_bytes(bytes.get(8..10)?.try_into().ok()?);
    if version != VERSION {
        return None;
    }
    let slot = match bytes[10] {
        0 => Slot::A,
        1 => Slot::B,
        _ => return None,
    };
    let state = *bytes.get(11)?;
    if state != STATE_HEALTHY && state != STATE_TRIAL {
        return None;
    }
    let token = u32::from_le_bytes(bytes.get(12..16)?.try_into().ok()?);
    let stored = u32::from_le_bytes(bytes.get(20..24)?.try_into().ok()?);
    if stored != checksum(&bytes[..20]) {
        return None;
    }
    Some(Marker { slot, token, state })
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut hash = 0x811c_9dc5u32;
    for byte in bytes {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

/// Find a matching marker on any UEFI block device.
#[cfg(target_os = "uefi")]
pub fn find_marker() -> Option<Marker> {
    use uefi::boot::{
        OpenProtocolAttributes, OpenProtocolParams, find_handles, image_handle, open_protocol,
    };
    use uefi::proto::media::block::BlockIO;

    let handles = find_handles::<BlockIO>().ok()?;
    for handle in handles {
        let params = OpenProtocolParams {
            handle,
            agent: image_handle(),
            controller: None,
        };
        // SAFETY: the marker sector is read-only and the protocol is opened non-exclusively;
        // no other agent mutates the block device during loader start-up.
        let Ok(mut block) =
            (unsafe { open_protocol::<BlockIO>(params, OpenProtocolAttributes::GetProtocol) })
        else {
            continue;
        };
        let block_size = block.media().block_size();
        let media_id = block.media().media_id();
        if block_size != MARKER_BYTES as u32 {
            continue;
        }
        let mut sector = [0u8; MARKER_BYTES];
        if block
            .read_blocks(media_id, MARKER_LBA, &mut sector)
            .is_err()
        {
            continue;
        }
        if let Some(marker) = decode(&sector) {
            return Some(marker);
        }
    }
    None
}

#[cfg(test)]
#[path = "health_ut.rs"]
mod tests;
