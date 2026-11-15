// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared test-only descriptor fixture.
//!
//! A synthetic UAC1 playback device: stereo S16, 44.1/48 kHz, one asynchronous IN data endpoint
//! and a feedback endpoint. It is our own construction, not a donor blob.

use crate::uac1::{
    AUDIO_CLASS, AcHeader, AsInterface, FeatureUnit, FormatTypeI, InputTerminal, OutputTerminal,
    SUBCLASS_AUDIOCONTROL, SUBCLASS_AUDIOSTREAMING, StreamingInterface, TERMINAL_SPEAKER,
    TERMINAL_USB_STREAMING, Uac1Config,
};
use crate::usb::{
    ConfigDescriptor, DeviceDescriptor, Direction, EndpointAddress, EndpointDescriptor,
    InterfaceDescriptor,
};

pub(crate) fn endpoint(
    address: u8,
    direction: Direction,
    attributes: u8,
    packet: u16,
) -> EndpointDescriptor {
    EndpointDescriptor {
        address: EndpointAddress::new(address, direction).expect("endpoint address"),
        attributes,
        max_packet: packet,
        interval: 1,
    }
}

pub(crate) fn sample() -> Uac1Config {
    Uac1Config {
        device: DeviceDescriptor {
            usb_version: 0x0200,
            class: 0,
            subclass: 0,
            protocol: 0,
            max_packet0: 64,
            vendor: 0x1234,
            product: 0x5678,
            device_version: 0x0100,
            configurations: 1,
        },
        config: ConfigDescriptor {
            total_length: 101,
            interfaces: 2,
            value: 1,
            attributes: 0x80,
            max_power: 250,
        },
        control: InterfaceDescriptor {
            number: 0,
            alternate: 0,
            endpoints: 0,
            class: AUDIO_CLASS,
            subclass: SUBCLASS_AUDIOCONTROL,
            protocol: 0,
        },
        header: AcHeader {
            adc_version: 0x0100,
            total_length: 39,
            interface_numbers: [1, 0, 0, 0],
            count: 1,
        },
        input: InputTerminal {
            id: 1,
            terminal_type: TERMINAL_USB_STREAMING,
            channels: 2,
            channel_config: 0x0003,
        },
        feature: FeatureUnit {
            id: 2,
            source_id: 1,
            controls: [0x03, 0x03, 0x03, 0, 0, 0, 0, 0, 0, 0, 0, 0],
            count: 3,
        },
        output: OutputTerminal {
            id: 3,
            terminal_type: TERMINAL_SPEAKER,
            source_id: 2,
        },
        streaming: StreamingInterface {
            interface: InterfaceDescriptor {
                number: 1,
                alternate: 1,
                endpoints: 2,
                class: AUDIO_CLASS,
                subclass: SUBCLASS_AUDIOSTREAMING,
                protocol: 0,
            },
            general: AsInterface {
                terminal_link: 1,
                delay: 1,
                format_tag: 1,
            },
            format: FormatTypeI {
                channels: 2,
                subframe_bytes: 2,
                bit_resolution: 16,
                rates: [44_100, 48_000, 0, 0, 0, 0, 0, 0],
                count: 2,
            },
            data: endpoint(1, Direction::In, 0x05, 0x00C0),
            feedback: Some(endpoint(2, Direction::In, 0x11, 4)),
        },
    }
}
