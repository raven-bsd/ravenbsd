// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Guest xHCI driver for the QEMU `qemu-xhci` + `usb-kbd` path.
//!
//! `devd` grants BAR0, the function configuration page, a DMA buffer and the legacy IRQ. The
//! driver maps the BAR/DMA, initializes the controller through [`raven_xhci`], enumerates the
//! keyboard with the S61 USB core, configures the interrupt IN endpoint, fetches the HID report
//! descriptor and hands the transport to the S62 [`usb_hid::KeyboardService`]. The service selects
//! the boot protocol and primes one interrupt-IN transfer.
//!
//! The `serve` argument (the release driver table uses it) keeps the keyboard service alive for
//! the console session and pushes every decoded byte to the `usb-hid` `kbd:` scheme that `consd`
//! reads. Without `serve` the driver keeps the S67 one-shot enumeration transcript.
//!
//! S67 proves enumeration and the transport seam on QEMU; key injection and console/TTY delivery
//! are S63. The Pi/RP1 DWC3 register offset remains UNVERIFIED.

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

    use raven_abi::types::Handle;
    use raven_driver::{ConfigRegion, DmaRegion, Irq, MmioRegion};
    use raven_rt::entry;
    use raven_rt::println;
    use raven_scheme::Client;
    use raven_usb_core::descriptor::{Direction as EndpointDirection, TransferType};
    use raven_usb_core::{
        AddressPool, ControlError, ControlTransport, Enumerator, SetupPacket, UsbError,
    };
    use raven_usb_hid::parse_hid_descriptor;
    use raven_xhci::{ControlDirection, Controller, MIN_DMA_LEN, Seam, XhciError};
    use usb_hid::{Error as HidError, KeyboardService, SetupPacket as HidSetup, TransferRing};

    entry!(main);

    /// `devd` grants resources in this order after the inherited handles.
    const BAR_HANDLE: Handle = Handle::new(3, 0);
    const CONFIG_HANDLE: Handle = Handle::new(4, 0);
    const DMA_HANDLE: Handle = Handle::new(5, 0);
    const IRQ_HANDLE: Handle = Handle::new(6, 0);

    /// Wire open flag for write access to the `kbd:` console queue.
    const OPEN_WRITE: u32 = 2;
    /// Bounded retries while the `usb-hid` service has not registered `kbd:` yet.
    const PUSH_ATTEMPTS: u8 = 5;
    /// Backoff between `kbd:` open attempts.
    const PUSH_RETRY_NS: u64 = 10_000_000;
    /// Idle pause between service-loop iterations.
    const SERVE_TICK_NS: u64 = 10_000_000;

    fn main(args: raven_rt::Args) -> i32 {
        match run(args) {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("xhci: {}", message);
                1
            }
        }
    }

    fn run(args: raven_rt::Args) -> Result<(), String> {
        let (bar_size, dma_size, serve) = parse_args(args)?;
        if bar_size == 0 || dma_size < MIN_DMA_LEN {
            return Err("devd passed no usable BAR/DMA buffer".to_string());
        }

        let bar_window = raven_rt::map_mmio_mut(BAR_HANDLE, bar_size)
            .map_err(|error| format!("BAR map: {}", error.description()))?;
        let config_window = raven_rt::map_mmio(CONFIG_HANDLE, raven_driver::PAGE)
            .map_err(|error| format!("config map: {}", error.description()))?;
        let config = ConfigRegion::new(config_window);
        let vendor = config
            .read_u16(0)
            .map_err(|_| "config vendor".to_string())?;
        let device = config
            .read_u16(2)
            .map_err(|_| "config device".to_string())?;
        let dma_info = raven_rt::dma_info(DMA_HANDLE)
            .map_err(|error| format!("DMA info: {}", error.description()))?;
        let dma_window = raven_rt::map_mmio_mut(DMA_HANDLE, dma_size)
            .map_err(|error| format!("DMA map: {}", error.description()))?;
        let seam = RealSeam::new(
            MmioRegion::new(bar_window),
            DmaRegion::new(dma_window, dma_info.bus_addr),
            Irq::new(IRQ_HANDLE),
        );
        println!(
            "xhci: bar=0 size={} dma={} bus={:#x} vendor={:#06x} device={:#06x}",
            bar_size, dma_size, dma_info.bus_addr, vendor, device
        );

        let mut controller = Controller::new(seam).map_err(xhci_text)?;
        let capability = controller.capability();
        println!(
            "xhci: version={:#06x} slots={} ports={} csz={}",
            capability.hci_version,
            capability.max_slots,
            capability.max_ports,
            u8::from(capability.csz)
        );
        controller.init().map_err(xhci_text)?;
        let port = controller.wait_for_port(2000).map_err(xhci_text)?;
        let speed = controller.reset_port(port).map_err(xhci_text)?;
        println!("xhci: port={port} speed={speed}");
        let slot = controller.enable_slot().map_err(xhci_text)?;
        controller
            .address_device(slot, port, speed, 8)
            .map_err(xhci_text)?;
        println!("xhci: slot={slot} addressed");

        let mut pool = AddressPool::new();
        let mut enumerator = Enumerator::new();
        {
            let mut transport = ControlAdapter {
                controller: &mut controller,
            };
            enumerator
                .enumerate(&mut transport, &mut pool)
                .map_err(usb_text)?;
        }
        let descriptor = enumerator.device().copied().ok_or("no device descriptor")?;
        let configuration = enumerator
            .configuration()
            .ok_or("no active configuration")?;
        println!(
            "xhci: device vendor={:#06x} product={:#06x} class={} configs={} interfaces={}",
            descriptor.vendor_id,
            descriptor.product_id,
            descriptor.class,
            descriptor.num_configurations,
            configuration.interfaces.len()
        );
        let interface = configuration
            .interfaces
            .iter()
            .find(|interface| interface.descriptor.class == 3)
            .ok_or("no HID interface")?;
        let report_length = interface
            .class_specific
            .iter()
            .find_map(|raw| {
                (raw.get(1).copied() == Some(0x21))
                    .then(|| parse_hid_descriptor(raw).ok())
                    .flatten()
                    .map(|descriptor| descriptor.report_length)
            })
            .ok_or("no HID report descriptor")?;
        let endpoint = interface
            .endpoints
            .iter()
            .find(|endpoint| {
                endpoint.address.direction == EndpointDirection::In
                    && endpoint.transfer_type() == Some(TransferType::Interrupt)
            })
            .ok_or("no interrupt IN endpoint")?;
        controller
            .configure_interrupt(
                slot,
                port,
                speed,
                endpoint.max_packet_size,
                endpoint.interval,
            )
            .map_err(xhci_text)?;
        println!(
            "xhci: hid interface={} report={} ep={:#04x}",
            interface.descriptor.number,
            report_length,
            endpoint.address.to_byte()
        );

        let setup = SetupPacket {
            request_type: 0x81,
            request: 6,
            value: 0x2200,
            index: u16::from(interface.descriptor.number),
            length: report_length,
        };
        let mut report = vec![0u8; usize::from(report_length)];
        let got = controller
            .control(setup.to_bytes(), &mut report, ControlDirection::In)
            .map_err(xhci_text)?;
        if got < report.len() {
            report.truncate(got);
        }
        let transport = HidTransport { controller };
        let mut service = KeyboardService::new(transport);
        service.init(&report).map_err(hid_text)?;
        println!("usb-keyboard: boot protocol ok");
        if serve {
            return serve_console(&mut service);
        }
        let mut queued = 0usize;
        for _ in 0..8 {
            queued = queued.saturating_add(service.poll().map_err(hid_text)?);
            if service.console_len() >= 4 {
                break;
            }
        }
        let mut console = [0u8; 8];
        let count = service.read_console(&mut console);
        println!("usb-keyboard: enumerated keys={queued}");
        println!("usb-keyboard: console hex={:02x?}", &console[..count]);
        println!("usb-keyboard: ok");
        Ok(())
    }

    /// Keep the keyboard service alive and feed decoded bytes to the `kbd:` console queue.
    ///
    /// `consd` owns the console multiplexer and reads `kbd:console`; the driver is only the
    /// producer. A missing `usb-hid` service is retried for a bounded time and then the bytes are
    /// dropped locally, exactly like the Pi 4 bridge: the UART console must never depend on a
    /// keyboard.
    fn serve_console(service: &mut KeyboardService<HidTransport>) -> Result<(), String> {
        println!("usb-keyboard: serving");
        let mut pushed = 0usize;
        loop {
            service.poll().map_err(hid_text)?;
            let mut console = [0u8; 8];
            let count = service.read_console(&mut console);
            if count > 0 {
                if pushed == 0 {
                    println!("usb-keyboard: first console bytes len={count}");
                }
                pushed = pushed.saturating_add(1);
                push_console(&console[..count]);
            }
            let _ = raven_rt::nanosleep(SERVE_TICK_NS);
        }
    }

    /// Push decoded ASCII bytes to the `usb-hid` `kbd:` service.
    fn push_console(bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }
        let client = Client::new();
        for _ in 0..PUSH_ATTEMPTS {
            if let Ok(handle) = client.open("kbd:console", OPEN_WRITE) {
                let _ = client.write(handle, 0, bytes);
                let _ = client.close(handle);
                return;
            }
            let _ = raven_rt::nanosleep(PUSH_RETRY_NS);
        }
    }

    fn parse_args(args: raven_rt::Args) -> Result<(usize, usize, bool), String> {
        let mut bar_size = 0usize;
        let mut dma_size = 0usize;
        let mut serve = false;
        for argument in args.iter().skip(1) {
            if let Some(value) = argument.strip_prefix("bar_size=") {
                bar_size = value
                    .parse()
                    .map_err(|_| format!("bad bar_size `{argument}`"))?;
            } else if let Some(value) = argument.strip_prefix("dma=") {
                dma_size = value.parse().map_err(|_| format!("bad dma `{argument}`"))?;
            } else if argument == "serve" {
                serve = true;
            }
        }
        Ok((bar_size, dma_size, serve))
    }

    struct RealSeam {
        mmio: MmioRegion,
        dma: DmaRegion,
        irq: Irq,
    }

    impl RealSeam {
        fn new(mmio: MmioRegion, dma: DmaRegion, irq: Irq) -> Self {
            Self { mmio, dma, irq }
        }
    }

    impl Seam for RealSeam {
        fn read32(&mut self, offset: usize) -> Result<u32, XhciError> {
            self.mmio.read_u32(offset).map_err(|_| XhciError::Access)
        }
        fn write32(&mut self, offset: usize, value: u32) -> Result<(), XhciError> {
            self.mmio
                .write_u32(offset, value)
                .map_err(|_| XhciError::Access)
        }
        fn read64(&mut self, offset: usize) -> Result<u64, XhciError> {
            self.mmio.read_u64(offset).map_err(|_| XhciError::Access)
        }
        fn write64(&mut self, offset: usize, value: u64) -> Result<(), XhciError> {
            self.mmio
                .write_u64(offset, value)
                .map_err(|_| XhciError::Access)
        }
        fn dma_bus(&self) -> u64 {
            self.dma.bus_address()
        }
        fn dma_len(&self) -> usize {
            self.dma.len()
        }
        fn dma_read_u32(&self, offset: usize) -> Result<u32, XhciError> {
            self.dma.read_u32(offset).map_err(|_| XhciError::Access)
        }
        fn dma_write_u32(&mut self, offset: usize, value: u32) -> Result<(), XhciError> {
            self.dma
                .write_u32(offset, value)
                .map_err(|_| XhciError::Access)
        }
        fn dma_read_u64(&self, offset: usize) -> Result<u64, XhciError> {
            self.dma.read_u64(offset).map_err(|_| XhciError::Access)
        }
        fn dma_write_u64(&mut self, offset: usize, value: u64) -> Result<(), XhciError> {
            self.dma
                .write_u64(offset, value)
                .map_err(|_| XhciError::Access)
        }
        fn dma_read_bytes(&self, offset: usize, out: &mut [u8]) -> Result<(), XhciError> {
            self.dma
                .read_bytes(offset, out)
                .map_err(|_| XhciError::Access)
        }
        fn dma_write_bytes(&mut self, offset: usize, data: &[u8]) -> Result<(), XhciError> {
            self.dma
                .write_bytes(offset, data)
                .map_err(|_| XhciError::Access)
        }
        fn publish(&self) {
            self.dma.publish();
        }
        fn acquire(&self) {
            self.dma.acquire();
        }
        fn delay_us(&mut self, micros: u32) {
            let _ = raven_rt::nanosleep(u64::from(micros) * 1000);
        }
        fn wait_irq(&mut self, timeout_ns: u64) {
            if let Ok(count) = self.irq.wait(timeout_ns)
                && count > 0
            {
                let _ = self.irq.ack();
            }
        }
    }

    struct ControlAdapter<'a> {
        controller: &'a mut Controller<RealSeam>,
    }

    impl ControlTransport for ControlAdapter<'_> {
        fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
            if setup.request == raven_usb_core::control::request::SET_ADDRESS && setup.is_standard()
            {
                return Ok(0);
            }
            let length = usize::from(setup.length);
            if data.len() != length {
                return Err(UsbError::Control(ControlError::LengthMismatch {
                    expected: length,
                    got: data.len(),
                }));
            }
            let direction = match setup.direction() {
                raven_usb_core::Direction::In => ControlDirection::In,
                raven_usb_core::Direction::Out if length == 0 => ControlDirection::NoData,
                raven_usb_core::Direction::Out => ControlDirection::Out,
            };
            self.controller
                .control(setup.to_bytes(), data, direction)
                .map_err(usb_control)
        }
    }

    struct HidTransport {
        controller: Controller<RealSeam>,
    }

    impl TransferRing for HidTransport {
        fn control_out(&mut self, setup: HidSetup, data: &[u8]) -> Result<(), HidError> {
            let mut buffer = [0u8; 8];
            let take = data.len().min(buffer.len());
            if data.len() > buffer.len() {
                return Err(HidError::NoDevice);
            }
            buffer[..take].copy_from_slice(&data[..take]);
            let direction = if take == 0 {
                ControlDirection::NoData
            } else {
                ControlDirection::Out
            };
            self.controller
                .control(setup.encode(), &mut buffer[..take], direction)
                .map(|_| ())
                .map_err(|_| HidError::NoDevice)
        }

        fn control_in(&mut self, setup: HidSetup, data: &mut [u8]) -> Result<usize, HidError> {
            self.controller
                .control(setup.encode(), data, ControlDirection::In)
                .map_err(|_| HidError::NoDevice)
        }

        fn interrupt_in(&mut self, _endpoint: u8, length: usize) -> Result<(), HidError> {
            let length = u8::try_from(length).map_err(|_| HidError::NoDevice)?;
            self.controller
                .interrupt_in(length)
                .map_err(|_| HidError::NoDevice)
        }

        fn next_report(&mut self) -> Option<[u8; 8]> {
            let event = self.controller.wait_interrupt().ok()?;
            if event.completion_code() != 1 && event.completion_code() != 13 {
                return None;
            }
            let mut report = [0u8; 8];
            self.controller.read_interrupt(&mut report).ok()?;
            Some(report)
        }
    }

    fn usb_control(error: XhciError) -> UsbError {
        if error == XhciError::Completion(6) {
            UsbError::Control(ControlError::Stall)
        } else {
            UsbError::Control(ControlError::Transport)
        }
    }

    fn xhci_text(error: XhciError) -> String {
        error.to_string()
    }

    fn usb_text(error: UsbError) -> String {
        error.to_string()
    }

    fn hid_text(error: HidError) -> String {
        match error {
            HidError::TooShort => "HID descriptor too short".to_string(),
            HidError::BadItem => "HID descriptor item is bad".to_string(),
            HidError::NotAKeyboard => "HID device is not a boot keyboard".to_string(),
            HidError::ShortReport => "HID report is short".to_string(),
            HidError::NotInitialized => "HID service is not initialized".to_string(),
            HidError::NoDevice => "HID transport failed".to_string(),
        }
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
