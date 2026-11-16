// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! HID class and report descriptor parsing.

use crate::Error;

/// HID class descriptor type (`bDescriptorType`).
pub const HID_DESCRIPTOR_TYPE: u8 = 0x21;
/// HID report descriptor type as referenced by the class descriptor.
pub const REPORT_DESCRIPTOR_TYPE: u8 = 0x22;
/// Generic Desktop usage page.
pub const GENERIC_DESKTOP: u16 = 0x01;
/// Keyboard application usage.
pub const KEYBOARD_USAGE: u16 = 0x06;
/// Mouse application usage.
pub const MOUSE_USAGE: u16 = 0x02;
/// Maximum parsed report fields.
pub const MAX_FIELDS: usize = 16;
/// Global-item push/pop depth.
const GLOBAL_STACK: usize = 4;

/// The HID class descriptor (the interface companion descriptor).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HidDescriptor {
    /// `bcdHID`.
    pub version: u16,
    /// `bCountryCode`.
    pub country: u8,
    /// Length of the report descriptor referenced by this interface.
    pub report_length: u16,
}

/// One input or output field in a report descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Field {
    /// True for an Output item, false for Input.
    pub output: bool,
    /// Bit offset inside the report payload (no report-ID prefix counted).
    pub bit_offset: u16,
    /// Bits per report element.
    pub bit_size: u8,
    /// Number of elements.
    pub count: u16,
    /// Usage page in effect.
    pub usage_page: u16,
    /// First usage in the field (or the single usage).
    pub usage_min: u16,
    /// Last usage in the field.
    pub usage_max: u16,
    /// The raw `Input`/`Output` item flags.
    pub flags: u8,
}

/// Parsed report layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReportLayout {
    /// Application usage page.
    pub usage_page: u16,
    /// Application usage.
    pub usage: u16,
    /// Report identifier (0 = no report ID).
    pub report_id: u8,
    /// Total input payload bits.
    pub input_bits: u16,
    /// Total output payload bits.
    pub output_bits: u16,
    /// Parsed fields.
    pub fields: [Field; MAX_FIELDS],
    /// Number of valid fields.
    pub count: usize,
}

impl ReportLayout {
    /// True when this is the eight-byte boot keyboard layout.
    #[must_use]
    pub fn is_boot_keyboard(&self) -> bool {
        self.usage_page == GENERIC_DESKTOP
            && self.usage == KEYBOARD_USAGE
            && self.report_id == 0
            && self.input_bits == 64
    }

    /// The boot mouse report length: three bytes, or four with the wheel byte.
    ///
    /// Returns `None` when the layout is not a Generic Desktop mouse without a report ID, or the
    /// input payload is not the boot mouse's 24 or 32 bits.
    #[must_use]
    pub fn boot_mouse_report_len(&self) -> Option<usize> {
        if self.usage_page != GENERIC_DESKTOP || self.usage != MOUSE_USAGE || self.report_id != 0 {
            return None;
        }
        match self.input_bits {
            24 => Some(3),
            32 => Some(4),
            _ => None,
        }
    }

    /// True when this is a three- or four-byte boot mouse layout.
    #[must_use]
    pub fn is_boot_mouse(&self) -> bool {
        self.boot_mouse_report_len().is_some()
    }
}

/// Parse the nine-or-more-byte HID class descriptor.
///
/// # Errors
///
/// [`Error::TooShort`] for a truncated descriptor, [`Error::BadItem`] for the wrong descriptor
/// type or a missing report descriptor entry.
pub fn parse_hid_descriptor(data: &[u8]) -> Result<HidDescriptor, Error> {
    let length = usize::from(data.first().copied().ok_or(Error::TooShort)?);
    if length < 9 || data.len() < length {
        return Err(Error::TooShort);
    }
    if data.get(1).copied() != Some(HID_DESCRIPTOR_TYPE) {
        return Err(Error::BadItem);
    }
    let version = read_u16(data, 2)?;
    let country = data.get(4).copied().ok_or(Error::TooShort)?;
    let descriptors = usize::from(data.get(5).copied().ok_or(Error::TooShort)?);
    let mut offset = 6usize;
    let mut report_length = None;
    let mut index = 0usize;
    while index < descriptors {
        let end = offset.checked_add(3).ok_or(Error::TooShort)?;
        if end > length {
            return Err(Error::TooShort);
        }
        let kind = data.get(offset).copied().ok_or(Error::TooShort)?;
        let length = read_u16(data, offset.saturating_add(1))?;
        if kind == REPORT_DESCRIPTOR_TYPE {
            report_length = Some(length);
        }
        offset = end;
        index = index.saturating_add(1);
    }
    Ok(HidDescriptor {
        version,
        country,
        report_length: report_length.ok_or(Error::BadItem)?,
    })
}

