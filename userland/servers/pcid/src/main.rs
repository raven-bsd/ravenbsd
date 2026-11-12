// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pcid`: PCI configuration-space enumerator and the `pci:` scheme.
//!
//! The server holds the root-device grant. It maps the ACPI blob, finds the MCFG/ECAM window,
//! enumerates the first PCI bus, and exposes one file per function plus `info` and `barN`
//! endpoints. Opening a BAR endpoint attaches a fresh `MmioRegion` handle to the client.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_proto_dev::{DEVICE_INFO_LEN, DeviceInfo};
    use raven_proto_fs::{DT_REG, DirEntry, DirEntryHeader, S_IFREG, Stat};
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    const PAGE: usize = 4096;
    const ACPI_BLOB: u32 = 1;
    const BLOB_MAGIC: u32 = 0x3143_4152;
    const BLOB_HEADER: usize = 24;
    const BLOB_ENTRY: usize = 20;
    const ECAM_BUS_BYTES: usize = 1 << 20;
    const ECAM_DEVICE_STRIDE: usize = 0x8000;
    const OPEN_OPCODE: u16 = 1;
    const OPEN_READ: u32 = 1;

    struct OpenFile {
        data: Vec<u8>,
        cursor: usize,
    }

    struct PcidHandler {
        root: Handle,
        devices: Vec<DeviceInfo>,
        /// Probed BAR sizes, parallel to `devices` (0 = absent or I/O BAR).
        sizes: Vec<[u64; 6]>,
        /// Physical ECAM base, used to attach one function's configuration page.
        ecam_base: u64,
        files: BTreeMap<u64, OpenFile>,
        pending: Option<(u64, String)>,
    }

    fn main(_args: raven_rt::Args) -> i32 {
        let root = Handle::ROOT_DEVICE;
        let acpi = match raven_rt::boot_blob(root, ACPI_BLOB) {
            Ok(handle) => handle,
            Err(error) => {
                raven_rt::eprintln!("pcid: acpi blob: {}", error.description());
                return 1;
            }
        };
        let acpi_map = match map_acpi(acpi) {
            Ok(blob) => blob,
            Err(error) => {
                raven_rt::eprintln!("pcid: acpi map: {}", error.description());
                return 1;
            }
        };
        let (ecam_base, bus) = match mcfg(acpi_map) {
            Some(value) => value,
            None => {
                raven_rt::eprintln!("pcid: MCFG missing");
                return 1;
            }
        };
        let ecam_handle = match raven_rt::mmio_create(root, ecam_base, ECAM_BUS_BYTES, 0) {
            Ok(handle) => handle,
            Err(error) => {
                raven_rt::eprintln!("pcid: ecam mmio: {}", error.description());
                return 1;
            }
        };
        // One writable mapping serves both directions: BAR sizing writes the BAR registers (all
        // ones, then the original value) and the enumeration only reads. The values are restored
        // before the server starts.
        let ecam_write = match raven_rt::map_mmio_mut(ecam_handle, ECAM_BUS_BYTES) {
            Ok(ecam) => ecam,
            Err(error) => {
                raven_rt::eprintln!("pcid: ecam rw map: {}", error.description());
                return 1;
            }
        };
        let (devices, sizes) = enumerate_and_size(ecam_write, bus);
        for device in &devices {
            raven_rt::println!(
                "pcid: device {} vendor={:#06x} device={:#06x} class={:#04x} subclass={:#04x} irq_line={} irq_pin={}",
                device.name(),
                device.vendor,
                device.device,
                device.class,
                device.subclass,
                device.interrupt_line,
                device.interrupt_pin
            );
        }
        raven_rt::println!("pcid: devices={}", devices.len());
        let mut server = match Server::register("pci") {
            Ok(server) => server,
            Err(error) => {
                raven_rt::eprintln!("pcid: pci: {}", error.description());
                return 1;
            }
        };
        let mut handler = PcidHandler {
            root,
            devices,
            sizes,
            ecam_base,
            files: BTreeMap::new(),
            pending: None,
        };
        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == OPEN_OPCODE
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.pending = Some((packet.packet.file_id, path.to_string()));
            }
            let _ = server.dispatch(packet, &mut handler);
        }
    }

    /// Map the complete ACPI bundle. The kernel captures at most 256 KiB and rounds the memory
    /// object up to pages; the exact size is not exposed, so probe downward from the maximum.
    fn map_acpi(handle: Handle) -> Result<&'static [u8], Errno> {
        let mut len = 256 * 1024;
        loop {
            match raven_rt::map_mmio(handle, len) {
                Ok(blob) => return Ok(blob),
                Err(Errno::EINVAL) if len > PAGE => len /= 2,
                Err(error) => return Err(error),
            }
        }
    }

    fn mcfg(blob: &[u8]) -> Option<(u64, u8)> {
        if read_u32(blob, 0)? != BLOB_MAGIC {
            return None;
        }
        let count = read_u32(blob, 16)? as usize;
        let mut index = 0usize;
        while index < count {
            let entry = BLOB_HEADER.checked_add(index.checked_mul(BLOB_ENTRY)?)?;
            if read_u32(blob, entry)? == u32::from_le_bytes(*b"MCFG") {
                let offset = read_u32(blob, entry.checked_add(8)?)? as usize;
                let length = read_u32(blob, entry.checked_add(12)?)? as usize;
                if offset == 0 || length < 56 {
                    return None;
                }
                let end = offset.checked_add(length)?;
                let mcfg = blob.get(offset..end)?;
                let base = read_u64(mcfg, 44)?;
                let segment = read_u16(mcfg, 52)?;
                let start = read_u8(mcfg, 54)?;
                let last = read_u8(mcfg, 55)?;
                if segment != 0 || base == 0 || start > last {
                    return None;
                }
                return Some((base, start));
            }
            index = index.checked_add(1)?;
        }
        None
    }

    fn enumerate(ecam: &[u8], bus: u8) -> Vec<DeviceInfo> {
        let mut devices = Vec::new();
        let mut slot = 0usize;
        while slot < 32 {
            let mut function = 0usize;
            while function < 8 {
                let offset = usize::from(bus)
                    .saturating_mul(ECAM_BUS_BYTES)
                    .saturating_add(slot.saturating_mul(ECAM_DEVICE_STRIDE))
                    .saturating_add(function.saturating_mul(PAGE));
                let Some(config) = ecam.get(offset..) else {
                    return devices;
                };
                let vendor = read_u16(config, 0).unwrap_or(0xFFFF);
                if vendor == 0xFFFF {
                    if function == 0 {
                        break;
                    }
                    function = function.saturating_add(1);
                    continue;
                }
                let mut bars = [0u32; 6];
                let mut bar = 0usize;
                while bar < 6 {
                    bars[bar] = read_u32(config, 0x10 + bar * 4).unwrap_or(0);
                    bar = bar.saturating_add(1);
                }
                devices.push(DeviceInfo {
                    bus,
                    slot: slot as u8,
                    function: function as u8,
                    class: read_u8(config, 0x0b).unwrap_or(0),
                    subclass: read_u8(config, 0x0a).unwrap_or(0),
                    prog_if: read_u8(config, 0x09).unwrap_or(0),
                    revision: read_u8(config, 0x08).unwrap_or(0),
                    vendor,
                    device: read_u16(config, 2).unwrap_or(0),
                    bars,
                    interrupt_line: read_u8(config, 0x3c).unwrap_or(0),
                    interrupt_pin: read_u8(config, 0x3d).unwrap_or(0),
                });
                function = function.saturating_add(1);
            }
            slot = slot.saturating_add(1);
        }
        devices
    }

    /// Size the BARs of every enumerated function, then hand both lists back.
    fn enumerate_and_size(ecam: &mut [u8], bus: u8) -> (Vec<DeviceInfo>, Vec<[u64; 6]>) {
        let devices = enumerate(ecam, bus);
        let sizes = probe_sizes(ecam, &devices);
        (devices, sizes)
    }

    /// Offset of one function's 4 KiB configuration page inside the ECAM bus window.
    fn ecam_offset(bus: u8, slot: u8, function: u8) -> usize {
        usize::from(bus)
            .saturating_mul(ECAM_BUS_BYTES)
            .saturating_add(usize::from(slot).saturating_mul(ECAM_DEVICE_STRIDE))
            .saturating_add(usize::from(function).saturating_mul(PAGE))
    }

    /// Probe each memory BAR's size by writing all ones and reading the mask back.
    ///
    /// The original value is restored immediately. I/O and absent BARs report zero; a 64-bit BAR
    /// consumes two slots and the second one stays zero.
    fn probe_sizes(ecam: &mut [u8], devices: &[DeviceInfo]) -> Vec<[u64; 6]> {
        const MAX_BAR_BYTES: u64 = 256 * 1024 * 1024;
        let mut sizes = Vec::with_capacity(devices.len());
        for device in devices {
            let offset = ecam_offset(device.bus, device.slot, device.function);
            let mut entry = [0u64; 6];
            let mut index = 0usize;
            while index < 6 {
                let Some(config) = ecam.get_mut(offset..) else {
                    break;
                };
                let register = 0x10 + index * 4;
                let value = read_u32(config, register).unwrap_or(0xFFFF_FFFF);
                if value == 0 || value & 1 != 0 {
                    index += 1;
                    continue;
                }
                let wide = (value >> 1) & 0x3 == 2 && index < 5;
                let saved_high = if wide {
                    read_u32(config, register + 4).unwrap_or(0)
                } else {
                    0
                };
                let _ = write_u32(config, register, 0xFFFF_FFFF);
                if wide {
                    let _ = write_u32(config, register + 4, 0xFFFF_FFFF);
                }
                let mask_low = read_u32(config, register).unwrap_or(0) & !0xF;
                let mask_high = if wide {
                    read_u32(config, register + 4).unwrap_or(0)
                } else {
                    0
                };
                let _ = write_u32(config, register, value);
                if wide {
                    let _ = write_u32(config, register + 4, saved_high);
                }
                let mask = if wide {
                    u64::from(mask_low) | (u64::from(mask_high) << 32)
                } else {
                    u64::from(mask_low)
                };
                if mask != 0 {
                    // The size is the lowest set bit of the mask: devices only decode the address
                    // bits they implement, so the high bits of the all-ones write read back as
                    // zero and `!mask + 1` would be nonsense.
                    let size = mask & mask.wrapping_neg();
                    if size <= MAX_BAR_BYTES {
                        entry[index] = size;
                    }
                }
                index += if wide { 2 } else { 1 };
            }
            sizes.push(entry);
        }
        sizes
    }

    /// One line per BAR for the `bars` endpoint: `index kind size`.
    fn bars_text(device: &DeviceInfo, sizes: &[u64; 6]) -> String {
        let mut text = String::new();
        for index in 0..6 {
            let value = device.bars.get(index).copied().unwrap_or(0);
            let kind = if value == 0 {
                "absent"
            } else if value & 1 != 0 {
                "io"
            } else {
                "memory"
            };
            let size = sizes.get(index).copied().unwrap_or(0);
            text.push_str(&alloc::format!("{index} {kind} {size}\n"));
        }
        text
    }

    impl Handler for PcidHandler {
        fn open(
            &mut self,
            path: &str,
            flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path || flags & OPEN_READ == 0 {
                return Err(Errno::EINVAL.into());
            }
            if path.is_empty() || path == "/" {
                let data = self.directory();
                self.files.insert(file_id, OpenFile { data, cursor: 0 });
                return Ok(Opened::File(file_id));
            }
            let rest = path.strip_prefix('/').ok_or(Errno::EINVAL)?;
            let (name, endpoint) = rest.split_once('/').unwrap_or((rest, ""));
            let device = self
                .devices
                .iter()
                .find(|device| device.name() == name)
                .copied()
                .ok_or(Errno::ENOENT)?;
            if endpoint == "info" {
                let mut bytes = [0u8; DEVICE_INFO_LEN];
                device.encode(&mut bytes);
                self.files.insert(
                    file_id,
                    OpenFile {
                        data: bytes.to_vec(),
                        cursor: 0,
                    },
                );
                return Ok(Opened::File(file_id));
            }
            if endpoint == "bars" {
                let position = self
                    .devices
                    .iter()
                    .position(|candidate| candidate.name() == name)
                    .ok_or(Errno::ENOENT)?;
                let sizes = self.sizes.get(position).copied().unwrap_or([0u64; 6]);
                let data = bars_text(&device, &sizes).into_bytes();
                self.files.insert(file_id, OpenFile { data, cursor: 0 });
                return Ok(Opened::File(file_id));
            }
            if endpoint == "config" {
                // One function's 4 KiB configuration page, attached read-only; the client maps it
                // with `map_mmio` and walks the capability list through `raven-driver`.
                let phys = self.ecam_base.saturating_add(ecam_offset(
                    device.bus,
                    device.slot,
                    device.function,
                ) as u64);
                let handle = raven_rt::mmio_create(self.root, phys, PAGE, 0)?;
                return Ok(Opened::Attach(handle));
            }
            if let Some(index) = endpoint.strip_prefix("bar") {
                let index: usize = index.parse().map_err(|_| Errno::EINVAL)?;
                let position = self
                    .devices
                    .iter()
                    .position(|candidate| candidate.name() == name)
                    .ok_or(Errno::ENOENT)?;
                let size = self
                    .sizes
                    .get(position)
                    .and_then(|sizes| sizes.get(index))
                    .copied()
                    .unwrap_or(0);
                if size == 0 {
                    return Err(Errno::ENOENT.into());
                }
                let phys = bar_phys(device.bars, index).ok_or(Errno::ENOENT)?;
                let handle = raven_rt::mmio_create(
                    self.root,
                    phys,
                    usize::try_from(size).unwrap_or(PAGE),
                    0,
                )?;
                return Ok(Opened::Attach(handle));
            }
            Err(Errno::ENOENT.into())
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let cursor = self.files.get(&file).ok_or(Errno::EBADF)?.cursor;
            let remaining = self
                .files
                .get(&file)
                .ok_or(Errno::EBADF)?
                .data
                .get(cursor..)
                .unwrap_or(&[]);
            let length = remaining.len().min(buffer.len());
            let target = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
            target.copy_from_slice(remaining.get(..length).ok_or(Errno::EINVAL)?);
            if let Some(open) = self.files.get_mut(&file) {
                open.cursor = open.cursor.saturating_add(length);
            }
            Ok(length)
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.files
                .remove(&file)
                .map(|_| ())
                .ok_or_else(|| Errno::EBADF.into())
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let data = &self.files.get(&file).ok_or(Errno::EBADF)?.data;
            let size = data.len() as u64;
            Ok(Stat {
                ino: file.wrapping_add(1),
                size,
                blocks: size.div_ceil(512),
                mode: S_IFREG | 0o444,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    impl PcidHandler {
        fn directory(&self) -> Vec<u8> {
            let mut data = Vec::new();
            for (index, device) in self.devices.iter().enumerate() {
                let name = device.name();
                let entry = DirEntry {
                    header: DirEntryHeader {
                        ino: index as u64 + 1,
                        kind: DT_REG,
                        name_len: u8::try_from(name.len()).unwrap_or(0),
                        pad: [0; 6],
                    },
                    name: name.as_bytes(),
                };
                let length = entry.encoded_len();
                let start = data.len();
                data.resize(start + length, 0);
                let _ = entry.encode(&mut data[start..]);
            }
            data
        }
    }

    fn bar_phys(bars: [u32; 6], index: usize) -> Option<u64> {
        let value = *bars.get(index)?;
        if value == 0 || value & 1 != 0 {
            return None;
        }
        match (value >> 1) & 0x3 {
            0 => Some(u64::from(value & !0xF)),
            2 => {
                let high = *bars.get(index.checked_add(1)?)?;
                Some(u64::from(value & !0xF) | (u64::from(high) << 32))
            }
            _ => None,
        }
    }

    fn read_u8(bytes: &[u8], offset: usize) -> Option<u8> {
        bytes.get(offset).copied()
    }

    fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
        Some(u16::from_le_bytes(
            bytes.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
        ))
    }

    fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?,
        ))
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) -> Option<()> {
        let end = offset.checked_add(4)?;
        bytes
            .get_mut(offset..end)?
            .copy_from_slice(&value.to_le_bytes());
        Some(())
    }

    fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
        ))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
