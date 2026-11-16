// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions for the public USB-core surface.

use alloc::vec::Vec;

use super::*;

fn device_bytes() -> [u8; DEVICE_LEN] {
    [
        18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x04, 0x1d, 0x01, 0x00, 0x00, 0x01, 1, 2, 3, 1,
    ]
}

fn config_bytes() -> Vec<u8> {
    let total: u16 = 25;
    let mut out = Vec::new();
    out.extend_from_slice(&[9, 2]);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&[1, 1, 0, 0x80, 50]);
    out.extend_from_slice(&[9, 4, 0, 0, 1, 0xff, 0x01, 0x02, 0]);
    out.extend_from_slice(&[7, 5, 0x81, 0x03, 8, 0, 10]);
    out
}

fn hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex fixture");
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let mut index = 0;
    while index < text.len() {
        let pair = text.get(index..index + 2).expect("hex pair");
        bytes.push(u8::from_str_radix(pair, 16).expect("hex digit"));
        index += 2;
    }
    bytes
}

fn fixture() -> (Vec<u8>, Vec<u8>) {
    let text = include_str!("../../../tests/fixtures/usb-core/synthetic-device.txt");
    let mut device = Vec::new();
    let mut config = Vec::new();
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        match fields.next() {
            Some("device") => device = hex(fields.next().expect("device hex")),
            Some("config") => config = hex(fields.next().expect("config hex")),
            _ => {}
        }
    }
    (device, config)
}

#[test]
fn the_synthetic_fixture_matches_the_public_parsers() {
    let (device, config) = fixture();
    let descriptor = DeviceDescriptor::parse(&device).expect("fixture device");
    let configuration = Configuration::parse(&config).expect("fixture configuration");
    assert_eq!(descriptor.vendor_id, 0x1d04);
    assert_eq!(configuration.header.configuration_value, 1);
    assert_eq!(configuration.interfaces.len(), 1);
    assert_eq!(configuration.interfaces[0].endpoints.len(), 1);
    assert_eq!(
        configuration.interfaces[0].endpoints[0].transfer_type(),
        Some(TransferType::Interrupt)
    );
}

#[test]
fn the_public_types_compose() {
    let descriptor = DeviceDescriptor::parse(&device_bytes()).expect("device");
    let configuration = Configuration::parse(&config_bytes()).expect("configuration");
    let device = Device::new(descriptor, vec![configuration]).expect("device aggregate");
    assert_eq!(device.descriptor.num_configurations, 1);
    assert_eq!(
        device
            .configuration(1)
            .and_then(|configuration| configuration.interface(0, 0))
            .map(|interface| interface.descriptor.class),
        Some(0xff)
    );

    let mut pool = AddressPool::new();
    assert_eq!(pool.assign(), Ok(1));
    let mut hub = Hub::new(2).expect("hub");
    hub.event(0, HubEvent::Attach(PortSpeed::High))
        .expect("attach");
    assert_eq!(hub.enabled(), 0);
    let seam = XhciSeam::new(8, 8, 1, 1).expect("seam");
    assert_eq!(seam.slot(), 1);
    assert_eq!(seam.endpoint(), 1);

    let setup = get_descriptor(DescriptorType::Device, 0, 0, DEVICE_LEN as u16);
    assert_eq!(
        standard_request(setup),
        Some(StandardRequest::GetDescriptor)
    );
    assert_eq!(setup.direction(), Direction::In);
}
