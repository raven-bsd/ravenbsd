// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Device records and the small driver table used by `pcid` and `devd`.
//!
//! The records are a fixed little-endian layout, not a Rust memory cast, so the parser is
//! bounds-checked and has golden-byte tests. The driver table is a deliberately small
//! `[[driver]]` TOML subset; it is not a general TOML implementation.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Encoded length of one [`DeviceInfo`] record.
pub const DEVICE_INFO_LEN: usize = 40;

/// One PCI function discovered by `pcid`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DeviceInfo {
    /// PCI bus number.
    pub bus: u8,
    /// PCI slot/device number.
    pub slot: u8,
    /// PCI function number.
    pub function: u8,
    /// Base class.
    pub class: u8,
    /// Subclass.
    pub subclass: u8,
    /// Programming interface.
    pub prog_if: u8,
    /// Revision.
    pub revision: u8,
    /// Vendor id.
    pub vendor: u16,
    /// Device id.
    pub device: u16,
    /// The six 32-bit BAR values (0 when absent).
    pub bars: [u32; 6],
    /// PCI interrupt line register (0x3c), as programmed by the firmware.
    pub interrupt_line: u8,
    /// PCI interrupt pin register (0x3d); 0 means the function does not use an INTx pin.
    pub interrupt_pin: u8,
}

impl DeviceInfo {
    /// Encode into exactly [`DEVICE_INFO_LEN`] bytes.
    pub fn encode(&self, out: &mut [u8; DEVICE_INFO_LEN]) {
        out[0] = self.bus;
        out[1] = self.slot;
        out[2] = self.function;
        out[3] = self.class;
        out[4] = self.subclass;
        out[5] = self.prog_if;
        out[6] = self.revision;
        out[7] = 0;
        out[8..10].copy_from_slice(&self.vendor.to_le_bytes());
        out[10..12].copy_from_slice(&self.device.to_le_bytes());
        let mut index = 0usize;
        while index < 6 {
            let offset = 12 + index * 4;
            if let (Some(slot), Some(value)) =
                (out.get_mut(offset..offset + 4), self.bars.get(index))
            {
                slot.copy_from_slice(&value.to_le_bytes());
            }
            index += 1;
        }
        out[36] = self.interrupt_line;
        out[37] = self.interrupt_pin;
        out[38] = 0;
        out[39] = 0;
    }

    /// Decode exactly [`DEVICE_INFO_LEN`] bytes.
    pub fn decode(input: &[u8]) -> Option<Self> {
        if input.len() != DEVICE_INFO_LEN {
            return None;
        }
        let mut bars = [0u32; 6];
        let mut index = 0usize;
        while index < 6 {
            let offset = 12 + index * 4;
            let bytes = input.get(offset..offset + 4)?;
            bars[index] = u32::from_le_bytes(bytes.try_into().ok()?);
            index += 1;
        }
        Some(Self {
            bus: *input.first()?,
            slot: *input.get(1)?,
            function: *input.get(2)?,
            class: *input.get(3)?,
            subclass: *input.get(4)?,
            prog_if: *input.get(5)?,
            revision: *input.get(6)?,
            vendor: u16::from_le_bytes(input.get(8..10)?.try_into().ok()?),
            device: u16::from_le_bytes(input.get(10..12)?.try_into().ok()?),
            bars,
            interrupt_line: *input.get(36)?,
            interrupt_pin: *input.get(37)?,
        })
    }

    /// Stable directory name (`bb:ss.f`).
    pub fn name(&self) -> String {
        alloc::format!("{:02x}:{:02x}.{}", self.bus, self.slot, self.function)
    }
}

/// One driver-table entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DriverSpec {
    /// Driver name used in log lines and the `dev:` scheme.
    pub name: String,
    /// Required PCI vendor id.
    pub vendor: u16,
    /// Required PCI device id.
    pub device: u16,
    /// Executable path in the initfs.
    pub exec: String,
    /// Arguments passed after the executable path.
    pub args: Vec<String>,
    /// Number of launch/exit cycles (1 in normal service use, 10 in the lifecycle qtest).
    pub cycles: u32,
}

/// Parse the small `[[driver]]` table used by `/etc/drivers.toml`.
pub fn parse_drivers(text: &str) -> Result<Vec<DriverSpec>, DriverError> {
    let mut drivers = Vec::new();
    let mut current: Option<DriverSpec> = None;
    for (index, line) in text.lines().enumerate() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line == "[[driver]]" {
            if let Some(driver) = current.take() {
                drivers.push(driver);
            }
            current = Some(DriverSpec {
                name: String::new(),
                vendor: 0,
                device: 0,
                exec: String::new(),
                args: Vec::new(),
                cycles: 1,
            });
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(DriverError { line: index + 1 });
        };
        let key = key.trim();
        let value = value.trim();
        let Some(driver) = current.as_mut() else {
            return Err(DriverError { line: index + 1 });
        };
        match key {
            "name" => driver.name = parse_string(value).ok_or(DriverError { line: index + 1 })?,
            "vendor" => driver.vendor = parse_u16(value).ok_or(DriverError { line: index + 1 })?,
            "device" => driver.device = parse_u16(value).ok_or(DriverError { line: index + 1 })?,
            "exec" => driver.exec = parse_string(value).ok_or(DriverError { line: index + 1 })?,
            "args" => driver.args = parse_array(value).ok_or(DriverError { line: index + 1 })?,
            "cycles" => {
                driver.cycles = value.parse().map_err(|_| DriverError { line: index + 1 })?
            }
            _ => return Err(DriverError { line: index + 1 }),
        }
    }
    if let Some(driver) = current {
        drivers.push(driver);
    }
    for driver in &drivers {
        if driver.name.is_empty()
            || driver.exec.is_empty()
            || !driver.exec.starts_with('/')
            || driver.cycles == 0
            || driver.cycles > 100
        {
            return Err(DriverError { line: 0 });
        }
    }
    Ok(drivers)
}

/// A driver-table parse failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DriverError {
    /// 1-based line, or 0 for a missing required field.
    pub line: usize,
}

fn parse_string(value: &str) -> Option<String> {
    let value = value.strip_prefix('"')?.strip_suffix('"')?;
    if value.contains('"') || value.len() > 128 {
        return None;
    }
    Some(value.to_string())
}

fn parse_u16(value: &str) -> Option<u16> {
    let (digits, radix) = match value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
    {
        Some(digits) => (digits, 16),
        None => (value, 10),
    };
    u16::from_str_radix(digits, radix).ok()
}

fn parse_array(value: &str) -> Option<Vec<String>> {
    let value = value.strip_prefix('[')?.strip_suffix(']')?;
    let mut result = Vec::new();
    if value.trim().is_empty() {
        return Some(result);
    }
    for item in value.split(',') {
        result.push(parse_string(item.trim())?);
    }
    Some(result)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
