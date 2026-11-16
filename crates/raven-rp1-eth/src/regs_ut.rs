// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the register map.

use super::*;

#[test]
fn an_empty_map_has_no_offsets() {
    let map = RegisterMap::new();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
    assert_eq!(map.offset(Register::Control), None);
}

#[test]
fn a_set_offset_is_returned_by_name() {
    let mut map = RegisterMap::new();
    assert_eq!(map.set(Register::Control, 0x00), Ok(()));
    assert_eq!(map.set(Register::MdioData, 0x20), Ok(()));
    assert_eq!(map.offset(Register::Control), Some(0x00));
    assert_eq!(map.offset(Register::MdioData), Some(0x20));
    assert_eq!(map.offset(Register::Status), None);
    assert_eq!(map.len(), 2);
}

#[test]
fn an_offset_can_be_replaced_and_cleared() {
    let mut map = RegisterMap::new();
    assert_eq!(map.set(Register::Status, 0x04), Ok(()));
    assert_eq!(map.set(Register::Status, 0x08), Ok(()));
    assert_eq!(map.offset(Register::Status), Some(0x08));
    map.clear(Register::Status);
    assert_eq!(map.offset(Register::Status), None);
    assert!(map.is_empty());
}

#[test]
fn malformed_offsets_are_rejected() {
    let mut map = RegisterMap::new();
    assert_eq!(
        map.set(Register::Control, 0x02),
        Err(RegisterError::Misaligned(0x02))
    );
    assert_eq!(
        map.set(Register::Config, WINDOW_BYTES - 2),
        Err(RegisterError::Misaligned(WINDOW_BYTES - 2))
    );
    assert_eq!(
        map.set(Register::Config, WINDOW_BYTES),
        Err(RegisterError::OutOfWindow(WINDOW_BYTES))
    );
    assert!(map.is_empty());
}

#[test]
fn every_register_has_a_distinct_slot() {
    let mut map = RegisterMap::new();
    for (index, register) in Register::ALL.iter().enumerate() {
        let offset = u16::try_from(index).expect("index") * 4;
        assert_eq!(map.set(*register, offset), Ok(()), "{register:?}");
    }
    assert_eq!(map.len(), REGISTER_COUNT);
    for (index, register) in Register::ALL.iter().enumerate() {
        assert_eq!(
            map.offset(*register),
            Some(u16::try_from(index).expect("index") * 4)
        );
    }
}
