// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! VideoCore mailbox property-buffer codec and tag constants.
//!
//! A property buffer starts with `{u32 size, u32 code}`, then one or more tags
//! `{u32 tag, u32 len, u32 rcode, len bytes of values}`, and ends with a zero tag word. The ARM
//! side sends a 16-byte aligned DMA address through channel 8; the firmware replaces the values
//! and sets bit 31 of each tag's `rcode`. This module owns only the byte-level rules; the actual
//! mailbox registers and DMA window are in `platd`.

use raven_abi::errno::Errno;

/// Largest property buffer this implementation builds or accepts.
pub const MAX_PROPERTY_BYTES: usize = 256;
/// Most request/response value words one tag may carry.
pub const MAX_TAG_VALUES: usize = 16;

/// `GET_BOARD_MODEL`.
pub const TAG_GET_BOARD_MODEL: u32 = 0x0001_0001;
/// `GET_BOARD_REVISION`.
pub const TAG_GET_BOARD_REVISION: u32 = 0x0001_0002;
/// `GET_MACADDRESS`.
pub const TAG_GET_MACADDRESS: u32 = 0x0001_0003;
/// `GET_BOARDSERIAL`.
pub const TAG_GET_BOARDSERIAL: u32 = 0x0001_0004;
/// `GET_POWERSTATE`.
pub const TAG_GET_POWERSTATE: u32 = 0x0002_0001;
/// `GET_CLOCKRATE`.
pub const TAG_GET_CLOCKRATE: u32 = 0x0003_0002;
/// `GET_FIRMWAREREV`.
pub const TAG_GET_FIRMWAREREV: u32 = 0x0000_0001;

/// Firmware clock id for EMMC2.
pub const CLOCK_EMMC2: u32 = 12;
/// Firmware power-domain id for the SD card.
pub const POWER_SDCARD: u32 = 0;

/// Board identity returned by the mailbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardInfo {
    /// Board model (`GET_BOARD_MODEL`).
    pub model: u32,
    /// Board revision (`GET_BOARD_REVISION`).
    pub revision: u32,
    /// Board serial (`GET_BOARDSERIAL`).
    pub serial: u64,
    /// Ethernet MAC address (`GET_MACADDRESS`).
    pub mac: [u8; 6],
}

impl BoardInfo {
    /// A deterministic board identity for the fake backend.
    pub const fn fake() -> Self {
        Self {
            model: 0x0000_2711,
            revision: 0x00c0_3114,
            serial: 0x0000_0000_1234_5678,
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
        }
    }
}

/// A mailbox property channel.
pub trait Mailbox {
    /// Execute one tag query. `response` receives the firmware value words.
    fn query(&mut self, tag: u32, request: &[u32], response: &mut [u32]) -> Result<usize, Errno>;
}

/// A deterministic mailbox for tests and the QEMU platform path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FakeMailbox {
    /// Board identity returned for the board tags.
    pub board: BoardInfo,
    /// `(clock id, Hz)` pairs understood by `GET_CLOCKRATE`.
    pub clocks: [(u32, u32); 2],
    /// `(domain id, state)` pairs understood by `GET_POWERSTATE`.
    pub power: [(u32, u32); 2],
}

impl Default for FakeMailbox {
    fn default() -> Self {
        Self {
            board: BoardInfo::fake(),
            clocks: [(CLOCK_EMMC2, 50_000_000), (2, 48_000_000)],
            power: [(POWER_SDCARD, 1), (1, 1)],
        }
    }
}

impl FakeMailbox {
    /// A fake mailbox with the default board, clocks and power domains.
    pub const fn new() -> Self {
        Self {
            board: BoardInfo::fake(),
            clocks: [(CLOCK_EMMC2, 50_000_000), (2, 48_000_000)],
            power: [(POWER_SDCARD, 1), (1, 1)],
        }
    }
}

