// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! VideoCore mailbox property-buffer codec for a linear firmware display buffer.
//!
//! A property buffer is `{size, code}` followed by tags and a zero terminator. Each tag is
//! `{tag, buffer_bytes, request_bytes}` plus `buffer_bytes` of value space. The firmware replies
//! with the request address, clears `code` to `RESPONSE_SUCCESS`, sets bit 31 of every tag's value
//! length and fills the value space. This module only owns those byte rules; the mailbox registers,
//! the physical address and the cache maintenance live in the kernel glue.

/// Largest property buffer this codec builds or accepts.
pub const MAX_PROPERTY_BYTES: usize = 128;

/// `SET_PHYSICAL_W_H` (request `0x00048003`).
pub const TAG_SET_PHYSICAL_WH: u32 = 0x0004_8003;
/// `SET_VIRTUAL_W_H` (request `0x00048004`).
pub const TAG_SET_VIRTUAL_WH: u32 = 0x0004_8004;
/// `SET_DEPTH` (request `0x00048005`).
pub const TAG_SET_DEPTH: u32 = 0x0004_8005;
/// `SET_PIXEL_ORDER` (request `0x00048006`).
pub const TAG_SET_PIXEL_ORDER: u32 = 0x0004_8006;
/// `GET_PITCH` (request `0x00040008`).
pub const TAG_GET_PITCH: u32 = 0x0004_0008;
/// `ALLOCATE_BUFFER` (request `0x00040001`); the response carries the base and size.
pub const TAG_ALLOCATE_BUFFER: u32 = 0x0004_0001;

/// `SET_PIXEL_ORDER` value: blue, green, red bytes.
pub const PIXEL_ORDER_BGR: u32 = 0;
/// `SET_PIXEL_ORDER` value: red, green, blue bytes.
pub const PIXEL_ORDER_RGB: u32 = 1;

/// Firmware response success code.
const RESPONSE_SUCCESS: u32 = 0x8000_0000;
/// Tag value-length response bit.
const RESPONSE_BIT: u32 = 0x8000_0000;
/// Size of the property-buffer header.
const HEADER_BYTES: usize = 8;
/// Size of one tag header.
const TAG_HEADER_BYTES: usize = 12;
/// Number of tags checked by [`decode_response`].
const TAG_COUNT: u8 = 6;

/// A display request sent in one property message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayRequest {
    /// Physical and virtual width in pixels.
    pub width: u32,
    /// Physical and virtual height in pixels.
    pub height: u32,
    /// Bits per pixel (8, 16 or 32).
    pub depth: u32,
    /// `PIXEL_ORDER_RGB` or `PIXEL_ORDER_BGR`.
    pub pixel_order: u32,
    /// Alignment requested for the allocated buffer.
    pub alignment: u32,
}

impl Default for DisplayRequest {
    fn default() -> Self {
        Self {
            width: 1024,
            height: 768,
            depth: 32,
            pixel_order: PIXEL_ORDER_RGB,
            alignment: 4096,
        }
    }
}

/// The firmware's answer to an [`DisplayRequest`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DisplayInfo {
    /// VideoCore bus address of the allocated buffer.
    pub address: u32,
    /// Allocated buffer length in bytes.
    pub size: u32,
    /// Bytes from one row start to the next.
    pub pitch: u32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bits per pixel.
    pub depth: u32,
    /// Pixel-order value echoed by the firmware.
    pub pixel_order: u32,
}

/// A mailbox codec or firmware failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MailboxError {
    /// The supplied slice is smaller than the message.
    BufferTooSmall,
    /// The request has a width, height or depth the console does not support.
    InvalidRequest,
    /// The firmware reported a non-success buffer code.
    FirmwareError,
    /// A tag is missing, duplicated, unknown or has an inconsistent length.
    MalformedResponse,
    /// A tag did not set its response bit.
    TagFailed(u32),
    /// Allocated buffer is too small for the returned pitch and height.
    AllocationTooSmall,
}

/// Encode one display request; returns the message length in bytes.
///
/// The caller must place the buffer at a 16-byte-aligned, physically contiguous address and send
/// that physical address on mailbox channel 8.
pub fn encode_request(buffer: &mut [u8], request: &DisplayRequest) -> Result<usize, MailboxError> {
    if request.width == 0
        || request.height == 0
        || !matches!(request.depth, 8 | 16 | 32)
        || !matches!(request.pixel_order, PIXEL_ORDER_BGR | PIXEL_ORDER_RGB)
    {
        return Err(MailboxError::InvalidRequest);
    }
    let mut writer = Writer::new(buffer);
    writer.u32(0)?; // size, patched below
    writer.u32(0)?; // request code
    writer.tag(TAG_SET_PHYSICAL_WH, 8, 8, &[request.width, request.height])?;
    writer.tag(TAG_SET_VIRTUAL_WH, 8, 8, &[request.width, request.height])?;
    writer.tag(TAG_SET_DEPTH, 4, 4, &[request.depth])?;
    writer.tag(TAG_SET_PIXEL_ORDER, 4, 4, &[request.pixel_order])?;
    writer.tag(TAG_ALLOCATE_BUFFER, 8, 4, &[request.alignment])?;
    writer.tag(TAG_GET_PITCH, 4, 0, &[])?;
    writer.u32(0)?; // end tag
    let size = u32::try_from(writer.cursor).map_err(|_| MailboxError::BufferTooSmall)?;
    writer.set_u32(0, size)?;
    Ok(writer.cursor)
}

