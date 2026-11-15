// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the UAC1 descriptor set.

use super::*;
use crate::test_config::{endpoint, sample};
use crate::usb::{Direction, EndpointAddress};

fn encode(config: &Uac1Config) -> Vec<u8> {
    let mut bytes = [0u8; 256];
    let length = config.encode(&mut bytes).expect("encode");
    bytes[..length].to_vec()
}

fn find(bytes: &[u8], subtype: u8) -> Option<usize> {
    let mut index = 18usize;
    while index + 2 < bytes.len() {
        let length = usize::from(bytes[index]);
        if length < 2 {
            return None;
        }
        if bytes[index + 1] == CS_INTERFACE && bytes[index + 2] == subtype {
            return Some(index);
        }
        index = index.checked_add(length)?;
    }
    None
}

fn find_last(bytes: &[u8], subtype: u8) -> Option<usize> {
    let mut found = None;
    let mut index = 18usize;
    while index + 2 < bytes.len() {
        let length = usize::from(bytes[index]);
        if length < 2 {
            return None;
        }
        if bytes[index + 1] == CS_INTERFACE && bytes[index + 2] == subtype {
            found = Some(index);
        }
        index = index.checked_add(length)?;
    }
    found
}

#[test]
fn a_complete_descriptor_set_round_trips() {
    let config = sample();
    config.validate().expect("valid");
    let bytes = encode(&config);
    assert_eq!(bytes.len(), 119);
    assert_eq!(
        u16::from_le_bytes([bytes[20], bytes[21]]),
        101,
        "wTotalLength includes the configuration header"
    );
    let parsed = Uac1Config::parse(&bytes).expect("parse");
    assert_eq!(parsed, config);
}

#[test]
fn the_entity_graph_is_validated() {
    let mut config = sample();
    config.feature.source_id = 9;
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadEntity("terminal/unit chain"))
    );
    assert_eq!(
        config.encode(&mut [0u8; 256]),
        Err(Uac1Error::BadEntity("terminal/unit chain"))
    );

    let mut config = sample();
    config.output.terminal_type = 0x0101;
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadValue("output terminal type"))
    );

    let mut config = sample();
    config.streaming.general.terminal_link = 7;
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadEntity("AS terminal link"))
    );

    let mut config = sample();
    config.header.interface_numbers = [3, 0, 0, 0];
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadEntity("streaming interface number"))
    );
}

#[test]
fn the_format_and_endpoints_are_validated() {
    let mut config = sample();
    config.streaming.format.channels = 1;
    assert_eq!(config.validate(), Err(Uac1Error::BadValue("channel count")));

    let mut config = sample();
    config.streaming.format.count = 0;
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadValue("sample-rate count"))
    );

    let mut config = sample();
    config.streaming.data.attributes = 0x02;
    assert_eq!(config.validate(), Err(Uac1Error::BadValue("data endpoint")));

    let mut config = sample();
    config.streaming.data.address = EndpointAddress::new(1, Direction::Out).expect("address");
    assert_eq!(config.validate(), Err(Uac1Error::BadValue("data endpoint")));

    let mut config = sample();
    config.streaming.feedback = Some(endpoint(2, Direction::In, 0x12, 4));
    assert_eq!(
        config.validate(),
        Err(Uac1Error::BadValue("feedback endpoint"))
    );
}

#[test]
fn a_truncated_set_is_rejected() {
    let bytes = encode(&sample());
    assert!(matches!(
        Uac1Config::parse(&bytes[..20]),
        Err(Uac1Error::Descriptor(_)) | Err(Uac1Error::Truncated(_))
    ));

    let mut broken = encode(&sample());
    broken[1] = crate::usb::TYPE_CONFIG;
    assert!(matches!(
        Uac1Config::parse(&broken),
        Err(Uac1Error::Descriptor(_))
    ));

    let mut broken = encode(&sample());
    broken[20] = 0xFF;
    broken[21] = 0x00;
    assert_eq!(
        Uac1Config::parse(&broken),
        Err(Uac1Error::Truncated("configuration"))
    );
}

#[test]
fn malformed_class_descriptors_are_rejected() {
    let mut broken = encode(&sample());
    let input = find(&broken, INPUT_TERMINAL).expect("input");
    broken[input + 4] = 0x02;
    broken[input + 5] = 0x02;
    assert_eq!(
        Uac1Config::parse(&broken),
        Err(Uac1Error::BadValue("input terminal type"))
    );

    let mut broken = encode(&sample());
    let format = find_last(&broken, FORMAT_TYPE_I).expect("format");
    broken[format + 8] = 0;
    broken[format + 9] = 0;
    broken[format + 10] = 0;
    assert_eq!(
        Uac1Config::parse(&broken),
        Err(Uac1Error::BadValue("zero sample rate"))
    );

    let mut broken = encode(&sample());
    let feature = find(&broken, FEATURE_UNIT).expect("feature");
    broken[feature + 4] = 7;
    assert_eq!(
        Uac1Config::parse(&broken),
        Err(Uac1Error::BadEntity("terminal/unit chain"))
    );
}

#[test]
fn the_feature_unit_reports_its_controls() {
    let feature = sample().feature;
    assert!(feature.supports_mute());
    assert!(feature.supports_volume());
    let mut quiet = feature;
    quiet.controls = [0; MAX_CONTROLS];
    assert!(!quiet.supports_mute());
    assert!(!quiet.supports_volume());
}

#[test]
fn a_small_output_buffer_is_rejected() {
    let mut small = [0u8; 32];
    assert_eq!(sample().encode(&mut small), Err(Uac1Error::Overflow));
}