/// Parse a HID report descriptor into its input/output field layout.
///
/// Local items reset after every main item. Logical minimum/maximum are accepted and ignored: the
/// boot keyboard model uses the fixed report layout, and the field table is descriptive.
///
/// # Errors
///
/// [`Error::TooShort`] for a truncated item, [`Error::BadItem`] for an unsupported item size or a
/// global-item stack overflow.
pub fn parse_report_descriptor(data: &[u8]) -> Result<ReportLayout, Error> {
    let mut layout = ReportLayout::default();
    let mut usage_page = 0u16;
    let mut usage = 0u16;
    let mut usage_min = 0u16;
    let mut usage_max = 0u16;
    let mut report_size = 0u16;
    let mut report_count = 0u16;
    let mut report_id = 0u8;
    let mut input_bits = 0u16;
    let mut output_bits = 0u16;
    let mut stack = [(0u16, 0u16, 0u8, 0u16); GLOBAL_STACK];
    let mut depth = 0usize;

    let mut index = 0usize;
    while index < data.len() {
        let prefix = data.get(index).copied().ok_or(Error::TooShort)?;
        index = index.saturating_add(1);
        if prefix == 0xfe {
            // Long item: data size and tag follow the prefix, then `size` payload bytes.
            let size = usize::from(data.get(index).copied().ok_or(Error::TooShort)?);
            let skip = size.checked_add(2).ok_or(Error::TooShort)?;
            index = index.checked_add(skip).ok_or(Error::TooShort)?;
            if index > data.len() {
                return Err(Error::TooShort);
            }
            continue;
        }
        let size = match prefix & 0x03 {
            0 => 0usize,
            1 => 1,
            2 => 2,
            3 => 4,
            _ => return Err(Error::BadItem),
        };
        let item_type = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        let value = read_bits(data, index, size)?;
        index = index.checked_add(size).ok_or(Error::TooShort)?;

        match item_type {
            // Global items.
            1 => match tag {
                0x0 => usage_page = value as u16,
                0x7 => report_size = value as u16,
                0x8 => report_id = value as u8,
                0x9 => report_count = value as u16,
                0xa => {
                    if let Some(slot) = stack.get_mut(depth) {
                        *slot = (usage_page, report_size, report_id, report_count);
                        depth = depth.saturating_add(1);
                    } else {
                        return Err(Error::BadItem);
                    }
                }
                0xb => {
                    if depth == 0 {
                        return Err(Error::BadItem);
                    }
                    depth = depth.saturating_sub(1);
                    if let Some(slot) = stack.get(depth).copied() {
                        (usage_page, report_size, report_id, report_count) = slot;
                    }
                }
                _ => {}
            },
            // Local items.
            2 => match tag {
                0x0 => {
                    if size == 4 {
                        usage_page = (value >> 16) as u16;
                        usage = (value & 0xffff) as u16;
                    } else {
                        usage = value as u16;
                    }
                }
                0x1 => usage_min = value as u16,
                0x2 => usage_max = value as u16,
                _ => {}
            },
            // Main items.
            0 => match tag {
                0x8 => {
                    let bits = report_size.saturating_mul(report_count);
                    if let Some(slot) = layout.fields.get_mut(layout.count) {
                        *slot = Field {
                            output: false,
                            bit_offset: input_bits,
                            bit_size: report_size.min(0xff) as u8,
                            count: report_count,
                            usage_page,
                            usage_min,
                            usage_max,
                            flags: value as u8,
                        };
                        layout.count = layout.count.saturating_add(1);
                    }
                    input_bits = input_bits.saturating_add(bits);
                    reset_local(&mut usage, &mut usage_min, &mut usage_max);
                }
                0x9 => {
                    let bits = report_size.saturating_mul(report_count);
                    if let Some(slot) = layout.fields.get_mut(layout.count) {
                        *slot = Field {
                            output: true,
                            bit_offset: output_bits,
                            bit_size: report_size.min(0xff) as u8,
                            count: report_count,
                            usage_page,
                            usage_min,
                            usage_max,
                            flags: value as u8,
                        };
                        layout.count = layout.count.saturating_add(1);
                    }
                    output_bits = output_bits.saturating_add(bits);
                    reset_local(&mut usage, &mut usage_min, &mut usage_max);
                }
                0xa => {
                    // Only the top-level Application collection names the device; an inner
                    // Physical/Logical collection must not overwrite the application usage.
                    if usage != 0 && value == 0x01 {
                        layout.usage_page = usage_page;
                        layout.usage = usage;
                    }
                    reset_local(&mut usage, &mut usage_min, &mut usage_max);
                }
                0xc => {}
                _ => {}
            },
            _ => {}
        }
    }

    layout.report_id = report_id;
    layout.input_bits = input_bits;
    layout.output_bits = output_bits;
    Ok(layout)
}