/// Decode a firmware display response into [`DisplayInfo`].
pub fn decode_response(buffer: &[u8]) -> Result<DisplayInfo, MailboxError> {
    let size = usize::try_from(get_u32(buffer, 0)?).map_err(|_| MailboxError::MalformedResponse)?;
    if size < HEADER_BYTES + 4 || size > buffer.len() || size > MAX_PROPERTY_BYTES {
        return Err(MailboxError::MalformedResponse);
    }
    if get_u32(buffer, 4)? != RESPONSE_SUCCESS {
        return Err(MailboxError::FirmwareError);
    }

    let mut cursor = HEADER_BYTES;
    let mut seen = 0u8;
    let mut info = DisplayInfo::default();
    loop {
        let tag = get_u32(buffer, cursor)?;
        if tag == 0 {
            break;
        }
        let buffer_bytes = usize::try_from(get_u32(buffer, cursor + 4)?)
            .map_err(|_| MailboxError::MalformedResponse)?;
        let response = get_u32(buffer, cursor + 8)?;
        if response & RESPONSE_BIT == 0 {
            return Err(MailboxError::TagFailed(tag));
        }
        let value_bytes = usize::try_from(response & !RESPONSE_BIT)
            .map_err(|_| MailboxError::MalformedResponse)?;
        if value_bytes > buffer_bytes || !value_bytes.is_multiple_of(4) {
            return Err(MailboxError::MalformedResponse);
        }
        let values = cursor + TAG_HEADER_BYTES;
        let end = values
            .checked_add(buffer_bytes)
            .ok_or(MailboxError::MalformedResponse)?;
        if end > size {
            return Err(MailboxError::MalformedResponse);
        }
        let value = |index: usize| -> Result<u32, MailboxError> {
            if index
                .checked_mul(4)
                .is_none_or(|offset| offset + 4 > value_bytes)
            {
                return Err(MailboxError::MalformedResponse);
            }
            get_u32(buffer, values + index * 4)
        };
        let bit = match tag {
            TAG_SET_PHYSICAL_WH => {
                info.width = value(0)?;
                info.height = value(1)?;
                1 << 0
            }
            TAG_SET_VIRTUAL_WH => {
                let width = value(0)?;
                let height = value(1)?;
                if width != info.width || height != info.height {
                    return Err(MailboxError::MalformedResponse);
                }
                1 << 1
            }
            TAG_SET_DEPTH => {
                info.depth = value(0)?;
                1 << 2
            }
            TAG_SET_PIXEL_ORDER => {
                info.pixel_order = value(0)?;
                1 << 3
            }
            TAG_ALLOCATE_BUFFER => {
                info.address = value(0)?;
                info.size = value(1)?;
                1 << 4
            }
            TAG_GET_PITCH => {
                info.pitch = value(0)?;
                1 << 5
            }
            _ => return Err(MailboxError::MalformedResponse),
        };
        if seen & bit != 0 {
            return Err(MailboxError::MalformedResponse);
        }
        seen |= bit;
        cursor = end;
    }
    if cursor + 4 > size || seen != (1u8 << TAG_COUNT) - 1 {
        return Err(MailboxError::MalformedResponse);
    }
    if info.pixel_order != PIXEL_ORDER_RGB && info.pixel_order != PIXEL_ORDER_BGR {
        return Err(MailboxError::MalformedResponse);
    }
    if info.width == 0 || info.height == 0 || info.depth == 0 || info.pitch == 0 {
        return Err(MailboxError::MalformedResponse);
    }
    let required = u64::from(info.pitch)
        .checked_mul(u64::from(info.height))
        .ok_or(MailboxError::MalformedResponse)?;
    if required > u64::from(info.size) {
        return Err(MailboxError::AllocationTooSmall);
    }
    Ok(info)
}

struct Writer<'a> {
    buffer: &'a mut [u8],
    cursor: usize,
}

impl<'a> Writer<'a> {
    fn new(buffer: &'a mut [u8]) -> Self {
        Self { buffer, cursor: 0 }
    }

    fn u32(&mut self, value: u32) -> Result<(), MailboxError> {
        let end = self
            .cursor
            .checked_add(4)
            .ok_or(MailboxError::BufferTooSmall)?;
        let slot = self
            .buffer
            .get_mut(self.cursor..end)
            .ok_or(MailboxError::BufferTooSmall)?;
        slot.copy_from_slice(&value.to_le_bytes());
        self.cursor = end;
        Ok(())
    }

    fn set_u32(&mut self, offset: usize, value: u32) -> Result<(), MailboxError> {
        let end = offset.checked_add(4).ok_or(MailboxError::BufferTooSmall)?;
        let slot = self
            .buffer
            .get_mut(offset..end)
            .ok_or(MailboxError::BufferTooSmall)?;
        slot.copy_from_slice(&value.to_le_bytes());
        Ok(())
    }

    fn tag(
        &mut self,
        tag: u32,
        buffer_bytes: u32,
        request_bytes: u32,
        values: &[u32],
    ) -> Result<(), MailboxError> {
        self.u32(tag)?;
        self.u32(buffer_bytes)?;
        self.u32(request_bytes)?;
        for value in values {
            self.u32(*value)?;
        }
        let written = u32::try_from(values.len())
            .ok()
            .and_then(|count| count.checked_mul(4))
            .ok_or(MailboxError::BufferTooSmall)?;
        if written > buffer_bytes {
            return Err(MailboxError::InvalidRequest);
        }
        let mut padding = buffer_bytes - written;
        while padding > 0 {
            self.u32(0)?;
            padding = padding.saturating_sub(4);
        }
        Ok(())
    }
}

fn get_u32(bytes: &[u8], offset: usize) -> Result<u32, MailboxError> {
    let end = offset
        .checked_add(4)
        .ok_or(MailboxError::MalformedResponse)?;
    let slice = bytes
        .get(offset..end)
        .ok_or(MailboxError::MalformedResponse)?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

#[cfg(test)]
#[path = "mailbox_ut.rs"]
mod tests;
