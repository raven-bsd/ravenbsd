// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the standard USB descriptors.

use super::*;

fn device_bytes() -> [u8; DEVICE_LEN] {
    [
        18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x04, 0x1d, 0x01, 0x00, 0x00, 0x01, 1, 2, 3, 1,
    ]
}

fn config_bytes() -> Vec<u8> {
    let total: u16 = 52;
    let mut out = Vec::new();
    out.extend_from_slice(&[9, TYPE_CONFIG]);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&[1, 1, 0, 0x80, 50]);
    // Interface 0, alternate 0, two endpoints.
    out.extend_from_slice(&[9, TYPE_INTERFACE, 0, 0, 2, 0xff, 0x01, 0x02, 0]);
    out.extend_from_slice(&[7, TYPE_ENDPOINT, 0x81, 0x03, 8, 0, 10]);
    out.extend_from_slice(&[7, TYPE_ENDPOINT, 0x02, 0x02, 64, 0, 0]);
    // A vendor class-specific record after the endpoints.
    out.extend_from_slice(&[4, 0x30, 0xaa, 0xbb]);
    // Interface 0, alternate 1, one endpoint.
    out.extend_from_slice(&[9, TYPE_INTERFACE, 0, 1, 1, 0xff, 0x01, 0x02, 0]);
    out.extend_from_slice(&[7, TYPE_ENDPOINT, 0x83, 0x01, 16, 0, 1]);
    out
}

#[test]
fn the_device_descriptor_round_trips() {
    let descriptor = DeviceDescriptor::parse(&device_bytes()).expect("device");
    assert_eq!(descriptor.usb_version, 0x0200);
    assert_eq!(descriptor.max_packet_size_0, 64);
    assert_eq!(descriptor.vendor_id, 0x1d04);
    assert_eq!(descriptor.product_id, 0x0001);
    assert_eq!(descriptor.num_configurations, 1);
    assert_eq!(descriptor.encode(), device_bytes());
}

#[test]
fn the_configuration_parses_interfaces_endpoints_and_class_records() {
    let bytes = config_bytes();
    assert_eq!(bytes.len(), 52);
    let configuration = Configuration::parse(&bytes).expect("configuration");
    assert_eq!(configuration.header.total_length, 52);
    assert_eq!(configuration.header.num_interfaces, 1);
    assert_eq!(configuration.header.configuration_value, 1);
    assert_eq!(configuration.interfaces.len(), 2);

    let first = configuration.interface(0, 0).expect("interface 0.0");
    assert_eq!(first.descriptor.class, 0xff);
    assert_eq!(first.endpoints.len(), 2);
    assert_eq!(first.class_specific, [vec![4, 0x30, 0xaa, 0xbb]]);
    let input = first
        .endpoint(EndpointAddress::new(1, Direction::In).expect("address"))
        .expect("IN endpoint");
    assert_eq!(input.transfer_type(), Some(TransferType::Interrupt));
    assert_eq!(input.max_packet_size, 8);
    assert_eq!(input.interval, 10);
    let output = first
        .endpoint(EndpointAddress::new(2, Direction::Out).expect("address"))
        .expect("OUT endpoint");
    assert_eq!(output.transfer_type(), Some(TransferType::Bulk));
    assert_eq!(output.dci(), 4);
    assert_eq!(output.address().to_byte(), 0x02);

    let alternate = configuration.interface(0, 1).expect("interface 0.1");
    assert_eq!(alternate.endpoints.len(), 1);
    assert_eq!(
        alternate.endpoints[0].transfer_type(),
        Some(TransferType::Isochronous)
    );
    assert_eq!(alternate.endpoints[0].dci(), 7);
    assert_eq!(configuration.default_interface(0), Some(first));
}

#[test]
fn endpoint_addresses_and_directions_are_checked() {
    assert_eq!(
        EndpointAddress::new(1, Direction::In).unwrap().to_byte(),
        0x81
    );
    assert_eq!(
        EndpointAddress::new(15, Direction::Out).unwrap().to_byte(),
        0x0f
    );
    assert_eq!(EndpointAddress::new(1, Direction::In).unwrap().dci(), 3);
    assert_eq!(EndpointAddress::new(15, Direction::Out).unwrap().dci(), 30);
    assert!(EndpointAddress::new(0, Direction::In).is_none());
    assert!(EndpointAddress::new(16, Direction::In).is_none());
    assert!(EndpointAddress::from_byte(0x11).is_none());
    assert!(EndpointAddress::from_byte(0x80).is_none());
    assert_eq!(
        EndpointAddress::from_byte(0x82),
        EndpointAddress::new(2, Direction::In)
    );
}