fn reset_local(usage: &mut u16, usage_min: &mut u16, usage_max: &mut u16) {
    *usage = 0;
    *usage_min = 0;
    *usage_max = 0;
}

fn read_bits(data: &[u8], offset: usize, size: usize) -> Result<u32, Error> {
    let end = offset.checked_add(size).ok_or(Error::TooShort)?;
    let bytes = data.get(offset..end).ok_or(Error::TooShort)?;
    let mut value = 0u32;
    for (index, byte) in bytes.iter().enumerate() {
        value |= u32::from(*byte) << (index as u32 * 8);
    }
    Ok(value)
}

fn read_u16(data: &[u8], offset: usize) -> Result<u16, Error> {
    let low = data.get(offset).copied().ok_or(Error::TooShort)?;
    let high = data
        .get(offset.saturating_add(1))
        .copied()
        .ok_or(Error::TooShort)?;
    Ok(u16::from_le_bytes([low, high]))
}

#[cfg(test)]
#[path = "descriptor_ut.rs"]
mod tests;

/// A standard boot-keyboard report descriptor, shared by the host tests.
#[cfg(test)]
pub(crate) const BOOT_DESCRIPTOR: [u8; 63] = [
    0x05, 0x01, 0x09, 0x06, 0xa1, 0x01, 0x05, 0x07, 0x19, 0xe0, 0x29, 0xe7, 0x15, 0x00, 0x25, 0x01,
    0x75, 0x01, 0x95, 0x08, 0x81, 0x02, 0x95, 0x01, 0x75, 0x08, 0x81, 0x01, 0x95, 0x06, 0x75, 0x08,
    0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81, 0x00, 0x05, 0x08, 0x19, 0x01,
    0x29, 0x05, 0x95, 0x05, 0x75, 0x01, 0x91, 0x02, 0x95, 0x01, 0x75, 0x03, 0x91, 0x01, 0xc0,
];

/// A standard three-byte boot-mouse report descriptor, shared by the host tests.
#[cfg(test)]
pub(crate) const BOOT_MOUSE_3_DESCRIPTOR: [u8; 50] = [
    0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01,
    0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08, 0x95, 0x02, 0x81, 0x06,
    0xc0, 0xc0,
];

/// A four-byte (wheel) boot-mouse report descriptor, shared by the host tests.
#[cfg(test)]
pub(crate) const BOOT_MOUSE_DESCRIPTOR: [u8; 52] = [
    0x05, 0x01, 0x09, 0x02, 0xa1, 0x01, 0x09, 0x01, 0xa1, 0x00, 0x05, 0x09, 0x19, 0x01, 0x29, 0x03,
    0x15, 0x00, 0x25, 0x01, 0x95, 0x03, 0x75, 0x01, 0x81, 0x02, 0x95, 0x01, 0x75, 0x05, 0x81, 0x01,
    0x05, 0x01, 0x09, 0x30, 0x09, 0x31, 0x09, 0x38, 0x15, 0x81, 0x25, 0x7f, 0x75, 0x08, 0x95, 0x03,
    0x81, 0x06, 0xc0, 0xc0,
];
