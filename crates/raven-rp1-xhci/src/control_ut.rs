// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI control-transfer TRB sequence.

use super::*;
use crate::ring::TransferRing;

fn setup() -> [u8; 8] {
    // bmRequestType 0x21, bRequest 0x01, wValue 0x0201, wIndex 0, wLength 1.
    [0x21, 0x01, 0x01, 0x02, 0x00, 0x00, 0x01, 0x00]
}

#[test]
fn a_host_to_device_transfer_has_three_trbs() {
    let transfer =
        ControlTransfer::new(setup(), 0x2000, 1, ControlDirection::Out).expect("transfer");
    let (trbs, count) = transfer.trbs();
    assert_eq!(count, 3);
    assert_eq!(transfer.trb_count(), 3);

    let setup_trb = trbs[0];
    assert_eq!(setup_trb.kind(), TrbType::SetupStage);
    assert_eq!(setup_trb.parameter, 0x0001_0000_0201_0121);
    assert_eq!(setup_trb.status, 8);
    assert_eq!((setup_trb.control >> TRB_TRT_SHIFT) & TRB_TRT_MASK, 3);
    assert_ne!(setup_trb.control & TRB_IDT, 0);
    assert!(!setup_trb.cycle());

    let data_trb = trbs[1];
    assert_eq!(data_trb.kind(), TrbType::DataStage);
    assert_eq!(data_trb.parameter, 0x2000);
    assert_eq!(data_trb.status, 1);
    assert_eq!(data_trb.control & TRB_DIR_IN, 0, "OUT data stage");

    let status_trb = trbs[2];
    assert_eq!(status_trb.kind(), TrbType::StatusStage);
    assert_ne!(status_trb.control & TRB_DIR_IN, 0, "status stage is IN");
    assert_ne!(status_trb.control & TRB_IOC, 0);
}

#[test]
fn a_device_to_host_transfer_flips_the_data_and_status_directions() {
    let transfer =
        ControlTransfer::new(setup(), 0x3000, 1, ControlDirection::In).expect("transfer");
    let (trbs, count) = transfer.trbs();
    assert_eq!(count, 3);
    assert_eq!((trbs[0].control >> TRB_TRT_SHIFT) & TRB_TRT_MASK, 2);
    assert_ne!(trbs[1].control & TRB_DIR_IN, 0, "IN data stage");
    assert_eq!(trbs[2].control & TRB_DIR_IN, 0, "status stage is OUT");
    assert_ne!(trbs[2].control & TRB_IOC, 0);
}

#[test]
fn a_no_data_transfer_has_two_trbs() {
    let mut setup = setup();
    setup[6] = 0;
    setup[7] = 0;
    let transfer = ControlTransfer::new(setup, 0, 0, ControlDirection::NoData).expect("transfer");
    let (trbs, count) = transfer.trbs();
    assert_eq!(count, 2);
    assert_eq!(trbs[0].kind(), TrbType::SetupStage);
    assert_eq!((trbs[0].control >> TRB_TRT_SHIFT) & TRB_TRT_MASK, 0);
    assert_eq!(trbs[1].kind(), TrbType::StatusStage);
    assert_ne!(trbs[1].control & TRB_DIR_IN, 0);
}

#[test]
fn inconsistent_lengths_are_rejected() {
    assert!(ControlTransfer::new(setup(), 0, 0, ControlDirection::Out).is_none());
    assert!(ControlTransfer::new(setup(), 0, 4, ControlDirection::NoData).is_none());
    assert!(ControlTransfer::new(setup(), 0, 4, ControlDirection::In).is_some());
}

#[test]
fn the_sequence_pushes_onto_a_transfer_ring() {
    let mut ring = TransferRing::new(8).expect("ring");
    assert_eq!(ring.capacity(), 8);
    assert_eq!(ring.usable(), 7);
    assert_eq!(ring.free_slots(), 7);
    let transfer =
        ControlTransfer::new(setup(), 0x2000, 1, ControlDirection::Out).expect("transfer");
    let (trbs, count) = transfer.trbs();
    for (offset, trb) in trbs.iter().take(usize::from(count)).enumerate() {
        assert_eq!(ring.push(*trb), Ok(offset as u16));
    }
    assert_eq!(ring.pending(), 3);
    assert_eq!(ring.free_slots(), 4);
    assert!(
        ring.slot(2)
            .is_some_and(|trb| trb.kind() == TrbType::StatusStage)
    );
    assert_eq!(ring.complete(), Some(0));
    assert_eq!(ring.complete(), Some(1));
    assert_eq!(ring.complete(), Some(2));
    assert!(!ring.is_full());
}
