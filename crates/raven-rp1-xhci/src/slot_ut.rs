// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the slot and endpoint model.

use super::*;

#[test]
fn a_slot_moves_through_the_command_states() {
    let mut table = SlotTable::new(32).expect("table");
    assert_eq!(table.state(1), None);
    let slot = table.enable().expect("slot");
    assert_eq!(slot, 1);
    assert_eq!(table.state(slot), Some(SlotState::Enabled));
    assert_eq!(table.endpoint(slot, 1), Some(EndpointState::Disabled));

    assert_eq!(table.address(slot), Ok(()));
    assert_eq!(table.state(slot), Some(SlotState::Addressed));
    assert_eq!(table.endpoint(slot, 1), Some(EndpointState::Running));

    assert_eq!(table.configure(slot), Ok(()));
    assert_eq!(table.state(slot), Some(SlotState::Configured));
}

#[test]
fn command_order_is_enforced() {
    let mut table = SlotTable::new(4).expect("table");
    assert_eq!(table.address(1), Err(SlotTableError::UnknownSlot(1)));
    assert_eq!(table.configure(1), Err(SlotTableError::UnknownSlot(1)));
    assert_eq!(table.disable(1), Err(SlotTableError::UnknownSlot(1)));

    let slot = table.enable().expect("slot");
    assert_eq!(
        table.configure(slot),
        Err(SlotTableError::BadState {
            state: SlotState::Enabled,
            expected: SlotState::Addressed
        })
    );
    assert_eq!(
        table.address(slot + 1),
        Err(SlotTableError::UnknownSlot(slot + 1))
    );
}

#[test]
fn slots_are_allocated_and_released() {
    let mut table = SlotTable::new(2).expect("table");
    assert_eq!(table.enable(), Ok(1));
    assert_eq!(table.enable(), Ok(2));
    assert_eq!(table.enable(), Err(SlotTableError::Full));
    assert_eq!(table.disable(1), Ok(()));
    assert_eq!(table.state(1), None);
    assert_eq!(table.enable(), Ok(1));
}

#[test]
fn endpoints_run_halt_stop_and_reset() {
    let mut table = SlotTable::new(4).expect("table");
    let slot = table.enable().expect("slot");
    assert!(table.address(slot).is_ok());
    assert!(table.configure(slot).is_ok());

    assert_eq!(
        table.endpoint_enable(slot, 2),
        Ok(()),
        "Configure Endpoint enables a disabled endpoint"
    );
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Running));
    assert_eq!(table.endpoint_halt(slot, 2), Ok(()));
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Halted));
    assert!(table.endpoint_reset(slot, 2).is_ok());
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Running));
    assert_eq!(table.endpoint_stop(slot, 2), Ok(()));
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Stopped));
    assert_eq!(table.endpoint_start(slot, 2), Ok(()));
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Running));
    assert_eq!(table.endpoint_error(slot, 2), Ok(()));
    assert_eq!(table.endpoint(slot, 2), Some(EndpointState::Error));
}

#[test]
fn invalid_endpoint_transitions_are_rejected() {
    let mut table = SlotTable::new(4).expect("table");
    let slot = table.enable().expect("slot");
    assert!(table.address(slot).is_ok());

    assert_eq!(
        table.endpoint_halt(slot, 2),
        Err(SlotTableError::BadEndpointState {
            state: EndpointState::Disabled,
            transition: "halt"
        })
    );
    assert_eq!(
        table.endpoint_start(slot, 2),
        Err(SlotTableError::BadEndpointState {
            state: EndpointState::Disabled,
            transition: "start"
        })
    );
    assert_eq!(
        table.endpoint_halt(slot, 0),
        Err(SlotTableError::BadEndpoint(0))
    );
    assert_eq!(
        table.endpoint_halt(slot, 32),
        Err(SlotTableError::BadEndpoint(32))
    );

    let enabled = table.enable().expect("second slot");
    assert_eq!(
        table.endpoint_enable(enabled, 2),
        Err(SlotTableError::BadState {
            state: SlotState::Enabled,
            expected: SlotState::Addressed
        })
    );
}

#[test]
fn the_table_and_endpoint_ranges_are_bounded() {
    assert!(SlotTable::new(0).is_err());
    assert!(SlotTable::new(MAX_SLOTS as u8 + 1).is_err());
    let table = SlotTable::new(MAX_SLOTS as u8).expect("table");
    assert_eq!(table.max_slots(), MAX_SLOTS as u8);
    assert_eq!(table.slot(0), None);
    assert_eq!(table.slot(MAX_SLOTS as u8), None);
}
