// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for USB enumeration.

use alloc::vec::Vec;

use crate::control::{
    ControlError, ControlTransport, DescriptorType, SetupPacket, StandardRequest, request,
};

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

#[derive(Default)]
struct MockTransport {
    device: Vec<u8>,
    config: Vec<u8>,
    stall: Option<u8>,
    log: Vec<SetupPacket>,
}

impl MockTransport {
    fn new() -> Self {
        Self {
            device: device_bytes().to_vec(),
            config: config_bytes(),
            stall: None,
            log: Vec::new(),
        }
    }

    fn with_stall(request: u8) -> Self {
        let mut transport = Self::new();
        transport.stall = Some(request);
        transport
    }
}

impl ControlTransport for MockTransport {
    fn control(&mut self, setup: SetupPacket, data: &mut [u8]) -> Result<usize, UsbError> {
        self.log.push(setup);
        if self.stall == Some(setup.request) {
            return Err(UsbError::Control(ControlError::Stall));
        }
        match setup.request {
            request::GET_DESCRIPTOR => {
                let source = match DescriptorType::from_code((setup.value >> 8) as u8) {
                    DescriptorType::Device => self.device.clone(),
                    DescriptorType::Configuration => self.config.clone(),
                    _ => return Err(UsbError::Control(ControlError::Protocol)),
                };
                let take = source.len().min(data.len());
                data[..take].copy_from_slice(&source[..take]);
                Ok(take)
            }
            request::SET_ADDRESS | request::SET_CONFIGURATION | request::SET_INTERFACE => {
                assert_eq!(setup.length, 0);
                Ok(0)
            }
            _ => Err(UsbError::Control(ControlError::Protocol)),
        }
    }
}

#[test]
fn a_device_enumerates_from_attached_to_configured() {
    let mut transport = MockTransport::new();
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    assert_eq!(enumerator.state(), EnumerationState::Attached);
    assert_eq!(enumerator.address(), None);

    enumerator
        .enumerate(&mut transport, &mut pool)
        .expect("enumerate");
    assert_eq!(enumerator.state(), EnumerationState::Configured);
    assert_eq!(enumerator.address(), Some(1));
    assert_eq!(enumerator.active_configuration(), Some(1));
    assert_eq!(enumerator.configurations().len(), 1);
    assert_eq!(
        enumerator.device().map(|device| device.max_packet_size_0),
        Some(64)
    );
    assert_eq!(
        enumerator
            .configuration()
            .map(|config| config.header.total_length),
        Some(25)
    );
    assert_eq!(
        enumerator
            .interface(0, 0)
            .map(|interface| interface.endpoints.len()),
        Some(1)
    );
    assert!(pool.is_assigned(1));
    assert_eq!(transport.log.len(), 5);
    let requests: Vec<u8> = transport.log.iter().map(|setup| setup.request).collect();
    assert_eq!(
        requests,
        [
            request::GET_DESCRIPTOR,
            request::SET_ADDRESS,
            request::GET_DESCRIPTOR,
            request::GET_DESCRIPTOR,
            request::SET_CONFIGURATION
        ]
    );
    assert_eq!(
        StandardRequest::from_setup(transport.log[1]),
        Some(StandardRequest::SetAddress)
    );
    assert_eq!(transport.log[1].value, 1);
    assert_eq!(transport.log[4].value, 1);

    let device = device(&enumerator).expect("device aggregate");
    assert_eq!(device.configurations.len(), 1);
}

#[test]
fn a_failed_configuration_selection_releases_the_address() {
    let mut transport = MockTransport::with_stall(request::SET_CONFIGURATION);
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("stall");
    assert_eq!(error, UsbError::Control(ControlError::Stall));
    assert_eq!(enumerator.state(), EnumerationState::Addressed);
    assert_eq!(enumerator.address(), None);
    assert!(!pool.is_assigned(1));
    assert_eq!(enumerator.active_configuration(), None);
}

#[test]
fn short_and_malformed_descriptors_are_rejected() {
    let mut transport = MockTransport::new();
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    transport.device.truncate(8);
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("short device");
    assert!(matches!(
        error,
        UsbError::Control(ControlError::ShortTransfer { .. })
    ));
    assert_eq!(enumerator.state(), EnumerationState::Attached);

    let mut transport = MockTransport::new();
    transport.device[0] = 0;
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("bad device");
    assert!(matches!(error, UsbError::Descriptor(_)));
    assert_eq!(enumerator.state(), EnumerationState::Attached);

    let mut transport = MockTransport::new();
    transport.config.truncate(9);
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("short config");
    assert!(matches!(
        error,
        UsbError::Control(ControlError::ShortTransfer { .. })
    ));
    assert_eq!(enumerator.state(), EnumerationState::Addressed);
    assert_eq!(enumerator.address(), None);
}

#[test]
fn a_second_enumerate_is_a_state_error() {
    let mut transport = MockTransport::new();
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    enumerator
        .enumerate(&mut transport, &mut pool)
        .expect("first");
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("second");
    assert!(matches!(error, UsbError::State { .. }));
}

#[test]
fn set_interface_requires_the_configured_state_and_an_existing_alt() {
    let mut transport = MockTransport::new();
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    let error = enumerator
        .set_interface(&mut transport, 0, 0)
        .expect_err("not configured");
    assert!(matches!(error, UsbError::State { .. }));

    enumerator
        .enumerate(&mut transport, &mut pool)
        .expect("enumerate");
    enumerator
        .set_interface(&mut transport, 0, 0)
        .expect("set interface");
    let error = enumerator
        .set_interface(&mut transport, 9, 0)
        .expect_err("missing interface");
    assert!(matches!(error, UsbError::State { .. }));
    assert_eq!(
        transport.log.last().map(|setup| setup.request),
        Some(request::SET_INTERFACE)
    );
}

#[test]
fn release_returns_the_address_and_clears_the_configuration() {
    let mut transport = MockTransport::new();
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    enumerator
        .enumerate(&mut transport, &mut pool)
        .expect("enumerate");
    enumerator.release(&mut pool).expect("release");
    assert_eq!(enumerator.state(), EnumerationState::Described);
    assert_eq!(enumerator.address(), None);
    assert_eq!(enumerator.active_configuration(), None);
    assert!(!pool.is_assigned(1));
    enumerator.release(&mut pool).expect("idempotent release");
}

#[test]
fn a_configuration_with_the_wrong_count_is_rejected() {
    let mut transport = MockTransport::new();
    transport.config[4] = 2;
    let mut pool = AddressPool::new();
    let mut enumerator = Enumerator::new();
    let error = enumerator
        .enumerate(&mut transport, &mut pool)
        .expect_err("interface count");
    assert!(matches!(error, UsbError::Descriptor(_)));
    assert_eq!(enumerator.state(), EnumerationState::Addressed);
    assert!(!pool.is_assigned(1));
}
