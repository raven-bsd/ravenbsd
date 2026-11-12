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
        let ecam = match raven_rt::map_mmio(ecam_handle, ECAM_BUS_BYTES) {
            Ok(ecam) => ecam,
            Err(error) => {
                raven_rt::eprintln!("pcid: ecam map: {}", error.description());
                return 1;
            }
        };
        let devices = enumerate(ecam, bus);
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
            if let Some(index) = endpoint.strip_prefix("bar") {
                let index: usize = index.parse().map_err(|_| Errno::EINVAL)?;
                let phys = bar_phys(device.bars, index).ok_or(Errno::ENOENT)?;
                let handle = raven_rt::mmio_create(self.root, phys, PAGE, 0)?;
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

    fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
        Some(u64::from_le_bytes(
            bytes.get(offset..offset.checked_add(8)?)?.try_into().ok()?,
        ))
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
