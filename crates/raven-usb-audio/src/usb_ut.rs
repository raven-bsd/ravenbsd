// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the standard USB descriptors.

use super::*;

fn device() -> DeviceDescriptor {
    DeviceDescriptor {
        usb_version: 0x0200,
        class: 0,
        subclass: 0,
        protocol: 0,
        max_packet0: 64,
        vendor: 0x1234,
        product: 0x5678,
        device_version: 0x0100,
        configurations: 1,
    }
}

#[test]
fn a_device_descriptor_round_trips() {
    let bytes = device().encode();
    assert_eq!(DeviceDescriptor::parse(&bytes), Ok(device()));
    assert_eq!(bytes[0], DEVICE_LEN as u8);
    assert_eq!(bytes[1], TYPE_DEVICE);
}

#[test]
fn malformed_descriptors_are_rejected() {
    let mut bytes = device().encode();
    bytes[0] = 17;
    assert_eq!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::BadLength {
            got: 17,
            expected: 18
        })
    );
    let mut bytes = device().encode();
    bytes[1] = TYPE_CONFIG;
    assert_eq!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::BadType {
            got: TYPE_CONFIG,
            expected: TYPE_DEVICE
        })
    );
    assert_eq!(
        DeviceDescriptor::parse(&[0u8; 10]),
        Err(DescriptorError::TooShort { need: 18, got: 10 })
    );
}

#[test]
fn a_configuration_header_round_trips() {
    let config = ConfigDescriptor {
        total_length: 0x0040,
        interfaces: 2,
        value: 1,
        attributes: 0x80,
        max_power: 250,
    };
    assert_eq!(ConfigDescriptor::parse(&config.encode()), Ok(config));
}

#[test]
fn an_interface_round_trips() {
    let interface = InterfaceDescriptor {
        number: 2,
        alternate: 1,
        endpoints: 1,
        class: 0x01,
        subclass: 0x02,
        protocol: 0,
    };
    assert_eq!(
        InterfaceDescriptor::parse(&interface.encode()),
        Ok(interface)
    );
}

#[test]
fn an_endpoint_round_trips_and_decodes_its_type() {
    let endpoint = EndpointDescriptor {
        address: EndpointAddress::new(1, Direction::In).expect("address"),
        attributes: 0b0000_1101,
        max_packet: 0x00C0,
        interval: 1,
    };
    let parsed = EndpointDescriptor::parse(&endpoint.encode()).expect("endpoint");
    assert_eq!(parsed, endpoint);
    assert_eq!(parsed.transfer_type(), TransferType::Isochronous);
    assert_eq!(parsed.sync_type(), SyncType::Synchronous);
    assert_eq!(parsed.usage_type(), UsageType::Data);
    assert_eq!(parsed.max_packet_size(), 0x00C0);
}

#[test]
fn the_endpoint_usage_bits_are_decoded() {
    let feedback = EndpointDescriptor {
        address: EndpointAddress::new(1, Direction::In).expect("address"),
        attributes: 0b0001_0001,
        max_packet: 4,
        interval: 1,
    };
    assert_eq!(feedback.transfer_type(), TransferType::Isochronous);
    assert_eq!(feedback.sync_type(), SyncType::None);
    assert_eq!(feedback.usage_type(), UsageType::Feedback);
}

#[test]
fn endpoint_addresses_carry_the_direction_and_dci() {
    let in_one = EndpointAddress::new(1, Direction::In).expect("address");
    assert_eq!(in_one.to_byte(), 0x81);
    assert_eq!(in_one.dci(), 3);
    let out_two = EndpointAddress::new(2, Direction::Out).expect("address");
    assert_eq!(out_two.to_byte(), 0x02);
    assert_eq!(out_two.dci(), 4);
    assert!(EndpointAddress::new(0, Direction::In).is_none());
    assert!(EndpointAddress::new(16, Direction::In).is_none());
    assert_eq!(EndpointAddress::from_byte(0x81), Some(in_one));
}

#[test]
fn the_setup_packet_round_trips() {
    let setup = SetupPacket {
        request_type: 0x21,
        request: 0x01,
        value: 0x0201,
        index: 0x0000,
        length: 0x0001,
    };
    assert_eq!(SetupPacket::from_bytes(setup.to_bytes()), setup);
    assert_eq!(setup.direction(), Direction::Out);
    assert!(setup.is_class_interface());
    assert!(!setup.to_bytes().is_empty());
}

#[test]
fn a_read_setup_is_class_interface_with_in_direction() {
    let setup = SetupPacket {
        request_type: 0xA1,
        request: 0x81,
        value: 0x0201,
        index: 0,
        length: 1,
    };
    assert_eq!(setup.direction(), Direction::In);
    assert!(setup.is_class_interface());
}
