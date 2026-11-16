// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GENET v5 userspace driver: Pi 4 `nic:` frame service.
//!
//! `devd` grants one MMIO window, one DMA window and the legacy IRQ. The driver maps them through
//! [`PlatformSeam`], performs the typed GENET bring-up, programs the Pi 4 rings and serves the
//! B39 `nic:` contract that `netd` consumes. The frame-service core and its host tests live in
//! [`genet::nic`]; this module is the target glue.
//!
//! The platform-device launch and the exact ring-register values are UNVERIFIED on a Pi 4. The
//! `devd` platform path currently only matches the device tree; see `docs/pi4-net.md` for the
//! board follow-up.

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
    use alloc::vec;
    use alloc::vec::Vec;
    use core::sync::atomic::{AtomicBool, Ordering};

    use genet::Controller;
    use genet::frame::FrameBackend;
    use genet::nic::{
        ControlInfo, DEFAULT_MTU, NIC_INFO, NIC_LINK, NIC_MAC, NIC_MTU, NIC_QUIT, NIC_RELEASE,
        NIC_RING, NIC_SCHEME, NIC_START, NIC_STOP, NicService, RING_REGION_LEN, RING_REGION_OFFSET,
        RING_RX_COUNT, RING_TX_BUFFERS, ReleaseCommand, platform_layout, ring_layout,
    };
    use genet::platform::PlatformSeam;
    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_driver::Irq;
    use raven_genet::dtb::{DmaMap, GenetInfo};
    use raven_genet::link::LinkEvent;
    use raven_genet::mac::MacAddress;
    use raven_genet::phy::PhyMode;
    use raven_proto_fs::{OpenFlags, S_IFREG, Stat};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_rt::thread;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// A fresh child has 0 (self), 1 (initfs), 2 (namespace); `devd` grants in order.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);
    const OPEN_READ: u32 = 1;
    const IRQ_WAIT_NS: u64 = 10_000_000;
    const LINK_POLL_LOOPS: usize = 100;
    const MAC_RETRIES: usize = 100;
    const MAC_RETRY_NS: u64 = 10_000_000;

    struct State {
        service: NicService<PlatformSeam>,
        region: Vec<u8>,
    }

    static STATE: Mutex<Option<State>> = Mutex::new(None);
    static INFO: Mutex<Option<ControlInfo>> = Mutex::new(None);
    static LINK_UP: AtomicBool = AtomicBool::new(false);
    static STARTED: AtomicBool = AtomicBool::new(false);
    static QUIT: AtomicBool = AtomicBool::new(false);
    static PUMP_DONE: AtomicBool = AtomicBool::new(false);

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("genet: {message}");
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
            } else if argument == "serve" {
                // The production mode token; accepted for profile symmetry.
            }
        }
        if bar_index > 5 {
            return Err(format!("bar{bar_index} is not a PCI BAR index"));
        }
        if bar_size == 0 || dma_size == 0 {
            return Err("devd did not pass a usable BAR and DMA buffer".to_string());
        }

        let ring = ring_layout().map_err(describe)?;
        let layout = platform_layout(ring, dma_size).map_err(describe)?;
        let seam = PlatformSeam::from_handles(BAR_HANDLE, bar_size, DMA_HANDLE, dma_size, layout)
            .map_err(describe)?;
        let mac = read_plat_mac().ok_or_else(|| "plat:mac did not return a MAC".to_string())?;
        let info = pi4_info(Some(mac));
        let mut controller = Controller::attach(seam, info).map_err(describe)?;
        let revision = controller.probe().map_err(describe)?;
        controller.program_mac(mac).map_err(describe)?;
        controller.program_rgmii(false).map_err(describe)?;
        controller.ready().map_err(describe)?;
        raven_rt::println!(
            "genet: revision=0x{revision:08x} mac={mac} bar={bar_index} size={bar_size} dma={dma_size} irq={irq_line}"
        );

        let mut backend =
            FrameBackend::with_counts(controller, RING_RX_COUNT, RING_TX_BUFFERS as u16, 2)
                .map_err(describe)?;
        backend.program_rings().map_err(describe)?;
        backend.program_filters(mac, &[], false).map_err(describe)?;
        let region_bus = backend.seam().dma_bus_address() + RING_REGION_OFFSET as u64;
        let mut service = NicService::new(backend, ring, region_bus, dma_size).map_err(describe)?;
        let mut region = vec![0u8; RING_REGION_LEN];
        service.init_region(&mut region).map_err(describe)?;
        service.arm_rx().map_err(describe)?;
        service.write_region(&region).map_err(describe)?;
        *INFO.lock() = Some(service.info());
        *STATE.lock() = Some(State { service, region });
        STARTED.store(false, Ordering::Release);
        QUIT.store(false, Ordering::Release);
        PUMP_DONE.store(false, Ordering::Release);
        thread::spawn(pump_thread, 0, 128 * 1024)
            .map_err(|error| format!("pump thread: {}", error.description()))?;

        let mut handler = NicHandler::new(test);
        let mut server = Server::register(NIC_SCHEME)
            .map_err(|error| format!("register {NIC_SCHEME}: {}", error.description()))?;
        raven_rt::println!("genet: serving {NIC_SCHEME}:");
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
        raven_rt::println!("genet: serve done");
        Ok(())
    }

    fn pump_thread(_: usize) {
        let irq = Irq::new(IRQ_HANDLE);
        let mut loops = 0usize;
        loop {
            if QUIT.load(Ordering::Acquire) {
                break;
            }
            let mut guard = STATE.lock();
            let Some(state) = guard.as_mut() else {
                break;
            };
            loops = loops.wrapping_add(1);
            if loops.is_multiple_of(LINK_POLL_LOOPS) {
                match state.service.poll_link() {
                    Ok(Some(LinkEvent::Up { .. } | LinkEvent::Changed { .. })) => {
                        LINK_UP.store(true, Ordering::Release);
                        if let Some((speed, duplex)) = state.service.link() {
                            raven_rt::println!(
                                "GENET-NET-LINK speed={} duplex={}",
                                raven_genet::release::speed_mbps(speed),
                                raven_genet::release::duplex_name(duplex)
                            );
                        }
                    }
                    Ok(Some(LinkEvent::Down)) => LINK_UP.store(false, Ordering::Release),
                    Ok(None) => {}
                    Err(error) => raven_rt::eprintln!("genet: link: {error:?}"),
                }
            }
            if STARTED.load(Ordering::Acquire) {
                let result = state
                    .service
                    .read_region(&mut state.region)
                    .and_then(|()| state.service.service(&mut state.region, 64))
                    .and_then(|_| state.service.drain_tx(&mut state.region, 64).map(|_| ()))
                    .and_then(|()| state.service.write_region(&state.region));
                if let Err(error) = result {
                    raven_rt::eprintln!("genet: pump: {error:?}");
                    QUIT.store(true, Ordering::Release);
                }
            }
            drop(guard);
            let _ = irq.wait(IRQ_WAIT_NS);
            let _ = irq.ack();
        }
        PUMP_DONE.store(true, Ordering::Release);
    }

    fn pi4_info(mac: Option<MacAddress>) -> GenetInfo {
        GenetInfo {
            base: 0xfd58_0000,
            size: 0x1_0000,
            mdio_offset: 0xe14,
            mdio_size: 8,
            interrupts: [157, 158],
            interrupt_flags: [4, 4],
            phy_address: 1,
            phy_mode: PhyMode::RgmiiRxid,
            mac,
            dma: DmaMap::identity(),
        }
    }

    fn read_plat_mac() -> Option<MacAddress> {
        let client = raven_scheme::Client::new();
        for _ in 0..MAC_RETRIES {
            if let Ok(handle) = client.open("plat:mac", OPEN_READ) {
                let mut bytes = [0u8; 6];
                let read = client.read(handle, 0, &mut bytes);
                let _ = client.close(handle);
                if read == Ok(6) {
                    return Some(MacAddress::new(bytes));
                }
            }
            let _ = raven_rt::nanosleep(MAC_RETRY_NS);
        }
        None
    }

    fn describe(error: impl core::fmt::Debug) -> String {
        format!("{error:?}")
    }

    struct NicHandler {
        test: bool,
        pending: Option<(u64, String)>,
        opens: BTreeMap<u64, OpenKind>,
        next_id: u64,
        quit: bool,
    }

    enum OpenKind {
        Text { data: Vec<u8>, cursor: usize },
        Release { data: Vec<u8>, cursor: usize },
        Start,
        Stop,
        Quit,
    }

    impl NicHandler {
        fn new(test: bool) -> Self {
            Self {
                test,
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

        fn release_text() -> Vec<u8> {
            let mut buffer = [0u8; 512];
            let length = {
                let guard = STATE.lock();
                guard
                    .as_ref()
                    .and_then(|state| state.service.transcript_text(&mut buffer))
                    .unwrap_or(0)
            };
            if length == 0 {
                b"pending\n".to_vec()
            } else {
                buffer[..length].to_vec()
            }
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
                NIC_MAC => Some(
                    format!(
                        "{:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}\n",
                        info.mac[0],
                        info.mac[1],
                        info.mac[2],
                        info.mac[3],
                        info.mac[4],
                        info.mac[5]
                    )
                    .into_bytes(),
                ),
                NIC_MTU => Some(format!("{}\n", DEFAULT_MTU).into_bytes()),
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
                if !flags.readable() {
                    return Err(Errno::EACCES.into());
                }
                let data = b"info\nmac\nmtu\nlink\nring\nstart\nstop\nrelease\nquit\n".to_vec();
                self.insert(file_id, OpenKind::Text { data, cursor: 0 })?;
                return Ok(Opened::File(file_id));
            }
            if path == NIC_RING {
                if !flags.readable() {
                    return Err(Errno::EACCES.into());
                }
                return Ok(Opened::Attach(DMA_HANDLE));
            }
            if path == NIC_RELEASE {
                if !flags.readable() && !flags.writable() {
                    return Err(Errno::EACCES.into());
                }
                let data = Self::release_text();
                self.insert(file_id, OpenKind::Release { data, cursor: 0 })?;
                return Ok(Opened::File(file_id));
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
            if matches!(self.opens.get(&file), Some(OpenKind::Release { .. })) {
                let text = Self::release_text();
                if let Some(OpenKind::Release { data, cursor }) = self.opens.get_mut(&file) {
                    *data = text;
                    *cursor = 0;
                }
            }
            let Some(OpenKind::Text { data, cursor } | OpenKind::Release { data, cursor }) =
                self.opens.get_mut(&file)
            else {
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
                OpenKind::Release { .. } => {
                    let text = core::str::from_utf8(buffer).map_err(|_| Errno::EINVAL)?;
                    let command = ReleaseCommand::parse(text).ok_or(Errno::EINVAL)?;
                    let mut guard = STATE.lock();
                    let Some(state) = guard.as_mut() else {
                        return Err(Errno::ENODEV.into());
                    };
                    state
                        .service
                        .feed_release(command)
                        .map_err(|_| Errno::EINVAL)?;
                    if let Some(transcript) = state.service.transcript() {
                        raven_rt::println!("{transcript}");
                    }
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
                OpenKind::Release { data, cursor } => OpenKind::Release {
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
                OpenKind::Release { data, .. } => (data.len() as u64, S_IFREG | 0o666),
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
}

#[cfg(not(target_os = "none"))]
fn main() {}