impl Mailbox for FakeMailbox {
    fn query(&mut self, tag: u32, request: &[u32], response: &mut [u32]) -> Result<usize, Errno> {
        match tag {
            TAG_GET_BOARD_MODEL => write_one(response, self.board.model),
            TAG_GET_BOARD_REVISION => write_one(response, self.board.revision),
            TAG_GET_BOARDSERIAL => {
                if response.len() < 2 {
                    return Err(Errno::E2BIG);
                }
                response[0] = self.board.serial as u32;
                response[1] = (self.board.serial >> 32) as u32;
                Ok(2)
            }
            TAG_GET_MACADDRESS => {
                if response.len() < 2 {
                    return Err(Errno::E2BIG);
                }
                response[0] = u32::from_le_bytes([
                    self.board.mac[0],
                    self.board.mac[1],
                    self.board.mac[2],
                    self.board.mac[3],
                ]);
                response[1] = u32::from_le_bytes([self.board.mac[4], self.board.mac[5], 0, 0]);
                Ok(2)
            }
            TAG_GET_CLOCKRATE => {
                let id = request.first().copied().ok_or(Errno::EINVAL)?;
                let rate = self
                    .clocks
                    .iter()
                    .find(|(clock, _)| *clock == id)
                    .map(|(_, rate)| *rate)
                    .ok_or(Errno::ENODEV)?;
                if response.len() < 2 {
                    return Err(Errno::E2BIG);
                }
                response[0] = id;
                response[1] = rate;
                Ok(2)
            }
            TAG_GET_POWERSTATE => {
                let id = request.first().copied().ok_or(Errno::EINVAL)?;
                let state = self
                    .power
                    .iter()
                    .find(|(domain, _)| *domain == id)
                    .map(|(_, state)| *state)
                    .ok_or(Errno::ENODEV)?;
                if response.len() < 2 {
                    return Err(Errno::E2BIG);
                }
                response[0] = id;
                response[1] = state;
                Ok(2)
            }
            _ => Err(Errno::ENODEV),
        }
    }
}

fn write_one(response: &mut [u32], value: u32) -> Result<usize, Errno> {
    let slot = response.first_mut().ok_or(Errno::E2BIG)?;
    *slot = value;
    Ok(1)
}

/// Encode one tag request into `buffer`; returns the total buffer size in bytes.
///
/// The caller must pass a 16-byte aligned buffer that the firmware can read. The codec itself
/// only checks the supplied slice length.
pub fn encode_request(buffer: &mut [u8], tag: u32, request: &[u32]) -> Result<usize, Errno> {
    if request.len() > MAX_TAG_VALUES {
        return Err(Errno::E2BIG);
    }
    // size + code, tag header (tag/len/rcode), values, terminating zero tag.
    let words = 2usize
        .checked_add(3)
        .and_then(|value| value.checked_add(request.len()))
        .and_then(|value| value.checked_add(1))
        .ok_or(Errno::E2BIG)?;
    let size = words.checked_mul(4).ok_or(Errno::E2BIG)?;
    if size > buffer.len() || size > MAX_PROPERTY_BYTES {
        return Err(Errno::E2BIG);
    }
    buffer.get_mut(..size).ok_or(Errno::E2BIG)?.fill(0);
    put_u32(buffer, 0, size as u32)?;
    put_u32(buffer, 4, 0)?;
    put_u32(buffer, 8, tag)?;
    put_u32(buffer, 12, (request.len() * 4) as u32)?;
    put_u32(buffer, 16, 0)?;
    for (index, value) in request.iter().enumerate() {
        put_u32(buffer, 20 + index * 4, *value)?;
    }
    Ok(size)
}

/// Decode the values of `tag` from a firmware property response.
pub fn decode_response(buffer: &[u8], tag: u32, out: &mut [u32]) -> Result<usize, Errno> {
    let size = get_u32(buffer, 0)? as usize;
    if size < 24 || size > buffer.len() || size > MAX_PROPERTY_BYTES {
        return Err(Errno::EPROTO);
    }
    if get_u32(buffer, 4)? != 0x8000_0000 {
        return Err(Errno::EPROTO);
    }
    if get_u32(buffer, 8)? != tag {
        return Err(Errno::EPROTO);
    }
    let length = get_u32(buffer, 12)? as usize;
    if length > out.len() * 4 || !length.is_multiple_of(4) {
        return Err(Errno::E2BIG);
    }
    let rcode = get_u32(buffer, 16)?;
    if rcode & 0x8000_0000 == 0 {
        return Err(Errno::EIO);
    }
    let values = length / 4;
    for (index, slot) in out.iter_mut().take(values).enumerate() {
        *slot = get_u32(buffer, 20 + index * 4)?;
    }
    // The terminating zero tag must be present inside the declared size.
    if 20 + length + 4 > size || get_u32(buffer, 20 + length)? != 0 {
        return Err(Errno::EPROTO);
    }
    Ok(values)
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, Errno> {
    let end = offset.checked_add(4).ok_or(Errno::EINVAL)?;
    let slice = bytes.get(offset..end).ok_or(Errno::EINVAL)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) -> Result<(), Errno> {
    let end = offset.checked_add(4).ok_or(Errno::EINVAL)?;
    let slice = bytes.get_mut(offset..end).ok_or(Errno::EINVAL)?;
    slice.copy_from_slice(&value.to_le_bytes());
    Ok(())
}

#[cfg(test)]
#[path = "mailbox_ut.rs"]
mod tests;
