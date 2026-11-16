// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the feature-unit control requests.

use super::*;

fn unit() -> FeatureUnitState {
    FeatureUnitState::new(
        2,
        0,
        VolumeLimits {
            min: -4096,
            max: 0,
            res: 32,
        },
    )
}

fn setup(request: u8, entity: u8, selector: u8, length: u16) -> SetupPacket {
    let request_type = if request & 0x80 != 0 { 0xA1 } else { 0x21 };
    SetupPacket {
        request_type,
        request,
        value: (u16::from(entity) << 8) | u16::from(selector),
        index: 0,
        length,
    }
}

#[test]
fn mute_reads_and_writes() {
    let mut unit = unit();
    assert_eq!(
        unit.handle(setup(GET_CUR, 2, FEATURE_MUTE, 1), &[]),
        Response::Data {
            bytes: [0, 0],
            len: 1
        }
    );
    assert_eq!(
        unit.handle(setup(SET_CUR, 2, FEATURE_MUTE, 1), &[1]),
        Response::Ack
    );
    assert!(unit.mute);
    assert_eq!(
        unit.handle(setup(GET_CUR, 2, FEATURE_MUTE, 1), &[]),
        Response::Data {
            bytes: [1, 0],
            len: 1
        }
    );
}

#[test]
fn a_mute_value_above_one_is_stalled() {
    let mut unit = unit();
    assert_eq!(
        unit.handle(setup(SET_CUR, 2, FEATURE_MUTE, 1), &[2]),
        Response::Stalled
    );
    assert_eq!(
        unit.handle(setup(SET_CUR, 2, FEATURE_MUTE, 1), &[]),
        Response::Stalled
    );
}

#[test]
fn volume_reads_and_writes_in_256th_db() {
    let mut unit = unit();
    assert_eq!(
        unit.handle(
            setup(SET_CUR, 2, FEATURE_VOLUME, 2),
            &(-2048i16).to_le_bytes()
        ),
        Response::Ack
    );
    assert_eq!(unit.volume, -2048);
    assert_eq!(
        unit.handle(setup(GET_CUR, 2, FEATURE_VOLUME, 2), &[]),
        Response::Data {
            bytes: (-2048i16).to_le_bytes(),
            len: 2
        }
    );
    assert_eq!(
        unit.handle(setup(GET_MIN, 2, FEATURE_VOLUME, 2), &[]),
        Response::Data {
            bytes: (-4096i16).to_le_bytes(),
            len: 2
        }
    );
    assert_eq!(
        unit.handle(setup(GET_MAX, 2, FEATURE_VOLUME, 2), &[]),
        Response::Data {
            bytes: 0i16.to_le_bytes(),
            len: 2
        }
    );
    assert_eq!(
        unit.handle(setup(GET_RES, 2, FEATURE_VOLUME, 2), &[]),
        Response::Data {
            bytes: 32i16.to_le_bytes(),
            len: 2
        }
    );
}

#[test]
fn a_volume_outside_the_range_is_stalled() {
    let mut unit = unit();
    assert_eq!(
        unit.handle(setup(SET_CUR, 2, FEATURE_VOLUME, 2), &100i16.to_le_bytes()),
        Response::Stalled
    );
    assert_eq!(
        unit.handle(
            setup(SET_CUR, 2, FEATURE_VOLUME, 2),
            &(-4097i16).to_le_bytes()
        ),
        Response::Stalled
    );
    assert_eq!(unit.volume, 0);
}

#[test]
fn the_request_type_must_be_class_interface() {
    let mut unit = unit();
    let mut standard = setup(GET_CUR, 2, FEATURE_MUTE, 1);
    standard.request_type = 0x80;
    assert_eq!(unit.handle(standard, &[]), Response::Unsupported);

    let mut device = setup(GET_CUR, 2, FEATURE_MUTE, 1);
    device.request_type = 0xA0;
    assert_eq!(unit.handle(device, &[]), Response::Unsupported);

    let mut other_interface = setup(GET_CUR, 2, FEATURE_MUTE, 1);
    other_interface.index = 1;
    assert_eq!(unit.handle(other_interface, &[]), Response::Unsupported);

    let wrong_entity = setup(GET_CUR, 3, FEATURE_MUTE, 1);
    assert_eq!(unit.handle(wrong_entity, &[]), Response::Unsupported);

    let unknown_selector = setup(GET_CUR, 2, 0x05, 1);
    assert_eq!(unit.handle(unknown_selector, &[]), Response::Unsupported);
}

#[test]
fn the_direction_must_match_the_request() {
    let mut unit = unit();
    let mut read = setup(GET_CUR, 2, FEATURE_MUTE, 1);
    read.request_type = 0x21;
    assert_eq!(unit.handle(read, &[]), Response::Unsupported);

    let mut write = setup(SET_CUR, 2, FEATURE_MUTE, 1);
    write.request_type = 0xA1;
    assert_eq!(unit.handle(write, &[1]), Response::Unsupported);
}

#[test]
fn range_fields_are_read_only() {
    let mut unit = unit();
    assert_eq!(
        unit.handle(setup(SET_MIN, 2, FEATURE_VOLUME, 2), &[0, 0]),
        Response::Unsupported
    );
    assert_eq!(
        unit.handle(setup(SET_MAX, 2, FEATURE_VOLUME, 2), &[0, 0]),
        Response::Unsupported
    );
    assert_eq!(
        unit.handle(setup(SET_RES, 2, FEATURE_VOLUME, 2), &[0, 0]),
        Response::Unsupported
    );
}

#[test]
fn the_volume_limits_are_validated_and_clamped() {
    assert!(
        VolumeLimits {
            min: -10,
            max: 10,
            res: 1
        }
        .valid()
    );
    assert!(
        !VolumeLimits {
            min: 10,
            max: -10,
            res: 1
        }
        .valid()
    );
    let limits = VolumeLimits {
        min: -10,
        max: 10,
        res: 1,
    };
    assert_eq!(limits.clamp(-20), -10);
    assert_eq!(limits.clamp(0), 0);
    assert_eq!(limits.clamp(20), 10);
}
