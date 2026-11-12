// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! virtio-blk userspace driver (K15).
//!
//! `devd` launches this program with `bar=4` and `resources=dma+irq`, so the child receives the
//! modern virtio-pci BAR (handle 3), the function's configuration page (4), a 64 KiB DMA buffer
//! (5) and the legacy IRQ (6). The driver finds the four virtio-pci capabilities, resets the
//! function, negotiates `VIRTIO_F_VERSION_1` (plus `VIRTIO_BLK_F_FLUSH` and the read-only feature
//! when offered), reads its capacity, and serves the `disk:` scheme with the `raven-proto-block`
//! framing (`INFO`, `READ`, `WRITE`, `FLUSH`).
//!
//! One split request queue of at most eight entries lives in the DMA buffer; a request is the
//! bounded chain header -> optional data -> status. The data window is 32 KiB, so a 128-sector
//! protocol request is split into two chunks; every index and length is checked before a
//! descriptor is published. Completion is interrupt-driven with a bounded wait; a timeout resets
//! the device and returns `EIO`.
//!
//! With the `test` argument the driver also runs the qtest pattern self-test: cycle 1 writes,
//! flushes and verifies a deterministic pattern across the data area, writes a marker sector, then
//! serves `disk:` until a client flushes; cycle 2 re-reads the persisted data and compares the
//! checksum recorded in the marker. The production driver (no `test`) never writes to the disk
//! beyond what a client asks for.
//!
//! It keeps `#![forbid(unsafe_code)]`: MMIO goes through `raven_driver::MmioRegion`, DMA through
//! `DmaRegion`, and the interrupt through `Irq`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec;

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_driver::{AccessError, ConfigRegion, DmaRegion, Irq, MmioRegion};
    use raven_proto_block::{
        BlockInfo, INFO_LEN, Op as WireOp, REPLY_HEADER_LEN, Request as WireRequest, SECTOR_SIZE,
        encode_reply, write_reply_header,
    };
    use raven_proto_fs::OpenFlags;
    use raven_rt::entry;
    use raven_scheme::{Ctx, Handler, HandlerResult, OpenTable, Opened, Server};
    use virtio_blk::common::{
        DEVICE_FEATURE, DEVICE_FEATURE_SELECT, DEVICE_STATUS, DRIVER_FEATURE,
        DRIVER_FEATURE_SELECT, NUM_QUEUES, QUEUE_AVAIL, QUEUE_DESC, QUEUE_ENABLE,
        QUEUE_MSIX_VECTOR, QUEUE_NOTIFY_OFF, QUEUE_SELECT, QUEUE_SIZE, QUEUE_USED,
    };
    use virtio_blk::{
        BLK_F_FLUSH, BLK_F_RO, BLK_S_OK, BLK_S_UNSUPP, BLK_T_FLUSH, BLK_T_IN, BLK_T_OUT,
        CAP_COMMON_CFG, CAP_DEVICE_CFG, CAP_ISR_CFG, CAP_NOTIFY_CFG, CHUNK_SECTORS, DESC_F_NEXT,
        DESC_F_WRITE, Descriptor, F_VERSION_1, Features, Hasher, MAX_SECTORS, RequestError,
        RingLayout, STATUS_ACKNOWLEDGE, STATUS_DRIVER, STATUS_DRIVER_OK, STATUS_FEATURES_OK,
        decode_used, encode_descriptor, encode_request, fill_pattern, marker_decode, marker_encode,
        queue_size, validate_request, verify_pattern,
    };

    entry!(main);

    /// A fresh child has 0 (self), 1 (initfs), 2 (namespace); `devd` grants in order.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);

    /// The driver table matches `1af4:1042` (modern virtio-blk).
    const VIRTIO_VENDOR: u16 = 0x1AF4;
    const VIRTIO_BLK_DEVICE: u16 = 0x1042;

    /// Where the request queue, header, status and data chunk live in the DMA buffer.
    const RING_BASE: usize = 0x0000;
    const HEADER_OFFSET: usize = 0x0100;
    const STATUS_OFFSET: usize = 0x0110;
    const DATA_OFFSET: usize = 0x0200;
    const CHUNK_BYTES: usize = (CHUNK_SECTORS as usize) * (SECTOR_SIZE as usize);
    /// Legacy INTx wait per completion (one second).
    const IRQ_WAIT_NS: u64 = 1_000_000_000;

    /// qtest fixture layout: a small GPT lives in the first MiB; the pattern covers the data area
    /// after it, the marker sits in an unused prefix sector, and partition 1 is the probe's region.
    const MARKER_LBA: u64 = 1900;
    const PARTITION_FIRST: u64 = 34;
    const PARTITION_LAST: u64 = 1800;
    const DATA_FIRST: u64 = 2048;
    /// Data checksum sample stored in the marker: the first MiB of the data area.
    const SAMPLE_SECTORS: u64 = 2048;
    const PARTITION_FILL: u8 = 0x3C;
    const DATA_FILL: u8 = 0xA5;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("virtio-blk: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let mut bar_index = 0usize;
        let mut bar_size = 0usize;
        let mut dma_size = 0usize;
        let mut irq_line = 0u32;
        let mut test = false;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar=") {
                bar_index = value
                    .parse()
                    .map_err(|_| format!("bad bar argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("bar_size=") {
                bar_size = value
                    .parse()
                    .map_err(|_| format!("bad bar_size argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("dma=") {
                dma_size = value
                    .parse()
                    .map_err(|_| format!("bad dma argument `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("irq=") {
                irq_line = value
                    .parse()
                    .map_err(|_| format!("bad irq argument `{argument}`"))?;
            } else if argument == "test" {
                test = true;
            }
        }
        if bar_size == 0 || dma_size < DATA_OFFSET + CHUNK_BYTES {
            return Err("devd did not pass a usable BAR and DMA buffer".to_string());
        }

        let bar_window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size)
            .map_err(|error| format!("bar map: {}", error.description()))?;
        let bar = MmioRegion::new(bar_window);
        let config_window = raven_rt::map_mmio(CONFIG_HANDLE, raven_driver::PAGE)
            .map_err(|error| format!("config map: {}", error.description()))?;
        let config = ConfigRegion::new(config_window);
        let dma_info = raven_rt::dma_info(DMA_HANDLE)
            .map_err(|error| format!("dma info: {}", error.description()))?;
        let dma_window = raven_rt::map_mmio_mut(DMA_HANDLE, dma_size)
            .map_err(|error| format!("dma map: {}", error.description()))?;
        // The queue, header and status need bounded element access (`DmaRegion`); the data window
        // is a separate, disjoint part of the same mapping used as an ordinary slice, so the
        // 64 KiB payload moves with a compiled copy instead of 8,000 volatile accesses.
        let (meta, data) = dma_window.split_at_mut(DATA_OFFSET);
        let dma = DmaRegion::new(meta, dma_info.bus_addr);
        let irq = Irq::new(IRQ_HANDLE);

        let mut device =
            VirtioBlk::open(bar, config, dma, data, irq, bar_index, bar_size, irq_line)?;
        raven_rt::println!(
            "virtio-blk: capacity={} sector_size={} ro={}",
            device.capacity,
            SECTOR_SIZE,
            u8::from(device.read_only)
        );
        raven_rt::println!(
            "virtio-blk: features=0x{:08x} version1={}",
            device.features.low,
            u8::from(device.features.high & F_VERSION_1 != 0)
        );
        raven_rt::println!(
            "virtio-blk: queue0 size={} notify={:#x}",
            device.qsize,
            device.notify_offset
        );
        raven_rt::println!(
            "virtio-blk: dma bus={:#x} len={} irq={}",
            device.dma.bus_address(),
            DATA_OFFSET + device.data.len(),
            irq_line
        );

        // Test mode: cycle 2 is recognised before the scheme is registered. The marker proves the
        // first cycle's pattern reached the platter; verify the persisted data and exit without
        // serving. A fresh disk has no marker and takes the cycle-1 path below.
        if test {
            let mut marker = [0u8; SECTOR_SIZE as usize];
            device
                .read_sectors(MARKER_LBA, &mut marker)
                .map_err(|error| format!("marker read: {}", error.description()))?;
            if let Some(stored) = marker_decode(&marker) {
                verify_cycle_two(&mut device, stored)?;
                device.reset()?;
                raven_rt::println!("virtio-blk: done");
                return Ok(());
            }
        }

        let mut handler = DiskHandler::new(device, test);
        let mut server = match Server::register("disk") {
            Ok(server) => server,
            Err(error) => {
                return Err(format!("register disk: {}", error.description()));
            }
        };
        raven_rt::println!("virtio-blk: registered disk");

        // In test mode svcd's readiness probe is the first client: serve its open/close, then run
        // the pattern pass while blockd's queued scan waits. This keeps `disk:` in blockd's
        // namespace (it is registered before blockd starts) and still lets the whole-disk pattern
        // run before any client data request is answered.
        if test {
            loop {
                let Some(packet) = server.poll(None) else {
                    continue;
                };
                if packet.packet.opcode == 1
                    && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
                {
                    handler.expect_open(packet.packet.file_id, path);
                }
                let _ = server.dispatch(packet, &mut handler);
                if handler.closed_opens() >= 1 {
                    break;
                }
            }
            pattern_first(&mut handler.device)?;
            raven_rt::println!("virtio-blk: serving");
        }

        loop {
            let Some(packet) = server.poll(None) else {
                continue;
            };
            if packet.packet.opcode == 1
                && let Ok(path) = core::str::from_utf8(packet.packet.path_bytes())
            {
                handler.expect_open(packet.packet.file_id, path);
            }
            let _ = server.dispatch(packet, &mut handler);
            if handler.shutdown_requested() {
                break;
            }
        }
        handler.device.reset()?;
        raven_rt::println!("virtio-blk: serve done");
        Ok(())
    }

    /// The four virtio-pci capabilities, all on the BAR the driver was given.
    struct Caps {
        common: Region,
        notify: Region,
        isr: Region,
        device: Region,
        notify_multiplier: u32,
    }

    #[derive(Clone, Copy)]
    struct Region {
        offset: usize,
        length: usize,
    }

    fn read_capabilities(
        config: &ConfigRegion,
        bar_index: usize,
        bar_size: usize,
    ) -> Result<Caps, String> {
        let mut common = None;
        let mut notify = None;
        let mut isr = None;
        let mut device = None;
        let mut multiplier = 0u32;
        // QEMU prepends each capability, so the chain runs from the highest offset down. Walk it
        // with a bounded visited list instead of assuming increasing offsets.
        let mut seen = [0u8; raven_driver::MAX_CAPABILITIES];
        let mut seen_len = 0usize;
        let mut current = config.first_capability().map_err(describe)?;
        while let Some(capability) = current {
            let offset = usize::from(capability.offset);
            if seen
                .get(..seen_len)
                .unwrap_or(&[])
                .contains(&capability.offset)
            {
                break;
            }
            let Some(slot) = seen.get_mut(seen_len) else {
                break;
            };
            *slot = capability.offset;
            seen_len += 1;
            if capability.id != 0x09 {
                current = next_capability(config, capability.next)?;
                continue;
            }
            let cfg_type = config.read_u8(offset + 3).map_err(describe)?;
            if !(CAP_COMMON_CFG..=CAP_DEVICE_CFG).contains(&cfg_type) {
                // Type 5 is the optional PCI-configuration window (bar 0 by definition).
                current = next_capability(config, capability.next)?;
                continue;
            }
            let cap_bar = config.read_u8(offset + 4).map_err(describe)?;
            let region_offset = usize::try_from(config.read_u32(offset + 8).map_err(describe)?)
                .unwrap_or(usize::MAX);
            let length = usize::try_from(config.read_u32(offset + 12).map_err(describe)?)
                .unwrap_or(usize::MAX);
            if usize::from(cap_bar) != bar_index {
                return Err(format!(
                    "virtio capability type {cfg_type} is on bar{cap_bar}, driver has bar{bar_index}"
                ));
            }
            let end = region_offset
                .checked_add(length)
                .ok_or_else(|| "capability range overflows".to_string())?;
            if length == 0 || end > bar_size {
                return Err(format!("capability type {cfg_type} is outside the BAR"));
            }
            let region = Region {
                offset: region_offset,
                length,
            };
            match cfg_type {
                CAP_COMMON_CFG => {
                    if length < 56 {
                        return Err("common configuration is too short".to_string());
                    }
                    common = Some(region);
                }
                CAP_NOTIFY_CFG => {
                    if length < 20 {
                        return Err("notify capability is too short".to_string());
                    }
                    multiplier = config.read_u32(offset + 16).map_err(describe)?;
                    notify = Some(region);
                }
                CAP_ISR_CFG => {
                    if length < 1 {
                        return Err("ISR capability is empty".to_string());
                    }
                    isr = Some(region);
                }
                CAP_DEVICE_CFG => {
                    if length < 8 {
                        return Err("device configuration is too short".to_string());
                    }
                    device = Some(region);
                }
                _ => {}
            }
            current = next_capability(config, capability.next)?;
        }
        Ok(Caps {
            common: common.ok_or_else(|| "no common configuration capability".to_string())?,
            notify: notify.ok_or_else(|| "no notify capability".to_string())?,
            isr: isr.ok_or_else(|| "no ISR capability".to_string())?,
            device: device.ok_or_else(|| "no device configuration capability".to_string())?,
            notify_multiplier: multiplier,
        })
    }

    fn next_capability(
        config: &ConfigRegion,
        next: u8,
    ) -> Result<Option<raven_driver::Capability>, String> {
        if next == 0 {
            return Ok(None);
        }
        config.capability(next).map_err(describe)
    }

    fn common(caps: &Caps, register: usize) -> usize {
        caps.common.offset + register
    }

    /// The live device: BAR, DMA, IRQ, queue and negotiated geometry.
    struct VirtioBlk {
        bar: MmioRegion,
        dma: DmaRegion,
        /// The data window of the DMA buffer (offset [`DATA_OFFSET`] onwards).
        data: &'static mut [u8],
        irq: Irq,
        caps: Caps,
        layout: RingLayout,
        qsize: u16,
        notify_offset: usize,
        capacity: u64,
        read_only: bool,
        features: Features,
        avail_seq: u16,
        used_seq: u16,
        broken: bool,
    }

    impl VirtioBlk {
        #[allow(clippy::too_many_arguments)] // the five transferred handles plus the BAR geometry
        fn open(
            mut bar: MmioRegion,
            config: ConfigRegion,
            mut dma: DmaRegion,
            data: &'static mut [u8],
            irq: Irq,
            bar_index: usize,
            bar_size: usize,
            _irq_line: u32,
        ) -> Result<Self, String> {
            let (vendor, device) = config.ids().map_err(describe)?;
            if vendor != VIRTIO_VENDOR || device != VIRTIO_BLK_DEVICE {
                return Err(format!(
                    "unexpected function {vendor:#06x}:{device:#06x}, expected 1af4:1042"
                ));
            }
            raven_rt::println!("virtio-blk: id={vendor:04x}:{device:04x}");
            let caps = read_capabilities(&config, bar_index, bar_size)?;
            raven_rt::println!(
                "virtio-blk: common={:#x} notify={:#x} isr={:#x} device={:#x}",
                caps.common.offset,
                caps.notify.offset,
                caps.isr.offset,
                caps.device.offset
            );
            let features = reset_and_negotiate(&mut bar, &caps)?;
            // The capacity field is 8 bytes at the start of the device configuration; read it as
            // two 32-bit words because the capability only guarantees 4-byte alignment.
            let low = bar.read_u32(caps.device.offset).map_err(describe)?;
            let high = bar.read_u32(caps.device.offset + 4).map_err(describe)?;
            let capacity = u64::from(low) | (u64::from(high) << 32);
            if capacity == 0 {
                return Err("device reports zero capacity".to_string());
            }
            let read_only = features.has(5);

            let queues = bar.read_u16(common(&caps, NUM_QUEUES)).map_err(describe)?;
            if queues == 0 {
                return Err("device offers no request queue".to_string());
            }
            bar.write_u16(common(&caps, QUEUE_SELECT), 0)
                .map_err(describe)?;
            let maximum = bar.read_u16(common(&caps, QUEUE_SIZE)).map_err(describe)?;
            let qsize = queue_size(maximum)
                .ok_or_else(|| format!("device queue size {maximum} is too small"))?;
            let layout = RingLayout::new(RING_BASE, qsize);
            if layout.end > dma.len() || data.len() < CHUNK_BYTES {
                return Err("DMA buffer is too small for the request queue".to_string());
            }
            bar.write_u16(common(&caps, QUEUE_SIZE), qsize)
                .map_err(describe)?;
            bar.write_u16(common(&caps, QUEUE_MSIX_VECTOR), 0xFFFF)
                .map_err(describe)?;
            bar.write_u64(
                common(&caps, QUEUE_DESC),
                dma.bus_address() + layout.desc as u64,
            )
            .map_err(describe)?;
            bar.write_u64(
                common(&caps, QUEUE_AVAIL),
                dma.bus_address() + layout.avail as u64,
            )
            .map_err(describe)?;
            bar.write_u64(
                common(&caps, QUEUE_USED),
                dma.bus_address() + layout.used as u64,
            )
            .map_err(describe)?;
            bar.write_u16(common(&caps, QUEUE_ENABLE), 1)
                .map_err(describe)?;
            if bar
                .read_u16(common(&caps, QUEUE_ENABLE))
                .map_err(describe)?
                != 1
            {
                return Err("request queue did not enable".to_string());
            }
            let notify = bar
                .read_u16(common(&caps, QUEUE_NOTIFY_OFF))
                .map_err(describe)?;
            let notify_offset = usize::from(notify) * caps.notify_multiplier as usize;

            // The queue is programmed; the device is live from here.
            write_status(
                &mut bar,
                &caps,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
            )?;
            // The available-ring flags stay zero and the used ring starts at zero; the data window
            // is not assumed to be zeroed by the kernel.
            dma.write_u16(layout.avail, 0).map_err(describe)?;
            dma.publish();
            Ok(Self {
                bar,
                dma,
                data,
                irq,
                caps,
                layout,
                qsize,
                notify_offset,
                capacity,
                read_only,
                features,
                avail_seq: 0,
                used_seq: 0,
                broken: false,
            })
        }

        fn info(&self) -> BlockInfo {
            BlockInfo {
                sector_size: SECTOR_SIZE,
                read_only: u32::from(self.read_only),
                sectors: self.capacity,
                reserved: 0,
            }
        }

        fn reset(&mut self) -> Result<(), String> {
            write_status(&mut self.bar, &self.caps, 0)?;
            let mut spins = 0;
            while self
                .bar
                .read_u8(common(&self.caps, DEVICE_STATUS))
                .map_err(describe)?
                != 0
            {
                spins += 1;
                if spins > 1000 {
                    return Err("device did not reset".to_string());
                }
                raven_rt::yield_now();
            }
            Ok(())
        }

        fn notify(&mut self, queue: u16) -> Result<(), Errno> {
            let address = self.caps.notify.offset + self.notify_offset;
            let end = address.checked_add(2).ok_or(Errno::EIO)?;
            if end > self.caps.notify.offset + self.caps.notify.length || end > self.bar.len() {
                return Err(Errno::EIO);
            }
            self.bar.write_u16(address, queue).map_err(device_errno)
        }

        /// Copy a request payload into the DMA data window.
        ///
        /// The queue metadata still goes through `DmaRegion`'s bounded volatile accessors; the
        /// payload is the disjoint tail of the same mapping, so a normal slice copy is both safe
        /// and fast. The publish/acquire fences around the request order this copy against the
        /// device handshake.
        fn load_data(&mut self, buffer: &[u8]) -> Result<(), Errno> {
            let target = self.data.get_mut(..buffer.len()).ok_or(Errno::EINVAL)?;
            target.copy_from_slice(buffer);
            Ok(())
        }

        /// Copy the DMA data window into `buffer`.
        fn store_data(&mut self, buffer: &mut [u8]) -> Result<(), Errno> {
            let source = self.data.get(..buffer.len()).ok_or(Errno::EINVAL)?;
            buffer.copy_from_slice(source);
            Ok(())
        }

        fn run_request(&mut self, op: u32, sector: u64, buffer: &mut [u8]) -> Result<(), Errno> {
            if self.broken {
                return Err(Errno::EIO);
            }
            if !buffer.len().is_multiple_of(SECTOR_SIZE as usize) || buffer.len() > CHUNK_BYTES {
                return Err(Errno::EINVAL);
            }
            if op == BLK_T_OUT && !buffer.is_empty() {
                self.load_data(buffer)?;
            }
            self.dma
                .write_bytes(HEADER_OFFSET, &encode_request(op, sector))
                .map_err(device_errno)?;
            self.dma
                .write_u8(STATUS_OFFSET, 0xFF)
                .map_err(device_errno)?;

            let data_len = buffer.len();
            let has_data = data_len > 0;
            let mut data_flags = DESC_F_NEXT;
            if op == BLK_T_IN {
                data_flags |= DESC_F_WRITE;
            }
            let header = Descriptor {
                addr: self.dma.bus_address() + HEADER_OFFSET as u64,
                len: 16,
                flags: DESC_F_NEXT,
                next: if has_data { 1 } else { 2 },
            };
            let data = Descriptor {
                addr: self.dma.bus_address() + DATA_OFFSET as u64,
                len: data_len as u32,
                flags: data_flags,
                next: 2,
            };
            let status = Descriptor {
                addr: self.dma.bus_address() + STATUS_OFFSET as u64,
                len: 1,
                flags: DESC_F_WRITE,
                next: 0,
            };
            self.dma
                .write_bytes(self.layout.desc, &encode_descriptor(header))
                .map_err(device_errno)?;
            self.dma
                .write_bytes(self.layout.desc + 16, &encode_descriptor(data))
                .map_err(device_errno)?;
            self.dma
                .write_bytes(self.layout.desc + 32, &encode_descriptor(status))
                .map_err(device_errno)?;

            let slot = self.avail_seq % self.qsize;
            let ring_slot = self.layout.avail_slot(slot).ok_or(Errno::EIO)?;
            self.dma.write_u16(ring_slot, 0).map_err(device_errno)?;
            self.avail_seq = self.avail_seq.wrapping_add(1);
            self.dma
                .write_u16(self.layout.avail_idx(), self.avail_seq)
                .map_err(device_errno)?;
            self.dma.publish();
            self.notify(0)?;

            let mut rounds = 0usize;
            loop {
                rounds += 1;
                if rounds > 3 {
                    return self.fail();
                }
                let fired = match self.irq.wait(IRQ_WAIT_NS) {
                    Ok(fired) => fired,
                    Err(_) => return self.fail(),
                };
                if fired == 0 {
                    return self.fail();
                }
                let _isr = self
                    .bar
                    .read_u8(self.caps.isr.offset)
                    .map_err(device_errno)?;
                let _ = self.irq.ack();
                self.dma.acquire();
                let used = self
                    .dma
                    .read_u16(self.layout.used_idx())
                    .map_err(device_errno)?;
                if used == self.used_seq {
                    continue;
                }
                if used.wrapping_sub(self.used_seq) != 1 {
                    return self.fail();
                }
                break;
            }

            self.dma.acquire();
            let used = self
                .dma
                .read_u16(self.layout.used_idx())
                .map_err(device_errno)?;
            if used.wrapping_sub(self.used_seq) != 1 {
                return self.fail();
            }
            let slot = self.used_seq % self.qsize;
            self.used_seq = self.used_seq.wrapping_add(1);
            let offset = self.layout.used_slot(slot).ok_or(Errno::EIO)?;
            let mut entry = [0u8; 8];
            self.dma
                .read_bytes(offset, &mut entry)
                .map_err(device_errno)?;
            let used_entry = decode_used(&entry).ok_or(Errno::EIO)?;
            if used_entry.id != 0 {
                return self.fail();
            }
            let status = self.dma.read_u8(STATUS_OFFSET).map_err(device_errno)?;
            match status {
                BLK_S_OK => {
                    if op == BLK_T_IN && data_len > 0 {
                        self.store_data(buffer)?;
                    }
                    Ok(())
                }
                BLK_S_UNSUPP => Err(Errno::EOPNOTSUPP),
                _ => Err(Errno::EIO),
            }
        }

        /// Poll the used ring for up to two seconds; `true` when the request completed.
        /// A completion timeout or a malformed used entry: reset and refuse further requests.
        fn fail(&mut self) -> Result<(), Errno> {
            self.broken = true;
            let _ = self.reset();
            Err(Errno::EIO)
        }

        fn read_sectors(&mut self, sector: u64, buffer: &mut [u8]) -> Result<(), Errno> {
            self.run_chunked(BLK_T_IN, sector, buffer)
        }

        fn write_sectors(&mut self, sector: u64, buffer: &[u8]) -> Result<(), Errno> {
            if self.read_only {
                return Err(Errno::EROFS);
            }
            if !buffer.len().is_multiple_of(SECTOR_SIZE as usize)
                || buffer.len() > MAX_SECTORS as usize * SECTOR_SIZE as usize
            {
                return Err(Errno::EINVAL);
            }
            let mut offset = 0usize;
            while offset < buffer.len() {
                let end = (offset + CHUNK_BYTES).min(buffer.len());
                let lba = sector + (offset / SECTOR_SIZE as usize) as u64;
                let chunk = buffer.get(offset..end).ok_or(Errno::EINVAL)?;
                let mut scratch = vec![0u8; chunk.len()];
                scratch.copy_from_slice(chunk);
                self.run_request(BLK_T_OUT, lba, &mut scratch)?;
                offset = end;
            }
            Ok(())
        }

        fn run_chunked(&mut self, op: u32, sector: u64, buffer: &mut [u8]) -> Result<(), Errno> {
            if !buffer.len().is_multiple_of(SECTOR_SIZE as usize)
                || buffer.len() > MAX_SECTORS as usize * SECTOR_SIZE as usize
            {
                return Err(Errno::EINVAL);
            }
            let mut offset = 0usize;
            while offset < buffer.len() {
                let end = (offset + CHUNK_BYTES).min(buffer.len());
                let lba = sector + (offset / SECTOR_SIZE as usize) as u64;
                let chunk = buffer.get_mut(offset..end).ok_or(Errno::EINVAL)?;
                self.run_request(op, lba, chunk)?;
                offset = end;
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<(), Errno> {
            if !self.features.has(9) {
                return Err(Errno::EOPNOTSUPP);
            }
            let mut empty = [];
            self.run_request(BLK_T_FLUSH, 0, &mut empty)
        }
    }

    fn device_errno(_error: AccessError) -> Errno {
        Errno::EIO
    }

    fn write_status(bar: &mut MmioRegion, caps: &Caps, status: u8) -> Result<(), String> {
        bar.write_u8(common(caps, DEVICE_STATUS), status)
            .map_err(describe)
    }

    fn reset_and_negotiate(bar: &mut MmioRegion, caps: &Caps) -> Result<Features, String> {
        write_status(bar, caps, 0)?;
        let mut spins = 0;
        while bar.read_u8(common(caps, DEVICE_STATUS)).map_err(describe)? != 0 {
            spins += 1;
            if spins > 1000 {
                return Err("device did not reset".to_string());
            }
            raven_rt::yield_now();
        }
        write_status(bar, caps, STATUS_ACKNOWLEDGE)?;
        write_status(bar, caps, STATUS_ACKNOWLEDGE | STATUS_DRIVER)?;

        let mut device = Features::default();
        bar.write_u32(common(caps, DEVICE_FEATURE_SELECT), 0)
            .map_err(describe)?;
        device.low = bar
            .read_u32(common(caps, DEVICE_FEATURE))
            .map_err(describe)?;
        bar.write_u32(common(caps, DEVICE_FEATURE_SELECT), 1)
            .map_err(describe)?;
        device.high = bar
            .read_u32(common(caps, DEVICE_FEATURE))
            .map_err(describe)?;
        if !device.has(32) {
            return Err("device does not offer VIRTIO_F_VERSION_1".to_string());
        }
        let mut driver = Features {
            low: 0,
            high: F_VERSION_1,
        };
        if device.has(5) {
            driver.low |= BLK_F_RO;
        }
        if device.has(9) {
            driver.low |= BLK_F_FLUSH;
        }
        bar.write_u32(common(caps, DRIVER_FEATURE_SELECT), 0)
            .map_err(describe)?;
        bar.write_u32(common(caps, DRIVER_FEATURE), driver.low)
            .map_err(describe)?;
        bar.write_u32(common(caps, DRIVER_FEATURE_SELECT), 1)
            .map_err(describe)?;
        bar.write_u32(common(caps, DRIVER_FEATURE), driver.high)
            .map_err(describe)?;
        write_status(
            bar,
            caps,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK,
        )?;
        if bar.read_u8(common(caps, DEVICE_STATUS)).map_err(describe)? & STATUS_FEATURES_OK == 0 {
            return Err("device rejected the feature set".to_string());
        }
        Ok(driver)
    }

    /// The `disk:` handler: `raven-proto-block` framing over the request queue.
    struct DiskHandler {
        device: VirtioBlk,
        opens: OpenTable<()>,
        pending: Option<(u64, String)>,
        test: bool,
        shutdown: bool,
        closed: u32,
    }

    impl DiskHandler {
        fn new(device: VirtioBlk, test: bool) -> Self {
            Self {
                device,
                opens: OpenTable::new(),
                pending: None,
                test,
                shutdown: false,
                closed: 0,
            }
        }

        fn shutdown_requested(&self) -> bool {
            self.shutdown
        }

        /// Number of closed open-file descriptions (the readiness probe's open is the first).
        fn closed_opens(&self) -> u32 {
            self.closed
        }

        fn expect_open(&mut self, file_id: u64, path: &str) {
            self.pending = Some((file_id, path.to_string()));
        }

        fn reply_ok(&self, response: &mut [u8]) -> HandlerResult<usize> {
            encode_reply(response, 0, &[]).map_err(|_| Errno::EPROTO.into())
        }

        fn reply_error(&self, response: &mut [u8], error: Errno) -> HandlerResult<usize> {
            encode_reply(response, -i32::from(error.0), &[]).map_err(|_| Errno::EPROTO.into())
        }
    }

    fn request_error(error: RequestError) -> Errno {
        match error {
            RequestError::ReadOnly => Errno::EROFS,
            RequestError::BadOp | RequestError::BadRange => Errno::EINVAL,
        }
    }

    impl Handler for DiskHandler {
        fn open_with(
            &mut self,
            path: &str,
            flags: OpenFlags,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            let Some((file_id, expected)) = self.pending.take() else {
                return Err(Errno::EINVAL.into());
            };
            if expected != path || (!path.is_empty() && path != "0") {
                return Err(Errno::ENOENT.into());
            }
            if !matches!(
                flags,
                OpenFlags::READ | OpenFlags::WRITE | OpenFlags::READ_WRITE
            ) {
                return Err(Errno::EINVAL.into());
            }
            if self.device.read_only && flags.writable() {
                return Err(Errno::EROFS.into());
            }
            if self.opens.len() >= 16 {
                return Err(Errno::EMFILE.into());
            }
            self.opens.insert_with_id(file_id, (), flags)?;
            Ok(Opened::File(file_id))
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.opens.close(file)?;
            self.closed = self.closed.saturating_add(1);
            Ok(())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            Ok(self.opens.dup(file)?)
        }

        fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
            let flags = self.opens.get(file).ok_or(Errno::EBADF)?.flags;
            let decoded = match WireRequest::decode(request) {
                Ok(decoded) => decoded,
                Err(_) => return self.reply_error(response, Errno::EINVAL),
            };
            let info = self.device.info();
            match decoded.op {
                WireOp::Read => {
                    if !flags.readable() {
                        return self.reply_error(response, Errno::EBADF);
                    }
                    let bytes = match validate_request(
                        BLK_T_IN,
                        decoded.sector,
                        decoded.count,
                        info.sectors,
                        false,
                    ) {
                        Ok(bytes) => bytes,
                        Err(error) => return self.reply_error(response, request_error(error)),
                    };
                    let total = REPLY_HEADER_LEN + bytes;
                    if response.len() < total {
                        return self.reply_error(response, Errno::EINVAL);
                    }
                    let target = response
                        .get_mut(REPLY_HEADER_LEN..total)
                        .ok_or(Errno::EPROTO)?;
                    if let Err(error) = self.device.read_sectors(decoded.sector, target) {
                        return self.reply_error(response, error);
                    }
                    write_reply_header(response, 0, bytes).map_err(|_| Errno::EPROTO)?;
                    Ok(total)
                }
                WireOp::Write => {
                    if !flags.writable() {
                        return self.reply_error(response, Errno::EBADF);
                    }
                    if let Err(error) = validate_request(
                        BLK_T_OUT,
                        decoded.sector,
                        decoded.count,
                        info.sectors,
                        info.read_only != 0,
                    ) {
                        return self.reply_error(response, request_error(error));
                    }
                    if let Err(error) = self.device.write_sectors(decoded.sector, decoded.data) {
                        return self.reply_error(response, error);
                    }
                    self.reply_ok(response)
                }
                WireOp::Flush => {
                    if let Err(error) = self.device.flush() {
                        return self.reply_error(response, error);
                    }
                    if self.test {
                        self.shutdown = true;
                    }
                    self.reply_ok(response)
                }
                WireOp::Info => {
                    let mut payload = [0u8; INFO_LEN];
                    if info.encode(&mut payload).is_err() {
                        return self.reply_error(response, Errno::EPROTO);
                    }
                    encode_reply(response, 0, &payload).map_err(|_| Errno::EPROTO.into())
                }
                WireOp::List => self.reply_error(response, Errno::EINVAL),
            }
        }
    }

    /// One qtest pattern cycle: write/verify (first launch) or verify (second launch).
    /// Cycle 1: write the partition and data patterns, flush, sample-read the data area and record
    /// the marker. The full read-back is cycle 2, after the device and process restarted.
    fn pattern_first(device: &mut VirtioBlk) -> Result<(), String> {
        if device.read_only {
            return Err("qtest disk is read-only".to_string());
        }
        if device.capacity <= DATA_FIRST + CHUNK_SECTORS as u64 {
            return Err(format!("disk capacity {} is too small", device.capacity));
        }
        write_partition(device)?;
        write_data(device)?;
        device
            .flush()
            .map_err(|error| format!("pattern flush: {}", error.description()))?;
        raven_rt::println!("virtio-blk: pattern flushed");
        let checksum = read_sample(device)?;
        let marker = marker_encode(checksum);
        device
            .write_sectors(MARKER_LBA, &marker)
            .map_err(|error| format!("marker write: {}", error.description()))?;
        device
            .flush()
            .map_err(|error| format!("marker flush: {}", error.description()))?;
        raven_rt::println!(
            "virtio-blk: pattern sectors={} checksum={checksum:#018x}",
            device.capacity.saturating_sub(DATA_FIRST)
        );
        raven_rt::println!("virtio-blk: marker ok");
        Ok(())
    }

    /// Cycle 2: verify the persisted partition and data patterns against the stored checksum.
    fn verify_cycle_two(device: &mut VirtioBlk, stored: u64) -> Result<(), String> {
        if device.read_only {
            return Err("qtest disk is read-only".to_string());
        }
        let checksum = verify_data(device)?;
        verify_partition(device)?;
        if checksum != stored {
            return Err(format!(
                "cycle 2 checksum {checksum:#018x} != stored {stored:#018x}"
            ));
        }
        raven_rt::println!("virtio-blk: cycle 2 data ok checksum={checksum:#018x}");
        Ok(())
    }

    fn write_partition(device: &mut VirtioBlk) -> Result<(), String> {
        let mut chunk = vec![0u8; CHUNK_BYTES];
        let mut lba = PARTITION_FIRST;
        while lba <= PARTITION_LAST {
            let sectors = (PARTITION_LAST - lba + 1).min(CHUNK_SECTORS as u64) as usize;
            let bytes = sectors * SECTOR_SIZE as usize;
            fill_pattern(lba, PARTITION_FILL, &mut chunk[..bytes]);
            device
                .write_sectors(lba, &chunk[..bytes])
                .map_err(|error| format!("partition write: {}", error.description()))?;
            lba += sectors as u64;
        }
        Ok(())
    }

    fn verify_partition(device: &mut VirtioBlk) -> Result<(), String> {
        let mut chunk = vec![0u8; CHUNK_BYTES];
        let mut lba = PARTITION_FIRST;
        while lba <= PARTITION_LAST {
            let sectors = (PARTITION_LAST - lba + 1).min(CHUNK_SECTORS as u64) as usize;
            let bytes = sectors * SECTOR_SIZE as usize;
            device
                .read_sectors(lba, &mut chunk[..bytes])
                .map_err(|error| format!("partition read: {}", error.description()))?;
            if !verify_pattern(lba, PARTITION_FILL, &chunk[..bytes]) {
                return Err(format!("partition pattern mismatch at sector {lba}"));
            }
            lba += sectors as u64;
        }
        raven_rt::println!(
            "virtio-blk: partition pattern ok sectors={}",
            PARTITION_LAST - PARTITION_FIRST + 1
        );
        Ok(())
    }

    fn write_data(device: &mut VirtioBlk) -> Result<(), String> {
        let mut chunk = vec![0u8; CHUNK_BYTES];
        let mut lba = DATA_FIRST;
        while lba < device.capacity {
            let sectors = (device.capacity - lba).min(CHUNK_SECTORS as u64) as usize;
            let bytes = sectors * SECTOR_SIZE as usize;
            fill_pattern(lba, DATA_FILL, &mut chunk[..bytes]);
            device
                .write_sectors(lba, &chunk[..bytes])
                .map_err(|error| format!("pattern write: {}", error.description()))?;
            lba += sectors as u64;
        }
        Ok(())
    }

    /// End of the sampled region whose checksum is stored in the marker (1 MiB).
    fn sample_end(device: &VirtioBlk) -> u64 {
        DATA_FIRST
            .saturating_add(SAMPLE_SECTORS)
            .min(device.capacity)
    }

    /// Cycle 1's post-flush sample read: verify and hash the first MiB of the data area.
    fn read_sample(device: &mut VirtioBlk) -> Result<u64, String> {
        let mut chunk = vec![0u8; CHUNK_BYTES];
        let mut hasher = Hasher::new();
        let mut lba = DATA_FIRST;
        let end = sample_end(device);
        while lba < end {
            let sectors = (end - lba).min(CHUNK_SECTORS as u64) as usize;
            let bytes = sectors * SECTOR_SIZE as usize;
            device
                .read_sectors(lba, &mut chunk[..bytes])
                .map_err(|error| format!("sample read: {}", error.description()))?;
            if !verify_pattern(lba, DATA_FILL, &chunk[..bytes]) {
                return Err(format!("sample pattern mismatch at sector {lba}"));
            }
            hasher.update(&chunk[..bytes]);
            lba += sectors as u64;
        }
        Ok(hasher.finish())
    }

    /// Cycle 2: verify the whole data area byte for byte and hash the sampled region.
    fn verify_data(device: &mut VirtioBlk) -> Result<u64, String> {
        let mut chunk = vec![0u8; CHUNK_BYTES];
        let mut hasher = Hasher::new();
        let mut lba = DATA_FIRST;
        let end = sample_end(device);
        while lba < device.capacity {
            let sectors = (device.capacity - lba).min(CHUNK_SECTORS as u64) as usize;
            let bytes = sectors * SECTOR_SIZE as usize;
            device
                .read_sectors(lba, &mut chunk[..bytes])
                .map_err(|error| format!("cycle 2 read: {}", error.description()))?;
            if !verify_pattern(lba, DATA_FILL, &chunk[..bytes]) {
                return Err(format!("cycle 2 pattern mismatch at sector {lba}"));
            }
            if lba < end {
                let sample = ((end - lba).min(sectors as u64) as usize) * SECTOR_SIZE as usize;
                hasher.update(&chunk[..sample]);
            }
            lba += sectors as u64;
        }
        Ok(hasher.finish())
    }

    fn describe(error: AccessError) -> String {
        match error {
            AccessError::Bounds => "device access out of bounds".to_string(),
            AccessError::Alignment => "misaligned device access".to_string(),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
