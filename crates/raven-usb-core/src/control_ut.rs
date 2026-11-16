// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for setup packets, standard requests and the control seam.

use super::*;

fn setup() -> SetupPacket {
    SetupPacket {
        request_type: 0x80,
        request: request::GET_DESCRIPTOR,
        value: 0x0100,
        index: 0,
        length: 18,
    }
}

#[test]
fn the_setup_packet_round_trips() {
    let packet = setup();
    let bytes = packet.to_bytes();
    assert_eq!(bytes, [0x80, 6, 0, 1, 0, 0, 18, 0]);
    assert_eq!(SetupPacket::parse(&bytes), Ok(packet));
    assert_eq!(packet.direction(), Direction::In);
    assert_eq!(packet.kind(), RequestKind::Standard);
    assert_eq!(packet.recipient(), Recipient::Device);
    assert!(packet.is_standard());
}

#[test]
fn request_type_class_vendor_recipient_and_reserved_bits() {
    let packet = SetupPacket::parse(&[0xa1, 1, 0, 0, 0, 0, 1, 0]).expect("class interface");
    assert_eq!(packet.kind(), RequestKind::Class);
    assert_eq!(packet.recipient(), Recipient::Interface);
    assert_eq!(packet.direction(), Direction::In);

    let packet = SetupPacket::parse(&[0x42, 1, 0, 0, 0, 0, 1, 0]).expect("vendor endpoint");
    assert_eq!(packet.kind(), RequestKind::Vendor);
    assert_eq!(packet.recipient(), Recipient::Endpoint);

    assert_eq!(
        SetupPacket::parse(&[0x60, 1, 0, 0, 0, 0, 0, 0]),
        Err(ControlError::BadRequestType(0x60))
    );
    assert_eq!(
        SetupPacket::parse(&[0x1f, 1, 0, 0, 0, 0, 0, 0]),
        Err(ControlError::BadRequestType(0x1f))
    );
    assert_eq!(
        SetupPacket::parse(&[0; 7]),
        Err(ControlError::ShortSetup(7))
    );
}

#[test]
fn standard_requests_decode() {
    assert_eq!(
        StandardRequest::from_setup(setup()),
        Some(StandardRequest::GetDescriptor)
    );
    let set_address = set_address(7).expect("address");
    assert_eq!(
        StandardRequest::from_setup(set_address),
        Some(StandardRequest::SetAddress)
    );
    assert_eq!(
        StandardRequest::from_setup(SetupPacket {
            request_type: 0x40,
            request: request::GET_DESCRIPTOR,
            value: 0,
            index: 0,
            length: 0,
        }),
        None
    );
    assert_eq!(StandardRequest::from_code(0xff), None);
}

#[test]
fn descriptor_types_map_to_the_get_descriptor_value() {
    assert_eq!(DescriptorType::from_code(1), DescriptorType::Device);
    assert_eq!(DescriptorType::from_code(2), DescriptorType::Configuration);
    assert_eq!(DescriptorType::from_code(3), DescriptorType::String);
    assert_eq!(DescriptorType::from_code(4), DescriptorType::Interface);
    assert_eq!(DescriptorType::from_code(5), DescriptorType::Endpoint);
    assert_eq!(DescriptorType::from_code(0x30), DescriptorType::Other(0x30));
    assert_eq!(DescriptorType::String.code(), 3);

    let packet = get_descriptor(DescriptorType::Device, 0, 0, 18);
    assert_eq!(packet.value, 0x0100);
    assert_eq!(packet.length, 18);
    let packet = get_descriptor(DescriptorType::Configuration, 0, 0, 9);
    assert_eq!(packet.value, 0x0200);
}

#[test]
fn standard_request_constructors_are_well_formed() {
    let address = set_address(9).expect("address");
    assert_eq!(address.value, 9);
    assert_eq!(address.request_type, 0x00);
    assert!(set_address(0).is_none());
    assert!(set_address(128).is_none());

    let configuration = set_configuration(1).expect("configuration");
    assert_eq!(configuration.value, 1);
    assert_eq!(configuration.request_type, 0x00);
    assert!(set_configuration(0).is_none());

    let configuration = get_configuration();
    assert_eq!(configuration.request_type, 0x80);
    assert_eq!(configuration.length, 1);

    let status = get_status(Recipient::Endpoint, 0x81);
    assert_eq!(status.request_type, 0x82);
    assert_eq!(status.index, 0x81);
    assert_eq!(status.length, 2);

    let feature = set_feature(0, Recipient::Endpoint, 0x81);
    assert_eq!(feature.request_type, 0x02);
    assert_eq!(feature.request, request::SET_FEATURE);
    let feature = clear_feature(0, Recipient::Device, 0);
    assert_eq!(feature.request, request::CLEAR_FEATURE);

    let packet = set_interface(2, 1);
    assert_eq!(packet.value, 1);
    assert_eq!(packet.index, 2);
    assert_eq!(packet.request_type, 0x01);
    let packet = get_interface(2);
    assert_eq!(packet.request_type, 0x81);
    assert_eq!(packet.length, 1);
}

struct EmptyTransport;

impl ControlTransport for EmptyTransport {
    fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
        assert_eq!(setup.length, 0);
        assert!(data.is_empty());
        Ok(0)
    }
}

#[test]
fn no_data_control_checks_the_length() {
    let mut transport = EmptyTransport;
    assert_eq!(
        transport.control_no_data(set_address(1).expect("address")),
        Ok(())
    );
    assert_eq!(
        transport.control_no_data(setup()),
        Err(UsbError::Control(ControlError::LengthMismatch {
            expected: 18,
            got: 0
        }))
    );
}

#[test]
fn control_errors_display() {
    assert_eq!(
        ControlError::ShortTransfer {
            expected: 18,
            got: 8
        }
        .to_string(),
        "control returned 8 of 18 bytes"
    );
    assert_eq!(ControlError::Stall.to_string(), "control endpoint stalled");
}