#[test]
fn malformed_device_descriptors_are_rejected() {
    assert_eq!(
        DeviceDescriptor::parse(&[0u8; 17]),
        Err(DescriptorError::TooShort {
            needed: DEVICE_LEN,
            got: 17
        })
    );
    let mut bytes = device_bytes();
    bytes[0] = 0;
    assert!(matches!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::BadLength { .. })
    ));
    let mut bytes = device_bytes();
    bytes[1] = TYPE_CONFIG;
    assert!(matches!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::BadType { .. })
    ));
    let mut bytes = device_bytes();
    bytes[7] = 7;
    assert_eq!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::InvalidMaxPacket0(7))
    );
    let mut bytes = device_bytes();
    bytes[17] = 0;
    assert_eq!(
        DeviceDescriptor::parse(&bytes),
        Err(DescriptorError::ZeroConfigurations)
    );
}

#[test]
fn malformed_configurations_are_rejected() {
    let bytes = config_bytes();
    let mut bad = bytes.clone();
    bad[0] = 8;
    assert!(matches!(
        Configuration::parse(&bad),
        Err(DescriptorError::BadLength { .. })
    ));

    let mut bad = bytes.clone();
    bad[5] = 0;
    assert_eq!(
        Configuration::parse(&bad),
        Err(DescriptorError::ZeroConfigurationValue)
    );

    let mut bad = bytes.clone();
    bad[2] = 8;
    bad[3] = 0;
    assert!(matches!(
        Configuration::parse(&bad),
        Err(DescriptorError::BadTotalLength { .. })
    ));

    // The first interface now declares one endpoint instead of two.
    let mut bad = bytes.clone();
    bad[9 + 4] = 1;
    assert!(matches!(
        Configuration::parse(&bad),
        Err(DescriptorError::EndpointCount {
            interface: 0,
            alternate: 0,
            declared: 1,
            found: 2
        })
    ));

    // Two identical endpoint addresses on the same interface.
    let mut bad = bytes.clone();
    bad[27] = 0x81;
    bad[28] = 0x03;
    assert!(matches!(
        Configuration::parse(&bad),
        Err(DescriptorError::DuplicateEndpoint { .. })
    ));

    // An endpoint before any interface.
    let mut bad = bytes.clone();
    bad.drain(9..18);
    bad[2..4].copy_from_slice(&(43u16).to_le_bytes());
    assert_eq!(
        Configuration::parse(&bad),
        Err(DescriptorError::EndpointBeforeInterface)
    );

    // bNumInterfaces says two while only interface 0 exists.
    let mut bad = bytes.clone();
    bad[4] = 2;
    assert!(matches!(
        Configuration::parse(&bad),
        Err(DescriptorError::InterfaceCount {
            declared: 2,
            found: 1
        })
    ));
}

#[test]
fn a_device_rejects_a_configuration_count_mismatch() {
    let descriptor = DeviceDescriptor::parse(&device_bytes()).expect("device");
    let configuration = Configuration::parse(&config_bytes()).expect("configuration");
    let device = Device::new(descriptor, vec![configuration.clone()]).expect("device");
    assert_eq!(device.configuration(1), Some(&configuration));

    let mut descriptor = descriptor;
    descriptor.num_configurations = 2;
    assert_eq!(
        Device::new(descriptor, vec![configuration]),
        Err(DescriptorError::ConfigurationCount {
            declared: 2,
            found: 1
        })
    );
}

#[test]
fn duplicate_interface_numbers_are_rejected() {
    let mut bytes = config_bytes();
    // Turn the alternate-setting descriptor into a duplicate 0.0.
    bytes[36 + 3] = 0;
    assert!(matches!(
        Configuration::parse(&bytes),
        Err(DescriptorError::DuplicateInterface {
            number: 0,
            alternate: 0
        })
    ));
}
