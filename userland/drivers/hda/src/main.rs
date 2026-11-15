// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target Intel HDA driver for QEMU `intel-hda` + `hda-output`.
//!
//! `devd` launches this program with the function's BAR, read-only configuration page, 64 KiB
//! DMA buffer and legacy IRQ. The driver brings the controller and codec up, finds the output
//! converter and pin, programs one output stream whose BDL covers a 32 KiB PCM ring, and serves
//! `hda:out0`: opening it attaches the same DMA buffer to `audiod`, which maps the ring header and
//! writes periods through the B44 `DeviceSink`.
//!
//! The driver never allocates device memory itself: the ring lives in the first half of the DMA
//! buffer `pcid`/`devd` grant, while CORB, RIRB, the BDL and the position buffer live in the
//! second half. A monitor thread polls the link position (and wakes on the stream interrupt) and
//! publishes `read_index`/`underruns` in the ring header.
//!
//! The register, verb, BDL and ring-geometry rules live in the host-tested `hda-driver` library.
//! This file is target-only and keeps `#![forbid(unsafe_code)]`; all MMIO and DMA goes through
//! `raven_driver`.

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

    use hda_driver::{
        BdlEntry, CORBCTL, CORBCTL_RUN, CORBLBASE, CORBRP, CORBSIZE, CORBUBASE, CORBWP, DPLBASE,
        DPUBASE, GCTL, GCTL_CRST, INTCTL, INTSTS, PCI_CLASS_MULTIMEDIA, PCI_SUBCLASS_AUDIO,
        QEMU_HDA_DEVICE, QEMU_HDA_VENDOR, RINTCNT, RIRBCTL, RIRBCTL_DMAEN, RIRBLBASE, RIRBSIZE,
        RIRBUBASE, RIRBWP, SD_BASE, SD_BDPL, SD_BDPU, SD_CBL, SD_CTL_HI, SD_CTL_LO, SD_FORMAT,
        SD_LPIB, SD_LVI, SD_STATUS, SDCTL_SRST, STATESTS, WIDGET_AUDIO_OUTPUT, WIDGET_PIN_COMPLEX,
        cmd4, cmd12, config_is_output, format_48k, lpib_delta_frames, ring_geometry_48k,
        stream_control, stream_descriptor_offset, widget_has_input_amp, widget_has_output_amp,
        widget_type,
    };
    use raven_abi::errno::Errno;
    use raven_abi::types::Handle;
    use raven_driver::{AccessError, ConfigRegion, DmaRegion, Irq, MmioRegion};
    use raven_proto_audio::ring::{HEADER_LEN, RingHeader, RingLayout};
    use raven_proto_audio::{Format, SampleFormat};
    use raven_proto_fs::{S_IFCHR, Stat};
    use raven_rt::entry;
    use raven_rt::sync::Mutex;
    use raven_rt::thread;
    use raven_scheme::{Ctx, Handler, HandlerResult, Opened, Server};

    entry!(main);

    /// Child handle order installed by `devd`: BAR, config, DMA, IRQ.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);

    /// The ring is exactly 32 KiB; the rest of the 64 KiB DMA buffer holds the command rings.
    const RING_BYTES: usize = 32 * 1024;
    const RING_DATA_OFFSET: usize = 128;
    const RING_PERIOD_FRAMES: u32 = 480;
    const RING_PERIOD_COUNT: u16 = 17;
    const RING_FRAME_BYTES: u16 = 4;
    const BDL_OFFSET: usize = 0x8000;
    const CORB_OFFSET: usize = 0x8200;
    const RIRB_OFFSET: usize = 0x8600;
    const DPL_OFFSET: usize = 0x8e00;
    const CORB_ENTRIES: usize = 256;
    const RIRB_ENTRIES: usize = 256;
    const CODEC: u8 = 0;
    const STREAM_TAG: u8 = 1;
    const DEVICE: &str = "out0";
    const IRQ_WAIT_NS: u64 = 2_000_000;
    const MAX_NODES: u8 = 32;

    // Header offsets; `RingHeader` is `repr(C)` and the compiler checks them in the library.
    const OFF_READ: usize = core::mem::offset_of!(RingHeader, read_index);
    const OFF_WRITE: usize = core::mem::offset_of!(RingHeader, write_index);
    const OFF_UNDERRUNS: usize = core::mem::offset_of!(RingHeader, underruns);
    const OFF_STATE: usize = core::mem::offset_of!(RingHeader, state);

    static STREAM: Mutex<Option<HdaStream>> = Mutex::new(None);

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("hda: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let mut bar_size = 0usize;
        let mut dma_size = 0usize;
        let mut irq_line = 0u32;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar_size=") {
                bar_size = value.parse().map_err(|_| bad_argument(argument))?;
            } else if let Some(value) = argument.strip_prefix("dma=") {
                dma_size = value.parse().map_err(|_| bad_argument(argument))?;
            } else if let Some(value) = argument.strip_prefix("irq=") {
                irq_line = value.parse().map_err(|_| bad_argument(argument))?;
            }
        }
        if bar_size == 0 || dma_size < 64 * 1024 {
            return Err("devd did not pass a usable BAR and DMA buffer".to_string());
        }

        let config_window =
            raven_rt::map_mmio(CONFIG_HANDLE, raven_driver::PAGE).map_err(errno_string)?;
        let config = ConfigRegion::new(config_window);
        let (vendor, device) = config.ids().map_err(access_string)?;
        let (class, subclass) = config.class().map_err(access_string)?;
        if vendor != QEMU_HDA_VENDOR || device != QEMU_HDA_DEVICE {
            return Err(format!(
                "unexpected HDA function vendor={vendor:#06x} device={device:#06x}"
            ));
        }
        if class != PCI_CLASS_MULTIMEDIA || subclass != PCI_SUBCLASS_AUDIO {
            return Err(format!(
                "unexpected HDA class {class:#04x}:{subclass:#04x}, want 0403"
            ));
        }

        let bar_window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size).map_err(errno_string)?;
        let bar = MmioRegion::new(bar_window);
        let dma_info = raven_rt::dma_info(DMA_HANDLE).map_err(errno_string)?;
        let dma_window = raven_rt::map_mmio_mut(DMA_HANDLE, dma_size).map_err(errno_string)?;
        let dma = DmaRegion::new(dma_window, dma_info.bus_addr);
        let irq = Irq::new(IRQ_HANDLE);

        raven_rt::println!("hda: id={vendor:04x}:{device:04x} class={class:02x}{subclass:02x}");
        raven_rt::println!(
            "hda: dma bus={:#x} len={} irq={}",
            dma.bus_address(),
            dma.len(),
            irq_line
        );

        let mut controller = HdaController::new(bar, dma, irq);
        controller.init()?;
        let stream = controller.into_stream()?;
        raven_rt::println!(
            "hda: codec={} dac={} pin={} stream={} ring={}x{}x{}",
            stream.codec,
            stream.dac,
            stream.pin,
            stream.stream_desc,
            RING_PERIOD_COUNT,
            RING_PERIOD_FRAMES,
            RING_FRAME_BYTES
        );
        *STREAM.lock() = Some(stream);
        if let Err(error) = thread::spawn(monitor, 0, 64 * 1024) {
            return Err(format!("monitor thread: {}", error.description()));
        }

        raven_rt::println!("hda: serving");
        let mut server = Server::register("hda").map_err(errno_string)?;
        let mut handler = HdaHandler;
        server.run(&mut handler);
    }

    fn bad_argument(argument: &str) -> String {
        format!("bad argument `{argument}`")
    }

    fn errno_string(error: Errno) -> String {
        error.description().to_string()
    }

    fn access_string(error: AccessError) -> String {
        match error {
            AccessError::Bounds => "device access outside the mapped window".to_string(),
            AccessError::Alignment => "misaligned device access".to_string(),
        }
    }

    fn access<T>(result: Result<T, AccessError>) -> Result<T, String> {
        result.map_err(access_string)
    }

    struct Node {
        sub_count: u8,
        sub_start: u8,
        capabilities: u32,
        config_default: u32,
        connections: Vec<u8>,
        connection_default: u8,
    }

    impl Default for Node {
        fn default() -> Self {
            Self {
                sub_count: 0,
                sub_start: 0,
                capabilities: 0,
                config_default: 0,
                connections: Vec::new(),
                connection_default: 0,
            }
        }
    }

    struct HdaController {
        bar: MmioRegion,
        dma: DmaRegion,
        irq: Irq,
        input_streams: u8,
        output_streams: u8,
        stream_desc: usize,
        dac: u8,
        pin: u8,
        rirb_rp: u16,
    }

    impl HdaController {
        fn new(bar: MmioRegion, dma: DmaRegion, irq: Irq) -> Self {
            Self {
                bar,
                dma,
                irq,
                input_streams: 0,
                output_streams: 0,
                stream_desc: SD_BASE,
                dac: 0,
                pin: 0,
                rirb_rp: 0,
            }
        }

        fn init(&mut self) -> Result<(), String> {
            self.reset_controller()?;
            self.init_command_ring()?;
            self.enumerate()?;
            self.configure()?;
            Ok(())
        }

        fn reset_controller(&mut self) -> Result<(), String> {
            // Stop the command engines before touching the reset bit.
            self.stop_command_ring()?;
            access(self.bar.write_u16(STATESTS, 0x7fff))?;
            access(self.bar.write_u32(INTSTS, 0xffff_ffff))?;

            access(self.bar.write_u32(GCTL, 0))?;
            wait_for("controller reset low", || {
                access(self.bar.read_u32(GCTL)).map(|value| value & GCTL_CRST == 0)
            })?;
            let _ = raven_rt::nanosleep(1_000_000);
            access(self.bar.write_u32(GCTL, GCTL_CRST))?;
            wait_for("controller reset high", || {
                access(self.bar.read_u32(GCTL)).map(|value| value & GCTL_CRST != 0)
            })?;
            let _ = raven_rt::nanosleep(2_000_000);

            let _: u16 = wait_for_value("codec state", || {
                let value = access(self.bar.read_u16(STATESTS))?;
                Ok((value, value != 0))
            })?;
            let gcap = access(self.bar.read_u16(hda_driver::GCAP))?;
            self.input_streams = ((gcap >> 8) & 0x0f) as u8;
            self.output_streams = ((gcap >> 12) & 0x0f) as u8;
            if self.output_streams == 0 {
                return Err("controller reports no output streams".to_string());
            }
            Ok(())
        }

        fn stop_command_ring(&mut self) -> Result<(), String> {
            access(self.bar.write_u8(CORBCTL, 0))?;
            wait_for("CORB stop", || {
                access(self.bar.read_u8(CORBCTL)).map(|value| value & CORBCTL_RUN == 0)
            })?;
            access(self.bar.write_u8(RIRBCTL, 0))?;
            wait_for("RIRB stop", || {
                access(self.bar.read_u8(RIRBCTL)).map(|value| value & RIRBCTL_DMAEN == 0)
            })?;
            Ok(())
        }

        fn init_command_ring(&mut self) -> Result<(), String> {
            let corb_size = access(self.bar.read_u8(CORBSIZE))?;
            if corb_size & 0x40 == 0 {
                return Err("CORB does not support 256 entries".to_string());
            }
            access(self.bar.write_u8(CORBSIZE, (corb_size & 0xfc) | 0x02))?;
            let rirb_size = access(self.bar.read_u8(RIRBSIZE))?;
            if rirb_size & 0x40 == 0 {
                return Err("RIRB does not support 256 entries".to_string());
            }
            access(self.bar.write_u8(RIRBSIZE, (rirb_size & 0xfc) | 0x02))?;

            let corb_bus = self.dma.bus_address() + CORB_OFFSET as u64;
            access(self.bar.write_u32(CORBLBASE, corb_bus as u32))?;
            access(self.bar.write_u32(CORBUBASE, (corb_bus >> 32) as u32))?;
            let rirb_bus = self.dma.bus_address() + RIRB_OFFSET as u64;
            access(self.bar.write_u32(RIRBLBASE, rirb_bus as u32))?;
            access(self.bar.write_u32(RIRBUBASE, (rirb_bus >> 32) as u32))?;

            // CORB read pointer reset: set the reset bit, wait, clear it.
            access(self.bar.write_u16(CORBRP, 1 << 15))?;
            wait_for("CORBRP set", || {
                access(self.bar.read_u16(CORBRP)).map(|value| value & (1 << 15) != 0)
            })?;
            access(self.bar.write_u16(CORBRP, 0))?;
            wait_for("CORBRP clear", || {
                access(self.bar.read_u16(CORBRP)).map(|value| value & (1 << 15) == 0)
            })?;
            access(self.bar.write_u16(CORBWP, 0))?;

            access(self.bar.write_u16(RIRBWP, 1 << 15))?;
            access(self.bar.write_u16(RIRBWP, 0))?;
            self.rirb_rp = 0;
            // The controller stops pulling CORB entries when RINTCNT responses have been
            // produced; without an interrupt handler to clear RIRBSTS, use the maximum count.
            access(self.bar.write_u16(RINTCNT, 0xff))?;

            access(self.bar.write_u8(CORBCTL, CORBCTL_RUN))?;
            access(self.bar.write_u8(RIRBCTL, RIRBCTL_DMAEN))?;
            let interrupt_bit = 1u32 << u32::from(self.input_streams.min(30));
            access(self.bar.write_u32(INTCTL, (1 << 31) | interrupt_bit))?;
            raven_rt::println!(
                "hda: corb={:#x} rirb={:#x} corbctl={:#04x} rirbctl={:#04x} wp={} rp={} rirbwp={} intctl={:#x}",
                corb_bus,
                rirb_bus,
                access(self.bar.read_u8(CORBCTL))?,
                access(self.bar.read_u8(RIRBCTL))?,
                access(self.bar.read_u16(CORBWP))?,
                access(self.bar.read_u16(CORBRP))?,
                access(self.bar.read_u16(RIRBWP))?,
                access(self.bar.read_u32(INTCTL))?
            );
            Ok(())
        }

        fn enumerate(&mut self) -> Result<(), String> {
            let root = self.read_node(0)?;
            if root.sub_count == 0 {
                return Err("codec has no function groups".to_string());
            }
            let mut nodes = BTreeMap::<u8, Node>::new();
            let mut pins = Vec::new();
            let mut outputs = Vec::new();
            for index in 0..root.sub_count {
                let afg = root.sub_start.saturating_add(index);
                let afg_node = self.read_node(afg)?;
                for widget_index in 0..afg_node.sub_count {
                    let widget = afg_node.sub_start.saturating_add(widget_index);
                    if widget == 0 || widget >= MAX_NODES {
                        continue;
                    }
                    let node = self.read_node(widget)?;
                    match widget_type(node.capabilities) {
                        WIDGET_AUDIO_OUTPUT => outputs.push(widget),
                        WIDGET_PIN_COMPLEX => {
                            if config_is_output(node.config_default) {
                                pins.push(widget);
                            }
                        }
                        _ => {}
                    }
                    nodes.insert(widget, node);
                }
            }
            if outputs.is_empty() {
                return Err("codec has no audio output converter".to_string());
            }
            let pin = *pins.first().ok_or("codec has no output pin")?;
            let path = find_dac_path(pin, &nodes, 0).ok_or("no path from output pin to a DAC")?;
            let dac = *path.last().ok_or("empty DAC path")?;
            self.pin = pin;
            self.dac = dac;
            self.stream_desc = stream_descriptor_offset(self.input_streams, 0);
            raven_rt::println!("hda: path pin={} dac={} nodes={}", pin, dac, path.len());
            Ok(())
        }

        fn configure(&mut self) -> Result<(), String> {
            self.configure_widgets()?;
            self.program_stream()?;
            self.reset_ring()?;
            Ok(())
        }

        fn configure_widgets(&mut self) -> Result<(), String> {
            let path = self.path_to_dac().ok_or("codec output path disappeared")?;
            for node in &path {
                self.send_verb(*node, 0x705, 0)?;
            }
            self.send_verb(self.pin, 0x707, 0xc0)?;
            self.send_verb(self.pin, 0x70c, 2)?;
            self.send_verb(self.dac, 0x706, (STREAM_TAG << 4) & 0xff)?;
            self.send_cmd4(self.dac, 0x2, format_48k(2))?;
            for node in &path {
                let capabilities = self.send_verb(*node, 0xf00, 0x09)?;
                if widget_has_output_amp(capabilities) {
                    let caps = self.send_verb(*node, 0xf00, 0x12)?;
                    let gain = (caps & 0x7f) as u16;
                    let payload = 0x8000 | 0x2000 | 0x1000 | gain;
                    self.send_cmd4(*node, 0x3, payload)?;
                }
                if widget_has_input_amp(capabilities) {
                    let caps = self.send_verb(*node, 0xf00, 0x0d)?;
                    let gain = (caps & 0x7f) as u16;
                    let payload = 0x4000 | 0x2000 | 0x1000 | gain;
                    self.send_cmd4(*node, 0x3, payload)?;
                }
            }
            Ok(())
        }

        fn path_to_dac(&mut self) -> Option<Vec<u8>> {
            let mut nodes = BTreeMap::<u8, Node>::new();
            let root = self.read_node(0).ok()?;
            for index in 0..root.sub_count {
                let afg = root.sub_start.saturating_add(index);
                let afg_node = self.read_node(afg).ok()?;
                for widget_index in 0..afg_node.sub_count {
                    let widget = afg_node.sub_start.saturating_add(widget_index);
                    if widget == 0 || widget >= MAX_NODES {
                        continue;
                    }
                    if let Ok(node) = self.read_node(widget) {
                        nodes.insert(widget, node);
                    }
                }
            }
            find_dac_path(self.pin, &nodes, 0)
        }

        fn program_stream(&mut self) -> Result<(), String> {
            let geometry = ring_geometry_48k();
            let cbl = geometry.cbl_bytes();
            self.write_bdl(
                geometry.period_frames,
                geometry.period_count,
                geometry.frame_bytes,
            )?;

            access(self.bar.write_u32(self.stream_desc + SD_CTL_LO, 0))?;
            access(self.bar.write_u8(self.stream_desc + SD_STATUS, 0x1c))?;
            access(self.bar.write_u32(self.stream_desc + SD_CBL, cbl))?;
            access(self.bar.write_u16(
                self.stream_desc + SD_LVI,
                geometry.period_count.saturating_sub(1),
            ))?;
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_FORMAT, format_48k(2)),
            )?;
            let bdl_bus = self.dma.bus_address() + BDL_OFFSET as u64;
            access(
                self.bar
                    .write_u32(self.stream_desc + SD_BDPL, bdl_bus as u32),
            )?;
            access(
                self.bar
                    .write_u32(self.stream_desc + SD_BDPU, (bdl_bus >> 32) as u32),
            )?;
            let dpl_bus = self.dma.bus_address() + DPL_OFFSET as u64;
            access(self.bar.write_u32(DPLBASE, dpl_bus as u32))?;
            access(self.bar.write_u32(DPUBASE, (dpl_bus >> 32) as u32))?;
            let control = stream_control(STREAM_TAG, false, true);
            let reset = control | SDCTL_SRST;
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_CTL_LO, reset as u16),
            )?;
            access(
                self.bar
                    .write_u8(self.stream_desc + SD_CTL_HI, ((reset >> 16) & 0xff) as u8),
            )?;
            let _ = raven_rt::nanosleep(1_000_000);
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_CTL_LO, control as u16),
            )?;
            access(
                self.bar
                    .write_u8(self.stream_desc + SD_CTL_HI, ((control >> 16) & 0xff) as u8),
            )?;
            Ok(())
        }

        fn write_bdl(
            &mut self,
            period_frames: u32,
            period_count: u16,
            frame_bytes: u16,
        ) -> Result<(), String> {
            let period_bytes = period_frames
                .checked_mul(u32::from(frame_bytes))
                .ok_or("period byte size overflow")?;
            let mut bytes = [0u8; 16];
            for index in 0..period_count {
                let address = self.dma.bus_address()
                    + RING_DATA_OFFSET as u64
                    + u64::from(index) * u64::from(period_bytes);
                let entry = BdlEntry {
                    address,
                    length: period_bytes,
                    ioc: true,
                };
                if !entry.write(&mut bytes, 0) {
                    return Err("BDL entry buffer too small".to_string());
                }
                self.dma
                    .write_bytes(BDL_OFFSET + usize::from(index) * 16, &bytes)
                    .map_err(access_string)?;
            }
            Ok(())
        }

        fn reset_ring(&mut self) -> Result<(), String> {
            let format = Format {
                rate: 48_000,
                channels: 2,
                sample: SampleFormat::S16Le,
            };
            let layout = RingLayout::new(RING_BYTES, format, RING_PERIOD_FRAMES, RING_PERIOD_COUNT)
                .map_err(|error| format!("ring layout: {error}"))?;
            let mut header = [0u8; HEADER_LEN];
            layout
                .header(0)
                .write_to(&mut header)
                .map_err(|_| "ring header encoding".to_string())?;
            self.dma.write_bytes(0, &header).map_err(access_string)?;
            let zero = [0u8; 4096];
            let mut offset = RING_DATA_OFFSET;
            while offset < RING_DATA_OFFSET + RING_BYTES - HEADER_LEN {
                let end = (offset + zero.len()).min(RING_DATA_OFFSET + RING_BYTES - HEADER_LEN);
                self.dma
                    .write_bytes(offset, &zero[..end - offset])
                    .map_err(access_string)?;
                offset = end;
            }
            Ok(())
        }

        fn send_verb(&mut self, node: u8, verb: u16, payload: u8) -> Result<u32, String> {
            self.send_command(cmd12(CODEC, node, verb, payload))
        }

        fn send_cmd4(&mut self, node: u8, verb: u16, payload: u16) -> Result<u32, String> {
            self.send_command(cmd4(CODEC, node, verb, payload))
        }

        fn send_command(&mut self, command: u32) -> Result<u32, String> {
            wait_for("CORB free slot", || {
                let wp = access(self.bar.read_u16(CORBWP))? & 0xff;
                let rp = access(self.bar.read_u16(CORBRP))? & 0xff;
                Ok(wp == rp)
            })?;
            let write_pointer = access(self.bar.read_u16(CORBWP))? & 0xff;
            let index = (usize::from(write_pointer) + 1) % CORB_ENTRIES;
            self.dma
                .write_u32(CORB_OFFSET + index * 4, command)
                .map_err(access_string)?;
            self.dma.publish();
            access(self.bar.write_u16(CORBWP, index as u16))?;

            let previous = self.rirb_rp & 0xff;
            if let Err(error) = wait_for("RIRB response", || {
                let current = access(self.bar.read_u16(RIRBWP))? & 0xff;
                Ok(current != previous)
            }) {
                raven_rt::println!(
                    "hda: command={command:#010x} corbwp={} corbrp={} rirbwp={} corbctl={:#04x} rirbctl={:#04x}",
                    access(self.bar.read_u16(CORBWP))?,
                    access(self.bar.read_u16(CORBRP))?,
                    access(self.bar.read_u16(RIRBWP))?,
                    access(self.bar.read_u8(CORBCTL))?,
                    access(self.bar.read_u8(RIRBCTL))?
                );
                return Err(error);
            }
            self.dma.acquire();
            let response_index = (usize::from(previous) + 1) % RIRB_ENTRIES;
            let response = self
                .dma
                .read_u32(RIRB_OFFSET + response_index * 8)
                .map_err(access_string)?;
            self.rirb_rp = response_index as u16;
            Ok(response)
        }

        fn read_node(&mut self, node: u8) -> Result<Node, String> {
            let sub = self.send_verb(node, 0xf00, 0x04)?;
            let capabilities = self.send_verb(node, 0xf00, 0x09)?;
            let config_default = if node == 0 {
                0
            } else {
                self.send_verb(node, 0xf1c, 0x00)?
            };
            let connection_default = if node == 0 {
                0
            } else {
                (self.send_verb(node, 0xf01, 0x00)? & 0xff) as u8
            };
            let connections = self.read_connections(node)?;
            Ok(Node {
                sub_count: (sub & 0xff) as u8,
                sub_start: ((sub >> 16) & 0xff) as u8,
                capabilities,
                config_default,
                connections,
                connection_default,
            })
        }

        fn read_connections(&mut self, node: u8) -> Result<Vec<u8>, String> {
            let length = self.send_verb(node, 0xf00, 0x0e)?;
            let count = (length & 0x7f) as usize;
            let long = (length >> 7) & 1 != 0;
            let mut connections = Vec::new();
            let mut index = 0usize;
            while index < count {
                let response = self.send_verb(node, 0xf02, index as u8)?;
                if long {
                    for slot in 0..2 {
                        let value = ((response >> (16 * slot)) & 0xffff) as u16;
                        let address = (value & 0x7fff) as u8;
                        if address != 0 {
                            connections.push(address);
                        }
                    }
                } else {
                    for slot in 0..4 {
                        let value = ((response >> (8 * slot)) & 0xff) as u8;
                        let address = value & 0x7f;
                        if address != 0 {
                            connections.push(address);
                        }
                    }
                }
                let next = connections.len();
                if next <= index {
                    break;
                }
                index = next;
            }
            Ok(connections)
        }

        fn into_stream(self) -> Result<HdaStream, String> {
            let geometry = ring_geometry_48k();
            Ok(HdaStream {
                bar: self.bar,
                dma: self.dma,
                irq: self.irq,
                codec: CODEC,
                dac: self.dac,
                pin: self.pin,
                stream_desc: self.stream_desc,
                cbl: geometry.cbl_bytes(),
                frame_bytes: geometry.frame_bytes,
                running: false,
                last_lpib: 0,
                underrun: false,
            })
        }
    }

    fn find_dac_path(node: u8, nodes: &BTreeMap<u8, Node>, depth: u8) -> Option<Vec<u8>> {
        if depth > 8 {
            return None;
        }
        let info = nodes.get(&node)?;
        if widget_type(info.capabilities) == WIDGET_AUDIO_OUTPUT {
            return Some(alloc::vec![node]);
        }
        let connection = *info.connections.get(usize::from(info.connection_default))?;
        let mut path = find_dac_path(connection, nodes, depth.saturating_add(1))?;
        path.insert(0, node);
        Some(path)
    }

    struct HdaStream {
        bar: MmioRegion,
        dma: DmaRegion,
        irq: Irq,
        codec: u8,
        dac: u8,
        pin: u8,
        stream_desc: usize,
        cbl: u32,
        frame_bytes: u16,
        running: bool,
        last_lpib: u32,
        underrun: bool,
    }

    impl HdaStream {
        fn reset_ring(&mut self) -> Result<(), String> {
            self.stop()?;
            let format = Format {
                rate: 48_000,
                channels: 2,
                sample: SampleFormat::S16Le,
            };
            let layout = RingLayout::new(RING_BYTES, format, RING_PERIOD_FRAMES, RING_PERIOD_COUNT)
                .map_err(|error| format!("ring layout: {error}"))?;
            let mut header = [0u8; HEADER_LEN];
            layout
                .header(0)
                .write_to(&mut header)
                .map_err(|_| "ring header encoding".to_string())?;
            self.dma.write_bytes(0, &header).map_err(access_string)?;
            let zero = [0u8; 4096];
            let end = RING_DATA_OFFSET + RING_BYTES - HEADER_LEN;
            let mut offset = RING_DATA_OFFSET;
            while offset < end {
                let chunk_end = (offset + zero.len()).min(end);
                self.dma
                    .write_bytes(offset, &zero[..chunk_end - offset])
                    .map_err(access_string)?;
                offset = chunk_end;
            }
            self.last_lpib = 0;
            self.underrun = false;
            Ok(())
        }

        fn start(&mut self) -> Result<(), String> {
            let control = stream_control(STREAM_TAG, true, true);
            raven_rt::println!("hda: stream start");
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_CTL_LO, control as u16),
            )?;
            access(
                self.bar
                    .write_u8(self.stream_desc + SD_CTL_HI, ((control >> 16) & 0xff) as u8),
            )?;
            self.running = true;
            self.last_lpib = access(self.bar.read_u32(self.stream_desc + SD_LPIB))?;
            self.underrun = false;
            Ok(())
        }

        fn stop(&mut self) -> Result<(), String> {
            let control = stream_control(STREAM_TAG, false, true);
            if self.running {
                raven_rt::println!("hda: stream stop");
            }
            let reset = control | SDCTL_SRST;
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_CTL_LO, reset as u16),
            )?;
            access(
                self.bar
                    .write_u8(self.stream_desc + SD_CTL_HI, ((reset >> 16) & 0xff) as u8),
            )?;
            let _ = raven_rt::nanosleep(1_000_000);
            access(
                self.bar
                    .write_u16(self.stream_desc + SD_CTL_LO, control as u16),
            )?;
            access(
                self.bar
                    .write_u8(self.stream_desc + SD_CTL_HI, ((control >> 16) & 0xff) as u8),
            )?;
            self.running = false;
            Ok(())
        }

        fn tick(&mut self) -> Result<(), String> {
            let state = self.dma.read_u8(OFF_STATE).map_err(access_string)?;
            let write_index = self.dma.read_u64(OFF_WRITE).map_err(access_string)?;
            let read_index = self.dma.read_u64(OFF_READ).map_err(access_string)?;
            if state == 0 {
                if self.running {
                    self.stop()?;
                }
                return Ok(());
            }
            if !self.running && write_index > read_index {
                self.start()?;
            }
            if !self.running {
                return Ok(());
            }
            let lpib = access(self.bar.read_u32(self.stream_desc + SD_LPIB))?;
            let delta = lpib_delta_frames(self.last_lpib, lpib, self.cbl, self.frame_bytes);
            self.last_lpib = lpib;
            if delta == 0 {
                self.clear_status()?;
                return Ok(());
            }
            let frames = u64::from(delta);
            let available = write_index.saturating_sub(read_index);
            let next_read = if frames > available {
                (write_index, true)
            } else {
                (read_index.saturating_add(frames), false)
            };
            if next_read.1 {
                if !self.underrun {
                    let count = self
                        .dma
                        .read_u32(OFF_UNDERRUNS)
                        .map_err(access_string)?
                        .saturating_add(1);
                    self.dma
                        .write_u32(OFF_UNDERRUNS, count)
                        .map_err(access_string)?;
                    self.underrun = true;
                }
            } else if available >= u64::from(RING_PERIOD_FRAMES) {
                self.underrun = false;
            }
            self.dma
                .write_u64(OFF_READ, next_read.0)
                .map_err(access_string)?;
            self.clear_status()?;
            Ok(())
        }

        fn clear_status(&mut self) -> Result<(), String> {
            let status = access(self.bar.read_u8(self.stream_desc + SD_STATUS))?;
            if status & 0x1c != 0 {
                access(self.bar.write_u8(self.stream_desc + SD_STATUS, 0x1c))?;
            }
            Ok(())
        }
    }

    fn monitor(_argument: usize) {
        loop {
            let mut guard = STREAM.lock();
            if let Some(stream) = guard.as_mut() {
                let _ = stream.irq.wait(IRQ_WAIT_NS);
                let _ = stream.irq.ack();
                let _ = stream.tick();
            }
            drop(guard);
        }
    }

    fn wait_for<F>(what: &str, mut predicate: F) -> Result<(), String>
    where
        F: FnMut() -> Result<bool, String>,
    {
        for _ in 0..1000 {
            if predicate()? {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(1_000_000);
        }
        Err(format!("timeout: {what}"))
    }

    fn wait_for_value<T, F>(what: &str, mut predicate: F) -> Result<T, String>
    where
        F: FnMut() -> Result<(T, bool), String>,
    {
        for _ in 0..1000 {
            let (value, ready) = predicate()?;
            if ready {
                return Ok(value);
            }
            let _ = raven_rt::nanosleep(1_000_000);
        }
        Err(format!("timeout: {what}"))
    }

    struct HdaHandler;

    impl Handler for HdaHandler {
        fn open(
            &mut self,
            path: &str,
            _flags: u32,
            _mode: u32,
            _ctx: &Ctx,
        ) -> HandlerResult<Opened> {
            if path.is_empty() {
                return Ok(Opened::File(0));
            }
            if path != DEVICE {
                return Err(Errno::ENOENT.into());
            }
            let mut guard = STREAM.lock();
            let Some(stream) = guard.as_mut() else {
                return Err(Errno::EIO.into());
            };
            stream.reset_ring().map_err(|_| Errno::EIO)?;
            Ok(Opened::Attach(DMA_HANDLE))
        }

        fn fstat(&mut self, _file: u64) -> HandlerResult<Stat> {
            Ok(Stat {
                mode: S_IFCHR | 0o666,
                nlink: 1,
                ..Stat::default()
            })
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
