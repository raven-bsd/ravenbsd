// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! End-to-end assertions over the RP1 xHCI fixture and the UAC1 model.

use super::*;
use raven_fdt_lite::Fdt;
use raven_rp1_xhci::UsbControllers;

use crate::test_config::sample;

const XHCI: &[u8] = include_bytes!("../../../tests/fixtures/pi5/pi5-rp1-xhci.dtb");

#[test]
fn the_pi5_xhci_controllers_are_the_transport() {
    let fdt = Fdt::parse(XHCI).expect("fixture FDT");
    let controllers = UsbControllers::from_fdt(&fdt).expect("USB nodes");
    assert_eq!(controllers.count(), 2);
    assert_eq!(controllers.enabled_count(), 2);
    let first = controllers.get(0).expect("controller 0");
    assert_eq!(first.window.base(), 0x00c0_4020_0000);
    assert_eq!(first.window.size(), 0x10_0000);

    let mut host = AudioHost::new(sample(), 8, 8).expect("host");
    host.open(1, 48_000).expect("open");
    assert_eq!(host.dci(), 3);
    assert_eq!(host.queue(0x1000), Ok(0));
    host.device_complete(1).expect("event");
    let completion = host.complete().expect("completion");
    assert_eq!(completion.trb.endpoint(), 3);
    assert_eq!(completion.event.expect("event").completion_code(), 1);
}

#[test]
fn the_descriptor_set_round_trips_into_a_host() {
    let config = sample();
    let mut bytes = [0u8; 256];
    let length = config.encode(&mut bytes).expect("encode");
    let parsed = Uac1Config::parse(&bytes[..length]).expect("parse");
    assert_eq!(parsed, config);

    let mut host = AudioHost::new(parsed, 8, 8).expect("host");
    host.open(parsed.streaming.interface.alternate, 48_000)
        .expect("open");
    assert_eq!(host.format().map(|format| format.rate_hz), Some(48_000));
    assert_eq!(host.feedback(), Some(0x0006_0000));
}

#[test]
fn the_uac1_configuration_exposes_the_playback_graph() {
    let config = sample();
    assert_eq!(config.input.terminal_type, uac1::TERMINAL_USB_STREAMING);
    assert_eq!(config.output.terminal_type, uac1::TERMINAL_SPEAKER);
    assert!(config.feature.supports_mute());
    assert!(config.feature.supports_volume());
    assert!(config.streaming.format.supports_rate(44_100));
    assert!(config.streaming.format.supports_rate(48_000));
    assert!(!config.streaming.format.supports_rate(96_000));
}
