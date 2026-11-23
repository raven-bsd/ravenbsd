// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! virtio-net userspace driver (B12b): a persistent frame service.
//!
//! `devd` launches this program with `bar=4` and `resources=dma+irq`, so the child receives the
//! modern virtio-pci BAR (3), the configuration page (4), a 64 KiB DMA buffer (5) and the legacy
//! IRQ (6).  The driver negotiates VERSION_1/MAC/MTU/STATUS, programs the split RX/TX queues and
//! runs a persistent frame service:
//!
//! * `nic:info` - MAC, MTU, link, ring offset/length/counts/pool and DMA length;
//! * `nic:mac`, `nic:mtu`, `nic:link` - one line each;
//! * `nic:ring` - an attachment of the DMA buffer; the consumer maps it and slices
//!   `ring_offset..ring_offset+ring_len` (the B39 `raven-proto-net::ring` region);
//! * `nic:start` / `nic:stop` - arm or pause the pump (write one byte);
//! * `nic:quit` - test-only clean shutdown (the production driver serves until the machine stops).
//!
//! The B39 region lives at `RING_REGION_OFFSET` in the DMA buffer: header, 8 RX descriptors, 8 TX
//! descriptors and an 11-buffer pool.  RX pool buffers 0..7 are the device receive buffers; the
//! virtio-net header lands before the frame, so the B39 RX descriptor points past it.  A TX
//! descriptor names a pool buffer holding an Ethernet frame; the driver submits a two-descriptor
//! chain (a 12-byte zero virtio header then the frame) and only advances `tx_cons` after the
//! device completed the frame.
//!
//! A pump thread owns the device: it waits on the ISR/IRQ with a 10 ms bound, drains RX used
//! entries into the RX ring, re-arms buffers the consumer retired and consumes TX descriptors
//! after completion.  Every index, length, buffer offset and frame flag is checked; a timeout or
//! a malformed ring resets the device and reports `EIO`.
//!
//! It keeps `#![forbid(unsafe_code)]`.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::collections::BTreeMap;
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};

    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_driver::{AccessError, Capability, DmaRegion, Irq, MmioRegion};
    use raven_proto_fs::{OpenFlags, S_IFREG, Stat};
    use raven_proto_net::ring::{
        BUFFER_SIZE, Ring, RingConsumer, RingHeader, RingLayout as B39Layout, RingProducer,
    };
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_rt::thread;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};
    use virtio_net::common::{
        DEVICE_FEATURE, DEVICE_FEATURE_SELECT, DEVICE_STATUS, DRIVER_FEATURE,
        DRIVER_FEATURE_SELECT, MSIX_CONFIG, NUM_QUEUES, QUEUE_AVAIL, QUEUE_DESC, QUEUE_ENABLE,
        QUEUE_MSIX_VECTOR, QUEUE_NOTIFY_OFF, QUEUE_SELECT, QUEUE_SIZE, QUEUE_USED,
    };
    use virtio_net::mmio;
    use virtio_net::{
        CAP_COMMON_CFG, CAP_DEVICE_CFG, CAP_ISR_CFG, CAP_NOTIFY_CFG, ControlInfo, DESC_F_WRITE,
        Descriptor, F_VERSION_1, Features, MAX_FRAME_LEN, MacAddress, NET_F_MAC, NET_F_MTU,
        NET_F_STATUS, NET_HEADER_LEN, NIC_INFO, NIC_LINK, NIC_MAC, NIC_MTU, NIC_QUIT, NIC_RING,
        NIC_SCHEME, NIC_START, NIC_STOP, RING_REGION_LEN, RING_REGION_OFFSET, RING_RX_BUFFERS,
        RING_RX_COUNT, RING_TX_COUNT, RingLayout as SplitLayout, STATUS_ACKNOWLEDGE, STATUS_DRIVER,
        STATUS_DRIVER_OK, STATUS_FEATURES_OK, VIRTIO_NET_HEADER, build_arp_request, decode_used,
        encode_descriptor, frame_desc, parse_arp_reply, queue_size, ring_layout, rx_desc,
    };

    entry!(main);

    /// A fresh child has 0 (self), 1 (initfs), 2 (namespace); `devd` grants in order.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);
    const MSIX_BAR_HANDLE: Handle = Handle::new(7, 0);
    /// `devd dtb launch` grants one MMIO window, then DMA, then the IRQ (no PCI config page).
    const MMIO_DMA_HANDLE: Handle = Handle::new(4, 0);
    const MMIO_IRQ_HANDLE: Handle = Handle::new(5, 0);

    const VIRTIO_VENDOR: u16 = 0x1AF4;
    const VIRTIO_NET_DEVICE: u16 = 0x1041;
    /// Guest/gateway addresses the legacy self-test ARPs for (QEMU user networking).
    const GUEST_IP: [u8; 4] = [10, 0, 2, 15];
    const GATEWAY_IP: [u8; 4] = [10, 0, 2, 2];

    /// Split queues in the first 4 KiB of the DMA buffer.
    const RX_RING_BASE: usize = 0x0000;
    const TX_RING_BASE: usize = 0x0100;
    /// 12-byte zero virtio-net header for TX chains, below the B39 region.
    const TX_HEADER_OFFSET: usize = 0x0800;
    /// Bounded pump wait: the frame rings are checked at least every 1 ms.
    const IRQ_PUMP_NS: u64 = 1_000_000;

    /// MSI-X queue message and table location supplied by `devd`.
    struct MsixInfo {
        address_lo: u32,
        address_hi: u32,
        data: u32,
        bar: usize,
        bar_size: usize,
        table: usize,
    }

    static NIC: Mutex<Option<Nic>> = Mutex::new(None);
    static INFO: Mutex<Option<ControlInfo>> = Mutex::new(None);
    static STARTED: AtomicBool = AtomicBool::new(false);
    static QUIT: AtomicBool = AtomicBool::new(false);
    static PUMP_DONE: AtomicBool = AtomicBool::new(false);
    static LINK_UP: AtomicBool = AtomicBool::new(false);
    static RX_FRAMES: AtomicU64 = AtomicU64::new(0);
    static TX_FRAMES: AtomicU64 = AtomicU64::new(0);
    static TX_DROPS: AtomicU64 = AtomicU64::new(0);
    static TX_STALL: AtomicBool = AtomicBool::new(false);
    static IRQ_TX_LOGGED: AtomicBool = AtomicBool::new(false);
    static IRQ_PROGRESS_LOGGED: AtomicBool = AtomicBool::new(false);
    static IRQ_TOTAL: AtomicU64 = AtomicU64::new(0);

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
        let mut test = false;
        let mut serve = false;
        let mut mmio = false;
        let mut msix = false;
        let mut msix_addr_lo = None;
        let mut msix_addr_hi = None;
        let mut msix_data = None;
        let mut msix_bar = None;
        let mut msix_bar_size = None;
        let mut msix_table = None;
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
            } else if argument == "msix=1" || argument == "msix" {
                msix = true;
            } else if argument == "tx-stall" {
                // Test-only mutation: drop TX completions so the qtest must catch the wedge.
                TX_STALL.store(true, Ordering::Release);
            } else if let Some(value) = argument.strip_prefix("msix_addr_lo=") {
                msix_addr_lo = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_addr_lo argument `{argument}`"))?,
                );
            } else if let Some(value) = argument.strip_prefix("msix_addr_hi=") {
                msix_addr_hi = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_addr_hi argument `{argument}`"))?,
                );
            } else if let Some(value) = argument.strip_prefix("msix_data=") {
                msix_data = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_data argument `{argument}`"))?,
                );
            } else if let Some(value) = argument.strip_prefix("msix_bar=") {
                msix_bar = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_bar argument `{argument}`"))?,
                );
            } else if let Some(value) = argument.strip_prefix("msix_bar_size=") {
                msix_bar_size = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_bar_size argument `{argument}`"))?,
                );
            } else if let Some(value) = argument.strip_prefix("msix_table=") {
                msix_table = Some(
                    value
                        .parse()
                        .map_err(|_| format!("bad msix_table argument `{argument}`"))?,
                );
            } else if argument == "test" {
                test = true;
            } else if argument == "serve" {
                serve = true;
            } else if argument == "transport=mmio" {
                mmio = true;
            }
        }
        let msix = if msix {
            Some(MsixInfo {
                address_lo: msix_addr_lo
                    .ok_or_else(|| "msix requested without msix_addr_lo".to_string())?,
                address_hi: msix_addr_hi
                    .ok_or_else(|| "msix requested without msix_addr_hi".to_string())?,
                data: msix_data.ok_or_else(|| "msix requested without msix_data".to_string())?,
                bar: msix_bar.ok_or_else(|| "msix requested without msix_bar".to_string())?,
                bar_size: msix_bar_size
                    .ok_or_else(|| "msix requested without msix_bar_size".to_string())?,
                table: msix_table.ok_or_else(|| "msix requested without msix_table".to_string())?,
            })
        } else {
            None
        };
        if bar_size == 0 || dma_size < RING_REGION_OFFSET + RING_REGION_LEN {
            return Err("devd did not pass a usable BAR and DMA buffer".to_string());
        }

        // The virtio-mmio window is 0x200 bytes, but the kernel maps page-granular objects; map
        // the whole page the devd platform grant created (0x1000) and keep the register offsets.
        let bar_map_size = if mmio {
            bar_size.saturating_add(0xfff) & !0xfff
        } else {
            bar_size
        };
        let bar_window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_map_size)
            .map_err(|error| format!("bar map: {}", error.description()))?;
        let bar = MmioRegion::new(bar_window);
        let (config, dma_handle, irq_handle, msix_bar) = if mmio {
            // `devd dtb launch` grants one MMIO window, then DMA, then the IRQ.
            (None, MMIO_DMA_HANDLE, MMIO_IRQ_HANDLE, None)
        } else {
            let config_window = raven_rt::map_mmio_mut(CONFIG_HANDLE, raven_driver::PAGE)
                .map_err(|error| format!("config map: {}", error.description()))?;
            let config = MmioRegion::new(config_window);
            let msix_bar = match &msix {
                Some(info) => {
                    let window = raven_rt::map_mmio_mut(MSIX_BAR_HANDLE, info.bar_size)
                        .map_err(|error| format!("msix bar map: {}", error.description()))?;
                    Some(MmioRegion::new(window))
                }
                None => None,
            };
            (Some(config), DMA_HANDLE, IRQ_HANDLE, msix_bar)
        };
        let dma_info = raven_rt::dma_info(dma_handle)
            .map_err(|error| format!("dma info: {}", error.description()))?;
        let window = raven_rt::map_mmio_mut(dma_handle, dma_size)
            .map_err(|error| format!("dma map: {}", error.description()))?;
        // The split-queue metadata lives in the first 4 KiB; the B39 region and pool follow.
        let (meta, rest) = window.split_at_mut(RING_REGION_OFFSET);
        let (region, _tail) = rest.split_at_mut(RING_REGION_LEN);
        let dma = DmaRegion::new(meta, dma_info.bus_addr);
        let irq = Irq::new(irq_handle);

        let mut nic = Nic::open(
            bar, config, dma, region, irq, bar_index, bar_size, dma_size, irq_line, msix, msix_bar,
            mmio,
        )?;
        if !test && !serve {
            // B13b compatibility: the old one-shot qtest profile passes no mode token.
            return run_legacy(&mut nic);
        }
        let info = nic.info();
        *INFO.lock() = Some(info);
        LINK_UP.store(info.link, Ordering::Release);
        STARTED.store(false, Ordering::Release);
        QUIT.store(false, Ordering::Release);
        PUMP_DONE.store(false, Ordering::Release);
        {
            let mut guard = NIC.lock();
            *guard = Some(nic);
        }
        // K9 KVM2: the pump wake is bounded by the running thread's scheduler slice (measured
        // 10-14 ms late on a 1 ms wait), not by the requested timeout. Run the driver at the
        // highest nice so the woken pump preempts the current thread and drains TX before the
        // contract's half-close window.
        let _ = raven_rt::priority::setpriority(raven_rt::priority::PRIO_PROCESS, 0, -20);
        thread::spawn(pump_thread, 0, 128 * 1024)
            .map_err(|error| format!("pump thread: {}", error.description()))?;

        let mut handler = NicHandler::new(test, dma_handle);
        let mut server = match Server::register(NIC_SCHEME) {
            Ok(server) => server,
            Err(error) => {
                return Err(format!("register {}: {}", NIC_SCHEME, error.description()));
            }
        };
        raven_rt::println!("virtio-net: serving {NIC_SCHEME}:");
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
            if handler.should_quit() {
                break;
            }
        }
        QUIT.store(true, Ordering::Release);
        let mut spins = 0usize;
        while !PUMP_DONE.load(Ordering::Acquire) {
            spins += 1;
            if spins > 2000 {
                break;
            }
            let _ = raven_rt::nanosleep(1_000_000);
        }
        raven_rt::println!(
            "virtio-net: stats rx={} tx={} drops={}",
            RX_FRAMES.load(Ordering::Acquire),
            TX_FRAMES.load(Ordering::Acquire),
            TX_DROPS.load(Ordering::Acquire)
        );
        raven_rt::println!("virtio-net: serve done");
        Ok(())
    }

    /// The pump thread: wait on the ISR/IRQ and serve the frame rings.
    fn pump_thread(_: usize) {
        loop {
            if QUIT.load(Ordering::Acquire) {
                break;
            }
            let mut guard = NIC.lock();
            let Some(nic) = guard.as_mut() else {
                break;
            };
            if let Err(error) = nic.pump() {
                raven_rt::eprintln!("virtio-net: pump: {error}");
                QUIT.store(true, Ordering::Release);
            }
            drop(guard);
        }
        if let Some(mut nic) = NIC.lock().take() {
            let _ = nic.reset();
        }
        PUMP_DONE.store(true, Ordering::Release);
    }

    /// The four virtio-pci capabilities on the BAR the driver was given.
    struct Caps {
        mmio: bool,
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

    /// Split-queue state for one virtio queue.
    struct Queue {
        layout: SplitLayout,
        notify: usize,
        avail: u16,
        used: u16,
    }

    struct Nic {
        bar: MmioRegion,
        dma: DmaRegion,
        region: &'static mut [u8],
        irq: Irq,
        caps: Caps,
        rx: Queue,
        tx: Queue,
        qsize: u16,
        b39: B39Layout,
        bus: u64,
        region_bus: u64,
        mac: MacAddress,
        mtu: u16,
        features: Features,
        dma_len: usize,
        rx_armed: [bool; RING_RX_BUFFERS],
        rx_slot_buffer: [u8; RING_RX_BUFFERS],
        rx_prod: u32,
        rx_cons_seen: u32,
        tx_busy: bool,
        tx_waits: u32,
        last_fired: usize,
        last_isr: u8,
        broken: bool,
    }

    impl Nic {
        #[allow(clippy::too_many_arguments)] // five handles plus the BAR/DMA geometry
        fn open(
            mut bar: MmioRegion,
            config: Option<MmioRegion>,
            mut dma: DmaRegion,
            region: &'static mut [u8],
            irq: Irq,
            bar_index: usize,
            bar_size: usize,
            dma_len: usize,
            irq_line: u32,
            msix: Option<MsixInfo>,
            msix_bar: Option<MmioRegion>,
            is_mmio: bool,
        ) -> Result<Self, String> {
            let (caps, features, mac, link, mtu, msix_vector) = if is_mmio {
                if msix.is_some() {
                    return Err("MSI-X is not part of virtio-mmio".to_string());
                }
                let magic = bar.read_u32(mmio::MAGIC_VALUE).map_err(describe)?;
                let version = bar.read_u32(mmio::VERSION_REG).map_err(describe)?;
                let device = bar.read_u32(mmio::DEVICE_ID).map_err(describe)?;
                if magic != mmio::MAGIC || version != mmio::VERSION || device != mmio::DEVICE_ID_NET
                {
                    return Err(format!(
                        "unexpected virtio-mmio magic={magic:#010x} version={version} device={device}"
                    ));
                }
                raven_rt::println!("virtio-net: transport=mmio id={device} irq={irq_line}");
                let caps = Caps {
                    mmio: true,
                    common: Region {
                        offset: 0,
                        length: mmio::WINDOW_LEN,
                    },
                    notify: Region {
                        offset: mmio::QUEUE_NOTIFY,
                        length: 4,
                    },
                    isr: Region {
                        offset: mmio::INTERRUPT_STATUS,
                        length: 8,
                    },
                    device: Region {
                        offset: mmio::CONFIG,
                        length: 0x10,
                    },
                    notify_multiplier: 1,
                };
                raven_rt::println!(
                    "virtio-net: dma len={dma_len} bus={:#x} irq={irq_line}",
                    dma.bus_address()
                );
                let features = reset_and_negotiate(&mut bar, &caps)?;
                let mac = read_mac(&bar, &caps, &features)?;
                let (link, mtu) = read_status(&bar, &caps, &features)?;
                (caps, features, mac, link, mtu, 0xFFFF)
            } else {
                let mut config =
                    config.ok_or_else(|| "missing PCI configuration page".to_string())?;
                let vendor = config.read_u16(0x00).map_err(describe)?;
                let device = config.read_u16(0x02).map_err(describe)?;
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
                raven_rt::println!(
                    "virtio-net: dma len={dma_len} bus={:#x} irq={irq_line}",
                    dma.bus_address()
                );
                let features = reset_and_negotiate(&mut bar, &caps)?;
                let msix_vector = match (msix, msix_bar) {
                    (Some(info), Some(mut table)) => {
                        let cap = find_msix(&config)?.ok_or_else(|| {
                            "MSI-X was requested but the function has no MSI-X capability"
                                .to_string()
                        })?;
                        configure_msix(&mut table, &mut config, &cap, &info)?;
                        bar.write_u16(common(&caps, MSIX_CONFIG), 0)
                            .map_err(describe)?;
                        1
                    }
                    (Some(_), None) => {
                        return Err("MSI-X table BAR was not transferred".to_string());
                    }
                    (None, _) => 0xFFFF,
                };
                let mac = read_mac(&bar, &caps, &features)?;
                let (link, mtu) = read_status(&bar, &caps, &features)?;
                (caps, features, mac, link, mtu, msix_vector)
            };
            raven_rt::println!(
                "virtio-net: mac={mac} mtu={mtu} link={}",
                if link { "up" } else { "down" }
            );

            let b39 = ring_layout().map_err(|error| format!("ring layout: {error}"))?;
            if b39.buffer_count() < RING_RX_BUFFERS {
                return Err("ring pool is too small for the RX buffers".to_string());
            }
            b39.write_header(region, 0, 0)
                .map_err(|error| format!("ring header: {error}"))?;

            let queues = if is_mmio {
                2
            } else {
                bar.read_u16(common(&caps, NUM_QUEUES)).map_err(describe)?
            };
            if queues < 2 {
                return Err(format!("device offers {queues} queues, need 2"));
            }
            let maximum = if is_mmio {
                queue_maximum_mmio(&mut bar, 0)?
            } else {
                queue_maximum(&mut bar, &caps, 0)?
            };
            let qsize = queue_size(maximum)
                .ok_or_else(|| format!("device queue size {maximum} is too small"))?;
            let rx_layout = SplitLayout::new(RX_RING_BASE, qsize);
            let tx_layout = SplitLayout::new(TX_RING_BASE, qsize);
            if tx_layout.end > dma.len() || TX_HEADER_OFFSET + NET_HEADER_LEN > dma.len() {
                return Err("DMA buffer is too small for the split queues".to_string());
            }
            let bus = dma.bus_address();
            let (rx_notify, tx_notify) = if is_mmio {
                (
                    program_queue_mmio(&mut bar, 0, &rx_layout, bus)?,
                    program_queue_mmio(&mut bar, 1, &tx_layout, bus)?,
                )
            } else {
                (
                    program_queue(&mut bar, &caps, 0, &rx_layout, bus, msix_vector)?,
                    program_queue(&mut bar, &caps, 1, &tx_layout, bus, msix_vector)?,
                )
            };
            raven_rt::println!("virtio-net: queue0 rx size={qsize}");
            raven_rt::println!("virtio-net: queue1 tx size={qsize}");
            raven_rt::println!(
                "virtio-net: ring offset={:#x} len={} rx={} tx={} pool={} dma={}",
                RING_REGION_OFFSET,
                RING_REGION_LEN,
                RING_RX_COUNT,
                RING_TX_COUNT,
                b39.pool_len(),
                dma_len
            );

            // One RX descriptor per pool buffer; the device writes the virtio header first.
            for slot in 0..qsize {
                let Some(pool) = b39.buffer_offset(usize::from(slot)) else {
                    return Err("RX pool buffer".to_string());
                };
                let descriptor = Descriptor {
                    addr: bus
                        .saturating_add(RING_REGION_OFFSET as u64)
                        .saturating_add(pool as u64),
                    len: BUFFER_SIZE as u32,
                    flags: DESC_F_WRITE,
                    next: 0,
                };
                let address = rx_layout.desc + usize::from(slot) * 16;
                dma.write_bytes(address, &encode_descriptor(descriptor))
                    .map_err(describe)?;
            }
            // Fill the available ring with 0..qsize and publish it.
            dma.write_u16(rx_layout.avail, 0).map_err(describe)?;
            dma.write_u16(rx_layout.avail_idx(), qsize)
                .map_err(describe)?;
            for slot in 0..qsize {
                let ring_slot = rx_layout
                    .avail_slot(slot)
                    .ok_or_else(|| "RX avail slot".to_string())?;
                dma.write_u16(ring_slot, slot).map_err(describe)?;
            }
            dma.publish();

            let mut nic = Self {
                bar,
                dma,
                region,
                irq,
                caps,
                rx: Queue {
                    layout: rx_layout,
                    notify: rx_notify,
                    avail: qsize,
                    used: 0,
                },
                tx: Queue {
                    layout: tx_layout,
                    notify: tx_notify,
                    avail: 0,
                    used: 0,
                },
                qsize,
                b39,
                bus,
                region_bus: bus.saturating_add(RING_REGION_OFFSET as u64),
                mac,
                mtu,
                features,
                dma_len,
                rx_armed: [true; RING_RX_BUFFERS],
                rx_slot_buffer: [0; RING_RX_BUFFERS],
                rx_prod: 0,
                rx_cons_seen: 0,
                tx_busy: false,
                tx_waits: 0,
                last_fired: 0,
                last_isr: 0,
                broken: false,
            };
            // The queues are programmed; the device is live from here.
            write_status(
                &mut nic.bar,
                &nic.caps,
                STATUS_ACKNOWLEDGE | STATUS_DRIVER | STATUS_FEATURES_OK | STATUS_DRIVER_OK,
            )?;
            nic.notify_queue(nic.rx.notify, 0)?;
            Ok(nic)
        }

        fn info(&self) -> ControlInfo {
            ControlInfo {
                mac: self.mac,
                mtu: self.mtu,
                link: LINK_UP.load(Ordering::Acquire),
                dma_len: self.dma_len,
                ring_offset: RING_REGION_OFFSET,
                ring_len: RING_REGION_LEN,
                rx_count: RING_RX_COUNT,
                tx_count: RING_TX_COUNT,
                pool_len: self.b39.pool_len(),
            }
        }

        fn reset(&mut self) -> Result<(), String> {
            write_status(&mut self.bar, &self.caps, 0)?;
            let mut spins = 0;
            while read_device_status(&self.bar, &self.caps)? != 0 {
                spins += 1;
                if spins > 1000 {
                    return Err("device did not reset".to_string());
                }
                raven_rt::yield_now();
            }
            Ok(())
        }

        fn notify_queue(&mut self, notify_offset: usize, queue: u16) -> Result<(), String> {
            if self.caps.mmio {
                return self
                    .bar
                    .write_u32(mmio::QUEUE_NOTIFY, u32::from(queue))
                    .map_err(describe);
            }
            let address = self.caps.notify.offset + notify_offset;
            let end = address
                .checked_add(2)
                .ok_or_else(|| "notify address".to_string())?;
            if end > self.caps.notify.offset + self.caps.notify.length || end > self.bar.len() {
                return Err("notify address is outside the capability".to_string());
            }
            self.bar.write_u16(address, queue).map_err(describe)
        }

        fn push_avail(&mut self, rx: bool, entry: u16) -> Result<(), String> {
            let (layout, notify, queue) = if rx {
                (self.rx.layout, self.rx.notify, 0u16)
            } else {
                (self.tx.layout, self.tx.notify, 1u16)
            };
            let avail = if rx { self.rx.avail } else { self.tx.avail };
            let slot = avail % self.qsize;
            let ring_slot = layout
                .avail_slot(slot)
                .ok_or_else(|| "avail slot".to_string())?;
            self.dma.write_u16(ring_slot, entry).map_err(describe)?;
            let next = avail.wrapping_add(1);
            self.dma
                .write_u16(layout.avail_idx(), next)
                .map_err(describe)?;
            self.dma.publish();
            if rx {
                self.rx.avail = next;
            } else {
                self.tx.avail = next;
            }
            self.notify_queue(notify, queue)
        }

        /// Re-arm the device with the pool buffer of an RX descriptor after the consumer retired it.
        fn arm_rx(&mut self, buffer: usize) -> Result<(), String> {
            if buffer >= RING_RX_BUFFERS {
                return Err("RX buffer out of range".to_string());
            }
            self.push_avail(true, buffer as u16)?;
            if let Some(slot) = self.rx_armed.get_mut(buffer) {
                *slot = true;
            }
            Ok(())
        }

        fn update_link(&mut self) -> Result<(), String> {
            if self.features.has(16) {
                let status = self
                    .bar
                    .read_u16(self.caps.device.offset + 6)
                    .map_err(describe)?;
                LINK_UP.store(status & 1 != 0, Ordering::Release);
            }
            Ok(())
        }

        fn pump(&mut self) -> Result<(), String> {
            if self.broken {
                return Err("device is broken".to_string());
            }
            let fired = self.irq.wait(IRQ_PUMP_NS).unwrap_or(0);
            self.last_fired = fired;
            if fired > 0 {
                let isr = self.bar.read_u8(self.caps.isr.offset).map_err(describe)?;
                self.last_isr = isr;
                if self.caps.mmio {
                    self.bar
                        .write_u32(mmio::INTERRUPT_ACK, u32::from(isr))
                        .map_err(describe)?;
                }
                let _ = self.irq.ack();
                let tx = TX_FRAMES.load(Ordering::Acquire);
                if tx > 0 && !IRQ_TX_LOGGED.swap(true, Ordering::AcqRel) {
                    raven_rt::println!(
                        "virtio-net: irq tx fired={fired} isr={isr:#x} tx={tx} rx={}",
                        RX_FRAMES.load(Ordering::Acquire)
                    );
                }
                let total = IRQ_TOTAL.fetch_add(fired as u64, Ordering::AcqRel)
                    + u64::try_from(fired).unwrap_or(0);
                if total >= 24 && !IRQ_PROGRESS_LOGGED.swap(true, Ordering::AcqRel) {
                    raven_rt::println!(
                        "virtio-net: irq progress fired={total} tx={} rx={}",
                        TX_FRAMES.load(Ordering::Acquire),
                        RX_FRAMES.load(Ordering::Acquire)
                    );
                }
            }
            self.update_link()?;
            if !STARTED.load(Ordering::Acquire) {
                return Ok(());
            }
            self.drain_rx()?;
            self.retire_rx()?;
            self.pump_tx()?;
            Ok(())
        }

        fn fail(&mut self, message: String) -> Result<(), String> {
            self.broken = true;
            let _ = self.reset();
            Err(message)
        }

        fn drain_rx(&mut self) -> Result<(), String> {
            loop {
                let full = {
                    let producer = RingProducer::new(self.region, self.b39, Ring::Rx)
                        .map_err(|error| format!("RX ring: {error}"))?;
                    producer.is_full()
                };
                if full {
                    break;
                }
                self.dma.acquire();
                let used = self
                    .dma
                    .read_u16(self.rx.layout.used_idx())
                    .map_err(describe)?;
                if used == self.rx.used {
                    break;
                }
                let slot = self.rx.used % self.qsize;
                let offset = self
                    .rx
                    .layout
                    .used_slot(slot)
                    .ok_or_else(|| "RX used slot".to_string())?;
                let mut entry = [0u8; 8];
                self.dma.read_bytes(offset, &mut entry).map_err(describe)?;
                let used_entry = decode_used(&entry).ok_or("short RX used entry")?;
                self.rx.used = self.rx.used.wrapping_add(1);
                let id = usize::try_from(used_entry.id).map_err(|_| "RX descriptor id")?;
                let length = usize::try_from(used_entry.len).map_err(|_| "RX length")?;
                if id >= usize::from(self.qsize) {
                    return self.fail(format!("RX descriptor {id} outside the queue"));
                }
                if !(VIRTIO_NET_HEADER..=VIRTIO_NET_HEADER + MAX_FRAME_LEN).contains(&length) {
                    return self.fail(format!("RX length {length} outside the buffer"));
                }
                if !self.rx_armed.get(id).copied().unwrap_or(false) {
                    return self.fail(format!("RX descriptor {id} was not armed"));
                }
                if let Some(slot) = self.rx_armed.get_mut(id) {
                    *slot = false;
                }
                let frame_len = length - VIRTIO_NET_HEADER;
                let desc = rx_desc(&self.b39, id, frame_len)
                    .ok_or_else(|| "RX descriptor range".to_string())?;
                let producer_slot = (self.rx_prod % u32::from(RING_RX_COUNT)) as usize;
                {
                    let mut producer = RingProducer::new(self.region, self.b39, Ring::Rx)
                        .map_err(|error| format!("RX ring: {error}"))?;
                    producer
                        .push(desc)
                        .map_err(|error| format!("RX push: {error}"))?;
                }
                if let Some(slot) = self.rx_slot_buffer.get_mut(producer_slot) {
                    *slot = id as u8;
                }
                self.rx_prod = self.rx_prod.wrapping_add(1);
                RX_FRAMES.fetch_add(1, Ordering::AcqRel);
            }
            Ok(())
        }

        fn retire_rx(&mut self) -> Result<(), String> {
            let header = RingHeader::read_from(self.region)
                .map_err(|error| format!("ring header: {error}"))?;
            let cons = header.rx_cons;
            while self.rx_cons_seen != cons {
                let slot = (self.rx_cons_seen % u32::from(RING_RX_COUNT)) as usize;
                let buffer = self.rx_slot_buffer.get(slot).copied().unwrap_or_default() as usize;
                self.rx_cons_seen = self.rx_cons_seen.wrapping_add(1);
                if !self.rx_armed.get(buffer).copied().unwrap_or(false) {
                    self.arm_rx(buffer)?;
                }
            }
            Ok(())
        }

        fn pump_tx(&mut self) -> Result<(), String> {
            if self.tx_busy {
                self.dma.acquire();
                let used = self
                    .dma
                    .read_u16(self.tx.layout.used_idx())
                    .map_err(describe)?;
                if TX_STALL.load(Ordering::Acquire) {
                    // Test-only mutation: leave the used entry unread and the frame in flight.
                    self.tx_waits = self.tx_waits.wrapping_add(1);
                    if self.tx_waits.is_multiple_of(100) {
                        raven_rt::println!(
                            "virtio-net: tx-wait used={used} expected={} avail={} fired={} isr={:#x}",
                            self.tx.used,
                            self.tx.avail,
                            self.last_fired,
                            self.last_isr
                        );
                    }
                    return Ok(());
                }
                if used != self.tx.used {
                    if used.wrapping_sub(self.tx.used) != 1 {
                        return self.fail("TX used index drift".to_string());
                    }
                    let slot = self.tx.used % self.qsize;
                    let offset = self
                        .tx
                        .layout
                        .used_slot(slot)
                        .ok_or_else(|| "TX used slot".to_string())?;
                    let mut entry = [0u8; 8];
                    self.dma.read_bytes(offset, &mut entry).map_err(describe)?;
                    let used_entry = decode_used(&entry).ok_or("short TX used entry")?;
                    if used_entry.id != 0 {
                        return self.fail(format!("TX used id {}", used_entry.id));
                    }
                    self.tx.used = self.tx.used.wrapping_add(1);
                    {
                        let mut consumer = RingConsumer::new(self.region, self.b39, Ring::Tx)
                            .map_err(|error| format!("TX ring: {error}"))?;
                        let _ = consumer.pop();
                    }
                    self.tx_busy = false;
                    self.tx_waits = 0;
                    TX_FRAMES.fetch_add(1, Ordering::AcqRel);
                } else {
                    self.tx_waits = self.tx_waits.wrapping_add(1);
                    if self.tx_waits.is_multiple_of(100) {
                        raven_rt::println!(
                            "virtio-net: tx-wait used={used} expected={} avail={} fired={} isr={:#x}",
                            self.tx.used,
                            self.tx.avail,
                            self.last_fired,
                            self.last_isr
                        );
                    }
                }
                return Ok(());
            }

            let desc = {
                let consumer = RingConsumer::new(self.region, self.b39, Ring::Tx)
                    .map_err(|error| format!("TX ring: {error}"))?;
                consumer.peek()
            };
            let Some(desc) = desc else {
                return Ok(());
            };
            if frame_desc(self.region, &self.b39, desc).is_none() {
                {
                    let mut consumer = RingConsumer::new(self.region, self.b39, Ring::Tx)
                        .map_err(|error| format!("TX ring: {error}"))?;
                    let _ = consumer.pop();
                }
                TX_DROPS.fetch_add(1, Ordering::AcqRel);
                return Ok(());
            }

            // The device expects a virtio-net header first; the frame itself stays in the pool.
            let zero = [0u8; NET_HEADER_LEN];
            self.dma
                .write_bytes(TX_HEADER_OFFSET, &zero)
                .map_err(describe)?;
            let header = Descriptor {
                addr: self.bus + TX_HEADER_OFFSET as u64,
                len: NET_HEADER_LEN as u32,
                flags: virtio_net::DESC_F_NEXT,
                next: 1,
            };
            let frame = Descriptor {
                addr: self.region_bus + desc.addr,
                len: desc.len,
                flags: 0,
                next: 0,
            };
            self.dma
                .write_bytes(self.tx.layout.desc, &encode_descriptor(header))
                .map_err(describe)?;
            self.dma
                .write_bytes(self.tx.layout.desc + 16, &encode_descriptor(frame))
                .map_err(describe)?;
            self.push_avail(false, 0)?;
            self.tx_busy = true;
            Ok(())
        }
    }

    /// The control scheme handler.
    struct NicHandler {
        test: bool,
        dma_handle: Handle,
        pending: Option<(u64, String)>,
        opens: BTreeMap<u64, OpenKind>,
        next_id: u64,
        quit: bool,
    }

    enum OpenKind {
        Text { data: Vec<u8>, cursor: usize },
        Start,
        Stop,
        Quit,
    }

    impl NicHandler {
        fn new(test: bool, dma_handle: Handle) -> Self {
            Self {
                test,
                dma_handle,
                pending: None,
                opens: BTreeMap::new(),
                next_id: 1,
                quit: false,
            }
        }

        fn expect_open(&mut self, file_id: u64, path: &str) {
            self.pending = Some((file_id, path.to_string()));
        }

        fn should_quit(&self) -> bool {
            self.quit
        }

        fn text_for(&self, path: &str) -> Option<Vec<u8>> {
            let mut info = INFO.lock().as_ref().copied()?;
            info.link = LINK_UP.load(Ordering::Acquire);
            let mut buffer = [0u8; 256];
            match path {
                NIC_INFO => {
                    let length = info.encode(&mut buffer)?;
                    Some(buffer[..length].to_vec())
                }
                NIC_MAC => Some(format!("{}\n", info.mac).into_bytes()),
                NIC_MTU => Some(format!("{}\n", info.mtu).into_bytes()),
                NIC_LINK => {
                    Some(format!("{}\n", if info.link { "up" } else { "down" }).into_bytes())
                }
                _ => None,
            }
        }

        fn insert(&mut self, file_id: u64, kind: OpenKind) -> Result<(), Errno> {
            if self.opens.insert(file_id, kind).is_some() {
                return Err(Errno::EEXIST);
            }
            Ok(())
        }
    }

    impl Handler for NicHandler {
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
            if expected != path {
                return Err(Errno::EINVAL.into());
            }
            if path.is_empty() || path == "/" {
                // svcd's scheme readiness probe opens the scheme root.
                if !flags.readable() {
                    return Err(Errno::EACCES.into());
                }
                let data = b"info\nmac\nmtu\nlink\nring\nstart\nstop\nquit\n".to_vec();
                self.insert(file_id, OpenKind::Text { data, cursor: 0 })?;
                return Ok(Opened::File(file_id));
            }
            if path == NIC_RING {
                if !flags.readable() {
                    return Err(Errno::EACCES.into());
                }
                return Ok(Opened::Attach(self.dma_handle));
            }
            let kind = match path {
                NIC_INFO | NIC_MAC | NIC_MTU | NIC_LINK => {
                    if !flags.readable() {
                        return Err(Errno::EACCES.into());
                    }
                    let data = self.text_for(path).ok_or(Errno::ENOENT)?;
                    OpenKind::Text { data, cursor: 0 }
                }
                NIC_START | NIC_STOP | NIC_QUIT => {
                    if !flags.writable() {
                        return Err(Errno::EACCES.into());
                    }
                    match path {
                        NIC_START => OpenKind::Start,
                        NIC_STOP => OpenKind::Stop,
                        _ => OpenKind::Quit,
                    }
                }
                _ => return Err(Errno::ENOENT.into()),
            };
            self.insert(file_id, kind)?;
            Ok(Opened::File(file_id))
        }

        fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
            let Some(OpenKind::Text { data, cursor }) = self.opens.get_mut(&file) else {
                return Err(Errno::EBADF.into());
            };
            let remaining = data.get(*cursor..).unwrap_or(&[]);
            let length = remaining.len().min(buffer.len());
            let target = buffer.get_mut(..length).ok_or(Errno::EINVAL)?;
            target.copy_from_slice(remaining.get(..length).unwrap_or(&[]));
            *cursor += length;
            Ok(length)
        }

        fn write(&mut self, file: u64, _offset: u64, buffer: &[u8]) -> HandlerResult<usize> {
            let Some(kind) = self.opens.get(&file) else {
                return Err(Errno::EBADF.into());
            };
            match kind {
                OpenKind::Start => STARTED.store(true, Ordering::Release),
                OpenKind::Stop => STARTED.store(false, Ordering::Release),
                OpenKind::Quit => {
                    if !self.test {
                        return Err(Errno::EPERM.into());
                    }
                    QUIT.store(true, Ordering::Release);
                    self.quit = true;
                }
                OpenKind::Text { .. } => return Err(Errno::EBADF.into()),
            }
            Ok(buffer.len())
        }

        fn close(&mut self, file: u64) -> HandlerResult<()> {
            self.opens
                .remove(&file)
                .map(|_| ())
                .ok_or(Errno::EBADF.into())
        }

        fn dup(&mut self, file: u64) -> HandlerResult<u64> {
            let Some(kind) = self.opens.get(&file) else {
                return Err(Errno::EBADF.into());
            };
            let clone = match kind {
                OpenKind::Text { data, cursor } => OpenKind::Text {
                    data: data.clone(),
                    cursor: *cursor,
                },
                OpenKind::Start => OpenKind::Start,
                OpenKind::Stop => OpenKind::Stop,
                OpenKind::Quit => OpenKind::Quit,
            };
            let id = self.next_id;
            self.next_id = self.next_id.wrapping_add(1).max(1);
            self.insert(id, clone)?;
            Ok(id)
        }

        fn fstat(&mut self, file: u64) -> HandlerResult<Stat> {
            let Some(kind) = self.opens.get(&file) else {
                return Err(Errno::EBADF.into());
            };
            let (size, mode) = match kind {
                OpenKind::Text { data, .. } => (data.len() as u64, S_IFREG | 0o444),
                _ => (0, S_IFREG | 0o222),
            };
            Ok(Stat {
                ino: file,
                size,
                blocks: size.div_ceil(512),
                mode,
                nlink: 1,
                ..Stat::default()
            })
        }
    }

    /// B13b compatibility: send one ARP request and print the reply, then reset and exit.
    fn run_legacy(nic: &mut Nic) -> Result<(), String> {
        // The legacy line is programmed edge-triggered; unmask it before submitting so a fast
        // completion cannot be lost while the line is still masked.
        let _ = nic.irq.ack();
        let offset = nic
            .b39
            .buffer_offset(RING_RX_BUFFERS)
            .ok_or_else(|| "legacy TX buffer".to_string())?;
        let mut arp = [0u8; 42];
        build_arp_request(nic.mac, GUEST_IP, GATEWAY_IP, &mut arp)
            .ok_or_else(|| "ARP frame".to_string())?;
        nic.region
            .get_mut(offset..offset + arp.len())
            .ok_or_else(|| "legacy TX slice".to_string())?
            .copy_from_slice(&arp);
        let zero = [0u8; NET_HEADER_LEN];
        nic.dma
            .write_bytes(TX_HEADER_OFFSET, &zero)
            .map_err(describe)?;
        let header = Descriptor {
            addr: nic.bus + TX_HEADER_OFFSET as u64,
            len: NET_HEADER_LEN as u32,
            flags: virtio_net::DESC_F_NEXT,
            next: 1,
        };
        let frame = Descriptor {
            addr: nic.region_bus + offset as u64,
            len: arp.len() as u32,
            flags: 0,
            next: 0,
        };
        nic.dma
            .write_bytes(nic.tx.layout.desc, &encode_descriptor(header))
            .map_err(describe)?;
        nic.dma
            .write_bytes(nic.tx.layout.desc + 16, &encode_descriptor(frame))
            .map_err(describe)?;
        nic.push_avail(false, 0)?;
        raven_rt::println!("virtio-net: tx frame 42 bytes");
        let _ = legacy_wait(nic, false)?;
        let (id, length) = legacy_wait(nic, true)?;
        let start = nic
            .b39
            .buffer_offset(id)
            .ok_or_else(|| "RX buffer".to_string())?
            .checked_add(VIRTIO_NET_HEADER)
            .ok_or_else(|| "RX header".to_string())?;
        let frame_len = length
            .checked_sub(VIRTIO_NET_HEADER)
            .ok_or_else(|| "RX length".to_string())?;
        let end = start
            .checked_add(frame_len)
            .ok_or_else(|| "RX range".to_string())?;
        let frame = nic
            .region
            .get(start..end)
            .ok_or_else(|| "RX slice".to_string())?;
        let reply = parse_arp_reply(frame).ok_or_else(|| "no ARP reply".to_string())?;
        raven_rt::println!("virtio-net: rx frame {frame_len} bytes");
        raven_rt::println!(
            "virtio-net: arp reply from {} ({}.{}.{}.{})",
            reply.sender_mac,
            reply.sender_ip[0],
            reply.sender_ip[1],
            reply.sender_ip[2],
            reply.sender_ip[3]
        );
        nic.reset()?;
        raven_rt::println!("virtio-net: done");
        Ok(())
    }

    fn legacy_wait(nic: &mut Nic, rx: bool) -> Result<(usize, usize), String> {
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            if rounds > 5 {
                return Err("legacy completion timeout".to_string());
            }
            let fired = nic.irq.wait(1_000_000_000).unwrap_or(0);
            if fired == 0 {
                continue;
            }
            let _ = nic.bar.read_u8(nic.caps.isr.offset).map_err(describe)?;
            let _ = nic.irq.ack();
            nic.dma.acquire();
            let (layout, seen) = if rx {
                (nic.rx.layout, nic.rx.used)
            } else {
                (nic.tx.layout, nic.tx.used)
            };
            let used = nic.dma.read_u16(layout.used_idx()).map_err(describe)?;
            if used == seen {
                continue;
            }
            let slot = seen % nic.qsize;
            let offset = layout
                .used_slot(slot)
                .ok_or_else(|| "legacy used slot".to_string())?;
            let mut entry = [0u8; 8];
            nic.dma.read_bytes(offset, &mut entry).map_err(describe)?;
            let entry = decode_used(&entry).ok_or_else(|| "legacy used entry".to_string())?;
            if rx {
                nic.rx.used = seen.wrapping_add(1);
            } else {
                nic.tx.used = seen.wrapping_add(1);
            }
            return Ok((
                usize::try_from(entry.id).map_err(|_| "legacy used id")?,
                usize::try_from(entry.len).map_err(|_| "legacy used length")?,
            ));
        }
    }

    fn read_capabilities(
        config: &MmioRegion,
        bar_index: usize,
        bar_size: usize,
    ) -> Result<Caps, String> {
        let mut common = None;
        let mut notify = None;
        let mut isr = None;
        let mut device = None;
        let mut multiplier = 0u32;
        // QEMU prepends each capability, so the chain runs from the highest offset down.
        let mut seen = [0u8; raven_driver::MAX_CAPABILITIES];
        let mut seen_len = 0usize;
        let mut current = first_capability(config)?;
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
            mmio: false,
            common: common.ok_or_else(|| "no common configuration capability".to_string())?,
            notify: notify.ok_or_else(|| "no notify capability".to_string())?,
            isr: isr.ok_or_else(|| "no ISR capability".to_string())?,
            device: device.ok_or_else(|| "no device configuration capability".to_string())?,
            notify_multiplier: multiplier,
        })
    }

    fn next_capability(config: &MmioRegion, next: u8) -> Result<Option<Capability>, String> {
        if next == 0 {
            return Ok(None);
        }
        capability(config, next)
    }

    fn first_capability(config: &MmioRegion) -> Result<Option<Capability>, String> {
        let status = config.read_u16(0x06).map_err(describe)?;
        if status & (1 << 4) == 0 {
            return Ok(None);
        }
        let pointer = config.read_u8(0x34).map_err(describe)? & 0xFC;
        if pointer == 0 {
            return Ok(None);
        }
        capability(config, pointer)
    }

    fn capability(config: &MmioRegion, offset: u8) -> Result<Option<Capability>, String> {
        let offset = usize::from(offset);
        if !offset.is_multiple_of(4) || offset < 0x40 || offset + 1 >= raven_driver::PAGE {
            return Ok(None);
        }
        let id = config.read_u8(offset).map_err(describe)?;
        if id == 0 {
            return Ok(None);
        }
        let next = config.read_u8(offset + 1).map_err(describe)? & 0xFC;
        Ok(Some(Capability {
            id,
            offset: offset as u8,
            next,
        }))
    }

    /// One function's MSI-X capability.
    struct MsixCap {
        offset: u8,
        table_bar: usize,
        table_offset: usize,
        table_size: u16,
    }

    fn find_msix(config: &MmioRegion) -> Result<Option<MsixCap>, String> {
        let mut current = first_capability(config)?;
        let mut seen = [0u8; raven_driver::MAX_CAPABILITIES];
        let mut seen_len = 0usize;
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
            seen_len = seen_len.saturating_add(1);
            if capability.id == 0x11 {
                let control = config.read_u16(offset + 2).map_err(describe)?;
                let table = config.read_u32(offset + 4).map_err(describe)?;
                return Ok(Some(MsixCap {
                    offset: capability.offset,
                    table_bar: usize::try_from(table & 7).unwrap_or(usize::MAX),
                    table_offset: usize::try_from(table & !7).unwrap_or(usize::MAX),
                    table_size: (control & 0x7FF).saturating_add(1),
                }));
            }
            current = next_capability(config, capability.next)?;
        }
        Ok(None)
    }

    /// Program the MSI-X table for the RX/TX queue vector and enable MSI-X.
    ///
    /// Entry 0 is the configuration vector and is masked; entry 1 is shared by both queues.
    fn configure_msix(
        table: &mut MmioRegion,
        config: &mut MmioRegion,
        cap: &MsixCap,
        info: &MsixInfo,
    ) -> Result<(), String> {
        if cap.table_bar != info.bar || cap.table_offset != info.table {
            return Err(format!(
                "MSI-X table is bar{} offset {:#x}, devd passed bar{} offset {:#x}",
                cap.table_bar, cap.table_offset, info.bar, info.table
            ));
        }
        if cap.table_size < 2 {
            return Err(format!(
                "MSI-X table has {} entries; need 2",
                cap.table_size
            ));
        }
        let end = info
            .table
            .checked_add(32)
            .ok_or_else(|| "MSI-X table offset overflows".to_string())?;
        if end > info.bar_size {
            return Err("MSI-X table is outside the BAR".to_string());
        }
        table.write_u32(info.table, 0).map_err(describe)?;
        table.write_u32(info.table + 4, 0).map_err(describe)?;
        table.write_u32(info.table + 8, 0).map_err(describe)?;
        table.write_u32(info.table + 12, 1).map_err(describe)?;
        table
            .write_u32(info.table + 16, info.address_lo)
            .map_err(describe)?;
        table
            .write_u32(info.table + 20, info.address_hi)
            .map_err(describe)?;
        table
            .write_u32(info.table + 24, info.data)
            .map_err(describe)?;
        table.write_u32(info.table + 28, 0).map_err(describe)?;
        let offset = usize::from(cap.offset);
        let control = config.read_u16(offset + 2).map_err(describe)?;
        config
            .write_u16(offset + 2, (control | 0x8000) & !0x4000)
            .map_err(describe)?;
        let command = config.read_u16(0x04).map_err(describe)?;
        config.write_u16(0x04, command | 0x0400).map_err(describe)?;
        raven_rt::println!(
            "virtio-net: msix table={:#x} bar={} vector=1",
            info.table,
            info.bar
        );
        Ok(())
    }

    fn common(caps: &Caps, register: usize) -> usize {
        if !caps.mmio {
            return caps.common.offset + register;
        }
        let offset = match register {
            DEVICE_FEATURE_SELECT => mmio::DEVICE_FEATURES_SEL,
            DEVICE_FEATURE => mmio::DEVICE_FEATURES,
            DRIVER_FEATURE_SELECT => mmio::DRIVER_FEATURES_SEL,
            DRIVER_FEATURE => mmio::DRIVER_FEATURES,
            DEVICE_STATUS => mmio::STATUS,
            QUEUE_SELECT => mmio::QUEUE_SEL,
            QUEUE_SIZE => mmio::QUEUE_NUM,
            QUEUE_ENABLE => mmio::QUEUE_READY,
            _ => register,
        };
        caps.common.offset + offset
    }

    fn write_status(bar: &mut MmioRegion, caps: &Caps, status: u8) -> Result<(), String> {
        if caps.mmio {
            bar.write_u32(common(caps, DEVICE_STATUS), u32::from(status))
                .map_err(describe)
        } else {
            bar.write_u8(common(caps, DEVICE_STATUS), status)
                .map_err(describe)
        }
    }

    fn read_device_status(bar: &MmioRegion, caps: &Caps) -> Result<u8, String> {
        if caps.mmio {
            let value = bar
                .read_u32(common(caps, DEVICE_STATUS))
                .map_err(describe)?;
            Ok((value & 0xff) as u8)
        } else {
            bar.read_u8(common(caps, DEVICE_STATUS)).map_err(describe)
        }
    }

    fn reset_and_negotiate(bar: &mut MmioRegion, caps: &Caps) -> Result<Features, String> {
        write_status(bar, caps, 0)?;
        let mut spins = 0;
        while read_device_status(bar, caps)? != 0 {
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
        if caps.mmio {
            raven_rt::println!(
                "virtio-net: mmio device features low={:#010x} high={:#010x}",
                device.low,
                device.high
            );
        }
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
        let status = read_device_status(bar, caps)?;
        if caps.mmio {
            raven_rt::println!(
                "virtio-net: mmio driver features low={:#010x} high={:#010x} status={status:#04x}",
                driver.low,
                driver.high
            );
        }
        if status & STATUS_FEATURES_OK == 0 {
            return Err(format!(
                "device rejected the feature set (status={status:#04x} device={:#010x}:{:#010x} driver={:#010x}:{:#010x})",
                device.low, device.high, driver.low, driver.high
            ));
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

    /// `QueueNumMax` after selecting `queue` in the virtio-mmio transport.
    fn queue_maximum_mmio(bar: &mut MmioRegion, queue: u16) -> Result<u16, String> {
        bar.write_u32(mmio::QUEUE_SEL, u32::from(queue))
            .map_err(describe)?;
        let maximum = bar.read_u32(mmio::QUEUE_NUM_MAX).map_err(describe)?;
        Ok(u16::try_from(maximum).unwrap_or(u16::MAX))
    }

    /// Program one split queue through the virtio-mmio register window.
    fn program_queue_mmio(
        bar: &mut MmioRegion,
        queue: u16,
        layout: &SplitLayout,
        bus: u64,
    ) -> Result<usize, String> {
        bar.write_u32(mmio::QUEUE_SEL, u32::from(queue))
            .map_err(describe)?;
        bar.write_u32(mmio::QUEUE_NUM, u32::from(layout.size))
            .map_err(describe)?;
        let (desc_lo, desc_hi) = mmio::address_parts(bus + layout.desc as u64);
        let (avail_lo, avail_hi) = mmio::address_parts(bus + layout.avail as u64);
        let (used_lo, used_hi) = mmio::address_parts(bus + layout.used as u64);
        let (desc_lo_reg, desc_hi_reg) = mmio::address_offsets(mmio::QUEUE_DESC_LOW);
        let (avail_lo_reg, avail_hi_reg) = mmio::address_offsets(mmio::QUEUE_DRIVER_LOW);
        let (used_lo_reg, used_hi_reg) = mmio::address_offsets(mmio::QUEUE_DEVICE_LOW);
        bar.write_u32(desc_lo_reg, desc_lo).map_err(describe)?;
        bar.write_u32(desc_hi_reg, desc_hi).map_err(describe)?;
        bar.write_u32(avail_lo_reg, avail_lo).map_err(describe)?;
        bar.write_u32(avail_hi_reg, avail_hi).map_err(describe)?;
        bar.write_u32(used_lo_reg, used_lo).map_err(describe)?;
        bar.write_u32(used_hi_reg, used_hi).map_err(describe)?;
        bar.write_u32(mmio::QUEUE_READY, 1).map_err(describe)?;
        if bar.read_u32(mmio::QUEUE_READY).map_err(describe)? != 1 {
            return Err(format!("queue {queue} did not become ready"));
        }
        Ok(0)
    }

    fn program_queue(
        bar: &mut MmioRegion,
        caps: &Caps,
        queue: u16,
        layout: &SplitLayout,
        bus: u64,
        msix_vector: u16,
    ) -> Result<usize, String> {
        bar.write_u16(common(caps, QUEUE_SELECT), queue)
            .map_err(describe)?;
        bar.write_u16(common(caps, QUEUE_SIZE), layout.size)
            .map_err(describe)?;
        bar.write_u16(common(caps, QUEUE_MSIX_VECTOR), msix_vector)
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

    fn describe(error: AccessError) -> String {
        match error {
            AccessError::Bounds => "device access out of bounds".to_string(),
            AccessError::Alignment => "misaligned device access".to_string(),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
