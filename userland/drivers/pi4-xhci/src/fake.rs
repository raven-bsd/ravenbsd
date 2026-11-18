// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The fake Pi 4 xHCI path used by the `pi4-usb` qtest.
//!
//! A synthetic keyboard device answers the S61 enumeration requests and the S62 boot-keyboard
//! service. The controller side runs one Setup/Data/Status sequence through the D1
//! `raven-pi4-xhci` seam first, so the model's ring/event code is exercised by the driver.

use alloc::collections::VecDeque;
use alloc::format;
use alloc::string::{String, ToString};

use raven_pi4_xhci::trb::CompletionCode;
use raven_pi4_xhci::{DmaBuffer, EventRing, Host, Transfer, Trb, XhciSeam};
use raven_usb_core::descriptor::Direction as EndpointDirection;
use raven_usb_core::{
    AddressPool, ControlError, ControlTransport, Enumerator, SetupPacket, UsbError,
};
use usb_hid::{
    BOOT_KEYBOARD_DESCRIPTOR, Error as HidError, KeyboardService, SetupPacket as HidSetup,
    TransferRing,
};

const DEVICE_DESCRIPTOR: [u8; 18] = [
    0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40, 0x27, 0x06, 0x01, 0x00, 0x00, 0x01, 0x01, 0x02,
    0x03, 0x01,
];

const CONFIGURATION: [u8; 34] = [
    0x09, 0x02, 0x22, 0x00, 0x01, 0x01, 0x00, 0x80, 0x32, // config
    0x09, 0x04, 0x00, 0x00, 0x01, 0x03, 0x01, 0x01, 0x00, // HID boot keyboard
    0x09, 0x21, 0x11, 0x01, 0x00, 0x01, 0x22, 0x3f, 0x00, // HID descriptor, 63-byte report
    0x07, 0x05, 0x81, 0x03, 0x08, 0x00, 0x0a, // EP1 IN, interrupt, 8 bytes
];

const KEY_A: u8 = 0x04;

struct FakeHost {
    events: VecDeque<Trb>,
    allocated: usize,
    freed: usize,
    doorbells: usize,
    irqs: usize,
}

impl FakeHost {
    fn new() -> Self {
        Self {
            events: VecDeque::new(),
            allocated: 0,
            freed: 0,
            doorbells: 0,
            irqs: 0,
        }
    }
}

impl Host for FakeHost {
    fn allocate_dma(&mut self, len: usize, _align: usize) -> Option<DmaBuffer> {
        self.allocated += 1;
        Some(DmaBuffer {
            cpu: 0x1000,
            bus: 0x2000,
            len,
        })
    }

    fn free_dma(&mut self, _buffer: DmaBuffer) {
        self.freed += 1;
    }

    fn doorbell(&mut self, _slot: u8, _target: u8) {
        self.doorbells += 1;
    }

    fn wait_event(&mut self, events: &mut EventRing, _timeout_ns: u64) -> Option<Trb> {
        let event = self.events.pop_front()?;
        events.push(event).ok()?;
        events.pop()
    }

    fn acknowledge_irq(&mut self) {
        self.irqs += 1;
    }
}

/// One synthetic boot keyboard behind the S61 control and S62 report seams.
struct FakeKeyboard {
    queued: Option<[u8; 8]>,
    configured: bool,
    protocol: u8,
}

impl FakeKeyboard {
    const fn new() -> Self {
        Self {
            queued: None,
            configured: false,
            protocol: 1,
        }
    }

    fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
        let descriptor = (setup.value >> 8) as u8;
        let response: &[u8] = match setup.request {
            5 => {
                // SET_ADDRESS
                &[]
            }
            6 => match descriptor {
                1 => &DEVICE_DESCRIPTOR,
                2 => &CONFIGURATION,
                0x22 => &BOOT_KEYBOARD_DESCRIPTOR,
                _ => return Err(stall()),
            },
            8 => {
                let value = u8::from(self.configured);
                if let Some(slot) = data.get_mut(0) {
                    *slot = value;
                }
                return Ok(1);
            }
            9 => {
                self.configured = setup.value != 0;
                &[]
            }
            0x0a => &[],
            0x0b => {
                self.protocol = setup.value as u8;
                &[]
            }
            _ => return Err(stall()),
        };
        let length = usize::from(setup.length).min(response.len());
        let target = data.get_mut(..length).ok_or_else(stall)?;
        target.copy_from_slice(response.get(..length).ok_or_else(stall)?);
        Ok(length)
    }

    fn hid_control(&mut self, setup: HidSetup, data: &mut [u8]) -> Result<usize, UsbError> {
        let core = SetupPacket {
            request_type: setup.request_type,
            request: setup.request,
            value: setup.value,
            index: setup.index,
            length: setup.length,
        };
        self.control(core, data)
    }
}

