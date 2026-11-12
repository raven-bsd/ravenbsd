// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! virtio-net userspace driver (B13b).
//!
//! `devd` launches this program with `bar=4` and `resources=dma+irq`, so the child receives the
//! modern virtio-pci BAR (handle 3), the function's configuration page (4), a 64 KiB DMA buffer
//! (5) and the legacy IRQ (6). The driver finds the four virtio-pci capabilities, negotiates
//! `VIRTIO_F_VERSION_1`, `VIRTIO_NET_F_MAC`, `VIRTIO_NET_F_MTU` and `VIRTIO_NET_F_STATUS`, sets up
//! split RX/TX queues in the DMA buffer, sends one ARP request and waits for the reply.
//!
//! The frame path is zero-copy where the interface allows it: descriptors point straight into the
//! DMA buffer, and RX frames are parsed in place. Every index and length is bounded before use, so
//! a malformed used-ring entry or a short frame cannot panic or reach outside the buffer.
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

    use raven_abi::types::Handle;
    use raven_driver::{AccessError, ConfigRegion, DmaRegion, Irq, MmioRegion};
    use raven_rt::entry;
    use virtio_net::common::{
        DEVICE_FEATURE, DEVICE_FEATURE_SELECT, DEVICE_STATUS, DRIVER_FEATURE,
        DRIVER_FEATURE_SELECT, NUM_QUEUES, QUEUE_AVAIL, QUEUE_DESC, QUEUE_ENABLE,
        QUEUE_MSIX_VECTOR, QUEUE_NOTIFY_OFF, QUEUE_SELECT, QUEUE_SIZE, QUEUE_USED,
    };
    use virtio_net::{
        ArpReply, CAP_COMMON_CFG, CAP_DEVICE_CFG, CAP_ISR_CFG, CAP_NOTIFY_CFG, DESC_F_WRITE,
        Descriptor, F_VERSION_1, Features, MAX_FRAME_LEN, MacAddress, NET_F_MAC, NET_F_MTU,
        NET_F_STATUS, NET_HEADER_LEN, NetHeader, RingLayout, STATUS_ACKNOWLEDGE, STATUS_DRIVER,
        STATUS_DRIVER_OK, STATUS_FEATURES_OK, build_arp_request, decode_used, encode_descriptor,
        queue_size,
    };

    entry!(main);

    /// A fresh child has 0 (self), 1 (initfs), 2 (namespace); `devd` grants in order.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);

    /// The driver table matches `1af4:1041` (modern virtio-net).
    const VIRTIO_VENDOR: u16 = 0x1AF4;
    const VIRTIO_NET_DEVICE: u16 = 0x1041;
    /// Legacy INTx line that never wakes the driver up.
    const IRQ_WAIT_NS: u64 = 1_000_000_000;

    /// Where the two queues and the frame buffers live in the 64 KiB DMA buffer.
    const RX_RING_BASE: usize = 0x0000;
    const TX_RING_BASE: usize = 0x0100;
    const BUFFER_BASE: usize = 0x1000;
    /// One buffer per queue entry, large enough for a virtio header plus a maximum frame.
    const BUFFER_LEN: usize = 2048;
    /// The qtest's ARP request: a 42-byte Ethernet II frame.
    const ARP_LEN: usize = 42;
    /// Guest address used in the request; QEMU's user-mode network answers for the gateway.
    const GUEST_IP: [u8; 4] = [10, 0, 2, 15];
    const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("virtio-net: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let mut bar_index = 0usize;
        let mut bar_size = 0usize;
        let mut dma_size = 0usize;
        let mut irq_line = 0u32;
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
            }
        }
        if bar_size == 0 || dma_size < BUFFER_BASE + BUFFER_LEN {
            return Err("devd did not pass a usable BAR and DMA buffer".to_string());
        }

        let bar_window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size)
            .map_err(|error| format!("bar map: {}", error.description()))?;
        let mut bar = MmioRegion::new(bar_window);
        let config_window = raven_rt::map_mmio(CONFIG_HANDLE, raven_driver::PAGE)
            .map_err(|error| format!("config map: {}", error.description()))?;
        let config = ConfigRegion::new(config_window);
        let (vendor, device) = config.ids().map_err(describe)?;
        if vendor != VIRTIO_VENDOR || device != VIRTIO_NET_DEVICE {
            return Err(format!(
                "unexpected function {vendor:#06x}:{device:#06x}, expected 1af4:1041"
            ));
        }
        raven_rt::println!("virtio-net: id={vendor:04x}:{device:04x}");

        let caps = read_capabilities(&config, bar_index, bar_size)?;
        raven_rt::println!(
            "virtio-net: common={:#x} notify={:#x} isr={:#x} device={:#x}",
            caps.common.offset,
            caps.notify.offset,
            caps.isr.offset,
            caps.device.offset
        );

        let info = raven_rt::dma_info(DMA_HANDLE)
            .map_err(|error| format!("dma info: {}", error.description()))?;
        let dma_window = raven_rt::map_mmio_mut(DMA_HANDLE, dma_size)
            .map_err(|error| format!("dma map: {}", error.description()))?;
        let mut dma = DmaRegion::new(dma_window, info.bus_addr);
        raven_rt::println!(
            "virtio-net: dma len={} bus={:#x} irq={}",
            dma.len(),
            dma.bus_address(),
            irq_line
        );

        let features = reset_and_negotiate(&mut bar, &caps)?;
        let mac = read_mac(&bar, &caps, &features)?;
        let (link_up, mtu) = read_status(&bar, &caps, &features)?;
        raven_rt::println!(
            "virtio-net: mac={} mtu={} link={}",
            mac,
            mtu,
            if link_up { "up" } else { "down" }
        );

        // Both queues are provisioned before DRIVER_OK below.
        let (qsize, rx_layout, tx_layout, rx_notify, tx_notify) =
            setup_queues(&mut bar, &caps, &dma)?;
        // The device is live from here on.
        write_status(
            &mut bar,
            &caps,
            STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
        )?;

        let rx_buffers = prepare_rx(&mut dma, &rx_layout, qsize)?;
        notify(&mut bar, &caps, rx_notify, 0)?;

        // One ARP request, written into the TX buffer after a zero virtio-net header.
        let tx_buffer = BUFFER_BASE + usize::from(qsize) * BUFFER_LEN;
        dma.write_bytes(tx_buffer, &NetHeader::default().encode())
            .map_err(describe)?;
        let mut arp = [0u8; ARP_LEN];
        build_arp_request(mac, GUEST_IP, GATEWAY_IP, &mut arp)
            .ok_or_else(|| "ARP frame did not fit".to_string())?;
        dma.write_bytes(tx_buffer + NET_HEADER_LEN, &arp)
            .map_err(describe)?;
        let descriptor = Descriptor {
            addr: dma.bus_address() + tx_buffer as u64,
            len: (NET_HEADER_LEN + ARP_LEN) as u32,
            flags: 0,
            next: 0,
        };
        dma.write_bytes(tx_layout.desc, &encode_descriptor(descriptor))
            .map_err(describe)?;
        write_avail(&mut dma, &tx_layout, 1, &[0])?;
        dma.publish();
        notify(&mut bar, &caps, tx_notify, 1)?;
        raven_rt::println!("virtio-net: tx frame {ARP_LEN} bytes");

        let irq = Irq::new(IRQ_HANDLE);
        let mut tx_done = false;
        let mut tx_seen = 0u16;
        let mut rx_seen = 0u16;
        let mut reply = None;
        let mut rounds = 0usize;
        while rounds < 8 && reply.is_none() {
            let fired = irq
                .wait(IRQ_WAIT_NS)
                .map_err(|error| format!("irq wait: {}", error.description()))?;
            if fired == 0 {
                return Err("interrupt timed out".to_string());
            }
            let isr = bar.read_u8(caps.isr.offset).map_err(describe)?;
            let _ = isr; // reading the ISR byte acknowledges the device
            irq.ack()
                .map_err(|error| format!("irq ack: {}", error.description()))?;
            dma.acquire();

            let used = dma.read_u16(tx_layout.used_idx()).map_err(describe)?;
            if used != tx_seen {
                tx_seen = used;
                tx_done = true;
            }

            let used = dma.read_u16(rx_layout.used_idx()).map_err(describe)?;
            while rx_seen != used {
                let slot = rx_seen % qsize;
                let offset = rx_layout
                    .used_slot(slot)
                    .ok_or_else(|| "used slot out of range".to_string())?;
                let mut entry = [0u8; 8];
                dma.read_bytes(offset, &mut entry).map_err(describe)?;
                let used_entry =
                    decode_used(&entry).ok_or_else(|| "short used entry".to_string())?;
                rx_seen = rx_seen.wrapping_add(1);
                let id =
                    usize::try_from(used_entry.id).map_err(|_| "used descriptor id".to_string())?;
                if id >= usize::from(qsize) {
                    return Err(format!("used descriptor {id} outside the queue"));
                }
                let length =
                    usize::try_from(used_entry.len).map_err(|_| "used length".to_string())?;
                if !(NET_HEADER_LEN..=BUFFER_LEN).contains(&length) {
                    return Err(format!("received length {length} outside the buffer"));
                }
                let mut frame = [0u8; MAX_FRAME_LEN];
                let payload = length - NET_HEADER_LEN;
                if payload > frame.len() {
                    return Err(format!("received frame {payload} bytes is too long"));
                }
                let buffer = *rx_buffers.get(id).unwrap_or(&0);
                let mut header = [0u8; NET_HEADER_LEN];
                dma.read_bytes(buffer, &mut header).map_err(describe)?;
                NetHeader::decode(&header).ok_or_else(|| "short virtio header".to_string())?;
                dma.read_bytes(buffer + NET_HEADER_LEN, &mut frame[..payload])
                    .map_err(describe)?;
                if let Some(parsed) = virtio_net::parse_arp_reply(&frame[..payload]) {
                    reply = Some(parsed);
                    raven_rt::println!("virtio-net: rx frame {payload} bytes");
                    break;
                }
            }
            rounds += 1;
        }
        if !tx_done {
            return Err("TX descriptor never completed".to_string());
        }
        let reply: ArpReply = reply.ok_or_else(|| "no ARP reply".to_string())?;
        raven_rt::println!(
            "virtio-net: arp reply from {} ({}.{}.{}.{})",
            reply.sender_mac,
            reply.sender_ip[0],
            reply.sender_ip[1],
            reply.sender_ip[2],
            reply.sender_ip[3]
        );

        // Teardown: reset the device, then release the line. The DMA buffer is owned by devd and
        // closed with the process, and devd closes its own copies after the wait.
        write_status(&mut bar, &caps, 0)?;
        let _ = irq.ack();
        raven_rt::println!("virtio-net: done");
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
        // QEMU prepends each capability it adds, so the list runs from the highest offset
        // downwards. Walk it with a visited list instead of assuming increasing offsets; the
        // walk is bounded by MAX_CAPABILITIES and stops on a repeat.
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
                // Type 5 is the optional PCI-configuration window (bar 0 by definition); it is
                // not needed when the driver already holds the BAR and config page.
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
                    if length < 12 {
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
        if !device.has(5) {
            return Err("device does not offer a MAC address".to_string());
        }

        let mut driver = Features {
            low: NET_F_MAC,
            high: F_VERSION_1,
        };
        if device.has(3) {
            driver.low |= NET_F_MTU;
        }
        if device.has(16) {
            driver.low |= NET_F_STATUS;
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
        let status = bar.read_u8(common(caps, DEVICE_STATUS)).map_err(describe)?;
        if status & STATUS_FEATURES_OK == 0 {
            return Err("device rejected the feature set".to_string());
        }
        Ok(driver)
    }

    fn read_mac(bar: &MmioRegion, caps: &Caps, features: &Features) -> Result<MacAddress, String> {
        if !features.has(5) {
            return Err("MAC feature was not negotiated".to_string());
        }
        let base = caps.device.offset;
        let mut mac = [0u8; 6];
        for (index, byte) in mac.iter_mut().enumerate() {
            *byte = bar.read_u8(base + index).map_err(describe)?;
        }
        let mac = MacAddress(mac);
        if mac.0 == [0u8; 6] || mac.0 == [0xFF; 6] {
            return Err(format!("device returned an unusable MAC {mac}"));
        }
        Ok(mac)
    }

    fn read_status(
        bar: &MmioRegion,
        caps: &Caps,
        features: &Features,
    ) -> Result<(bool, u16), String> {
        let base = caps.device.offset;
        let mut link_up = true;
        if features.has(16) {
            let status = bar.read_u16(base + 6).map_err(describe)?;
            link_up = status & 1 != 0;
        }
        let mtu = if features.has(3) {
            bar.read_u16(base + 10).map_err(describe)?
        } else {
            1500
        };
        Ok((link_up, mtu))
    }

    fn queue_maximum(bar: &mut MmioRegion, caps: &Caps, queue: u16) -> Result<u16, String> {
        bar.write_u16(common(caps, QUEUE_SELECT), queue)
            .map_err(describe)?;
        bar.read_u16(common(caps, QUEUE_SIZE)).map_err(describe)
    }

    fn setup_queues(
        bar: &mut MmioRegion,
        caps: &Caps,
        dma: &DmaRegion,
    ) -> Result<(u16, RingLayout, RingLayout, usize, usize), String> {
        let queues = bar.read_u16(common(caps, NUM_QUEUES)).map_err(describe)?;
        if queues < 2 {
            return Err(format!("device offers {queues} queues, need 2"));
        }
        let maximum = queue_maximum(bar, caps, 0)?;
        let size = queue_size(maximum)
            .ok_or_else(|| format!("device queue size {maximum} is too small"))?;
        let rx = RingLayout::new(RX_RING_BASE, size);
        let tx = RingLayout::new(TX_RING_BASE, size);
        let tx_buffer = BUFFER_BASE + usize::from(size) * BUFFER_LEN;
        if tx.end > dma.len() || tx_buffer + BUFFER_LEN > dma.len() {
            return Err("DMA buffer is too small for the queues".to_string());
        }
        let rx_notify = program_queue(bar, caps, 0, &rx, dma.bus_address())?;
        let tx_notify = program_queue(bar, caps, 1, &tx, dma.bus_address())?;
        raven_rt::println!("virtio-net: queue0 rx size={size}");
        raven_rt::println!("virtio-net: queue1 tx size={size}");
        Ok((size, rx, tx, rx_notify, tx_notify))
    }

    fn program_queue(
        bar: &mut MmioRegion,
        caps: &Caps,
        queue: u16,
        layout: &RingLayout,
        bus: u64,
    ) -> Result<usize, String> {
        bar.write_u16(common(caps, QUEUE_SELECT), queue)
            .map_err(describe)?;
        bar.write_u16(common(caps, QUEUE_SIZE), layout.size)
            .map_err(describe)?;
        bar.write_u16(common(caps, QUEUE_MSIX_VECTOR), 0xFFFF)
            .map_err(describe)?;
        bar.write_u64(common(caps, QUEUE_DESC), bus + layout.desc as u64)
            .map_err(describe)?;
        bar.write_u64(common(caps, QUEUE_AVAIL), bus + layout.avail as u64)
            .map_err(describe)?;
        bar.write_u64(common(caps, QUEUE_USED), bus + layout.used as u64)
            .map_err(describe)?;
        bar.write_u16(common(caps, QUEUE_ENABLE), 1)
            .map_err(describe)?;
        if bar.read_u16(common(caps, QUEUE_ENABLE)).map_err(describe)? != 1 {
            return Err(format!("queue {queue} did not enable"));
        }
        let notify = bar
            .read_u16(common(caps, QUEUE_NOTIFY_OFF))
            .map_err(describe)?;
        Ok(usize::from(notify) * caps.notify_multiplier as usize)
    }

    fn notify(
        bar: &mut MmioRegion,
        caps: &Caps,
        notify_offset: usize,
        queue: u16,
    ) -> Result<(), String> {
        let address = caps.notify.offset + notify_offset;
        let end = address
            .checked_add(2)
            .ok_or_else(|| "notify offset".to_string())?;
        if end > caps.notify.offset + caps.notify.length || end > bar.len() {
            return Err("notify address is outside the capability".to_string());
        }
        bar.write_u16(address, queue).map_err(describe)
    }

    fn write_avail(
        dma: &mut DmaRegion,
        layout: &RingLayout,
        count: u16,
        entries: &[u16],
    ) -> Result<(), String> {
        dma.write_u16(layout.avail, 0).map_err(describe)?;
        dma.write_u16(layout.avail_idx(), count).map_err(describe)?;
        for (index, entry) in entries.iter().enumerate() {
            let slot = layout
                .avail_slot(u16::try_from(index).map_err(|_| "avail index".to_string())?)
                .ok_or_else(|| "avail slot out of range".to_string())?;
            dma.write_u16(slot, *entry).map_err(describe)?;
        }
        Ok(())
    }

    fn prepare_rx(
        dma: &mut DmaRegion,
        layout: &RingLayout,
        qsize: u16,
    ) -> Result<alloc::vec::Vec<usize>, String> {
        let mut buffers = alloc::vec::Vec::with_capacity(usize::from(qsize));
        for index in 0..qsize {
            let offset = BUFFER_BASE + usize::from(index) * BUFFER_LEN;
            buffers.push(offset);
            let descriptor = Descriptor {
                addr: dma.bus_address() + offset as u64,
                len: BUFFER_LEN as u32,
                flags: DESC_F_WRITE,
                next: 0,
            };
            let address = layout.desc + usize::from(index) * 16;
            dma.write_bytes(address, &encode_descriptor(descriptor))
                .map_err(describe)?;
        }
        let mut entries = alloc::vec::Vec::with_capacity(usize::from(qsize));
        for index in 0..qsize {
            entries.push(index);
        }
        write_avail(dma, layout, qsize, &entries)?;
        Ok(buffers)
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
