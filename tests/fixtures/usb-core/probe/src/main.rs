// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/init` for the `usb-core` qtest fixture.
//!
//! Exercises the S61 host-testable core inside the Raven guest: the synthetic
//! USB 2.0 descriptor set, address assignment, `GET_DESCRIPTOR`/`SET_ADDRESS`
//! control requests, full enumeration to `Configured`, endpoint decoding and
//! the hub port state machine. It does not touch xHCI hardware or a real USB
//! device.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::println;
    use raven_usb_core::{
        AddressPool, ControlTransport, DescriptorType, EndpointAddress, EnumerationState,
        Enumerator, Hub, HubEvent, HubPortState, PortSpeed, SetupPacket, StandardRequest, UsbError,
        get_descriptor,
    };

    entry!(main);

    /// Synthetic device descriptor from `tests/fixtures/usb-core/synthetic-device.txt`.
    const DEVICE: [u8; 18] = [
        0x12, 0x01, 0x00, 0x02, 0x00, 0x00, 0x00, 0x40, 0x04, 0x1d, 0x01, 0x00, 0x00, 0x01, 0x01,
        0x02, 0x03, 0x01,
    ];

    /// Synthetic configuration set: one interface and one interrupt IN endpoint.
    const CONFIG: [u8; 25] = [
        0x09, 0x02, 0x19, 0x00, 0x01, 0x01, 0x00, 0x80, 0x32, 0x09, 0x04, 0x00, 0x00, 0x01, 0xff,
        0x01, 0x02, 0x00, 0x07, 0x05, 0x81, 0x03, 0x08, 0x00, 0x0a,
    ];

    struct Synthetic {
        address: u8,
        configuration: u8,
        controls: u32,
    }

    impl Synthetic {
        const fn new() -> Self {
            Self {
                address: 0,
                configuration: 0,
                controls: 0,
            }
        }

        fn copy(source: &[u8], data: &mut [u8]) -> usize {
            let length = core::cmp::min(source.len(), data.len());
            if let Some(destination) = data.get_mut(..length) {
                destination.copy_from_slice(source.get(..length).unwrap_or(&[]));
            }
            length
        }
    }

    impl ControlTransport for Synthetic {
        fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
            self.controls = self.controls.saturating_add(1);
            match setup.request {
                6 => match setup.value >> 8 {
                    1 => Ok(Self::copy(&DEVICE, data)),
                    2 => Ok(Self::copy(&CONFIG, data)),
                    _ => Ok(0),
                },
                5 => {
                    self.address = (setup.value & 0x7f) as u8;
                    Ok(0)
                }
                9 => {
                    self.configuration = (setup.value & 0xff) as u8;
                    Ok(0)
                }
                _ => Ok(0),
            }
        }
    }

    fn main(_args: raven_rt::Args) -> i32 {
        println!("usb-core: start");
        match run() {
            Ok(()) => {
                println!("usb-core: pass");
                0
            }
            Err(message) => {
                println!("usb-core: fail {message}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let mut transport = Synthetic::new();
        let mut pool = AddressPool::new();
        let mut enumerator = Enumerator::new();
        enumerator
            .enumerate(&mut transport, &mut pool)
            .map_err(|_| "enumerate")?;
        if enumerator.state() != EnumerationState::Configured {
            return Err("state");
        }
        let device = enumerator.device().ok_or("device")?;
        let configuration = enumerator.configuration().ok_or("configuration")?;
        let interface = enumerator.interface(0, 0).ok_or("interface")?;
        let endpoint = interface.endpoints.first().ok_or("endpoint")?;
        if endpoint.address != EndpointAddress::from_byte(0x81).ok_or("endpoint address")? {
            return Err("endpoint mismatch");
        }
        println!(
            "usb-core: descriptors vendor=0x{:04x} product=0x{:04x} configs={}",
            device.vendor_id, device.product_id, device.num_configurations
        );
        println!(
            "usb-core: config value={} interfaces={} endpoints={}",
            configuration.header.configuration_value,
            configuration.header.num_interfaces,
            interface.endpoints.len()
        );
        println!(
            "usb-core: endpoint address=0x{:02x} dci={}",
            endpoint.address.to_byte(),
            endpoint.dci()
        );
        println!(
            "usb-core: address assigned={} pool={}",
            enumerator.address().unwrap_or(0),
            pool.assigned()
        );
        if pool.assigned() != 1 || enumerator.address() != Some(1) {
            return Err("address pool");
        }
        enumerator.release(&mut pool).map_err(|_| "release")?;
        if pool.assigned() != 0 {
            return Err("address leak");
        }

        let setup = get_descriptor(DescriptorType::Device, 0, 0, 18);
        let bytes = setup.to_bytes();
        let parsed = SetupPacket::parse(&bytes).map_err(|_| "setup parse")?;
        if parsed != setup
            || StandardRequest::from_setup(setup) != Some(StandardRequest::GetDescriptor)
        {
            return Err("setup request");
        }
        println!(
            "usb-core: setup request=0x{:02x} length={}",
            setup.request, setup.length
        );

        let mut hub = Hub::new(1).map_err(|_| "hub")?;
        hub.event(0, HubEvent::Attach(PortSpeed::High))
            .map_err(|_| "attach")?;
        hub.event(0, HubEvent::PowerOn).map_err(|_| "power")?;
        hub.event(0, HubEvent::Reset).map_err(|_| "reset")?;
        hub.event(0, HubEvent::ResetComplete)
            .map_err(|_| "reset complete")?;
        let port = hub.port(0).ok_or("port")?;
        if port.state() != HubPortState::Enabled {
            return Err("port state");
        }
        println!("usb-core: hub port=0 state=enabled");
        hub.event(0, HubEvent::Detach).map_err(|_| "detach")?;
        if hub.port(0).map(|port| port.state()) != Some(HubPortState::Disconnected) {
            return Err("detach state");
        }
        println!("usb-core: hub detach ok");
        Ok(())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