fn stall() -> UsbError {
    UsbError::Control(ControlError::Stall)
}

fn hid_stall() -> HidError {
    HidError::NoDevice
}

impl ControlTransport for FakeKeyboard {
    fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
        FakeKeyboard::control(self, setup, data)
    }
}

impl TransferRing for FakeKeyboard {
    fn control_out(&mut self, setup: HidSetup, data: &[u8]) -> Result<(), HidError> {
        let mut buffer = [0u8; 8];
        let length = data.len().min(buffer.len());
        buffer
            .get_mut(..length)
            .ok_or_else(hid_stall)?
            .copy_from_slice(data.get(..length).ok_or_else(hid_stall)?);
        self.hid_control(setup, &mut buffer[..length])
            .map(|_| ())
            .map_err(|_| hid_stall())
    }

    fn control_in(&mut self, setup: HidSetup, data: &mut [u8]) -> Result<usize, HidError> {
        self.hid_control(setup, data).map_err(|_| hid_stall())
    }

    fn interrupt_in(&mut self, _endpoint: u8, _length: usize) -> Result<(), HidError> {
        let mut report = [0u8; 8];
        if let Some(key) = report.get_mut(2) {
            *key = KEY_A;
        }
        self.queued = Some(report);
        Ok(())
    }

    fn next_report(&mut self) -> Option<[u8; 8]> {
        self.queued.take()
    }
}

/// Run the D1 ring check and the S61/S62 fake keyboard path.
pub fn run() -> Result<(), String> {
    ring_check()?;
    keyboard_check()?;
    Ok(())
}

fn ring_check() -> Result<(), String> {
    let mut host = FakeHost::new();
    host.events
        .push_back(Trb::transfer_event(1, 1, CompletionCode::Success, 0));
    let mut seam = XhciSeam::new(host).map_err(|error| error.to_string())?;
    let trbs = [
        Trb::setup(0x1122_3344_5566_7788),
        Trb::data(0x2000, 0),
        Trb::status(0),
    ];
    let completion = seam
        .submit(
            Transfer {
                slot: 1,
                endpoint: 1,
                trbs: &trbs,
            },
            1_000_000,
        )
        .map_err(|error| error.to_string())?;
    if completion.code != CompletionCode::Success {
        return Err("xhci ring did not complete".into());
    }
    raven_rt::println!("pi4-xhci: ring trbs={} completion=success", trbs.len());
    Ok(())
}

fn keyboard_check() -> Result<(), String> {
    let mut keyboard = FakeKeyboard::new();
    let mut enumerator = Enumerator::new();
    let mut pool = AddressPool::new();
    enumerator
        .enumerate(&mut keyboard, &mut pool)
        .map_err(|error| error.to_string())?;
    let device = enumerator.device().copied().ok_or("no device descriptor")?;
    let configuration = enumerator
        .configuration()
        .ok_or("no active configuration")?;
    raven_rt::println!(
        "pi4-xhci: device vendor={:#06x} product={:#06x} class={} configs={} interfaces={}",
        device.vendor_id,
        device.product_id,
        device.class,
        device.num_configurations,
        configuration.interfaces.len()
    );
    let interface = configuration
        .interfaces
        .iter()
        .find(|interface| interface.descriptor.class == 3)
        .ok_or("no HID interface")?;
    let endpoint = interface
        .endpoints
        .iter()
        .find(|endpoint| endpoint.address.direction == EndpointDirection::In)
        .ok_or("no interrupt IN endpoint")?;
    raven_rt::println!(
        "pi4-xhci: hid interface={} report={} ep={:#04x}",
        interface.descriptor.number,
        BOOT_KEYBOARD_DESCRIPTOR.len(),
        endpoint.address.to_byte()
    );

    let mut service = KeyboardService::new(keyboard);
    service
        .init(&BOOT_KEYBOARD_DESCRIPTOR)
        .map_err(|error| format!("{error:?}"))?;
    raven_rt::println!("pi4-xhci: keyboard boot protocol ok");
    let queued = service.poll().map_err(|error| format!("{error:?}"))?;
    let mut buffer = [0u8; 8];
    let read = service.read_console(&mut buffer);
    crate::kbd::push(buffer.get(..read).unwrap_or(&[]));
    let key = core::str::from_utf8(buffer.get(..read).unwrap_or(&[])).unwrap_or("?");
    if queued != 1 || read != 1 || key != "a" {
        return Err("fake keyboard did not decode a key".into());
    }
    raven_rt::println!("pi4-xhci: key={key}");
    raven_rt::println!("pi4-xhci: ok");
    Ok(())
}
