// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the xHCI transfer-ring seam.

use raven_rp1_xhci::{ControlDirection, Trb, TrbType};

use super::*;

fn setup() -> [u8; 8] {
    // GET_DESCRIPTOR device, 18 bytes.
    [0x80, 6, 0, 1, 0, 0, 18, 0]
}

#[test]
fn a_control_transfer_becomes_a_chained_trb_sequence() {
    let mut seam = XhciSeam::new(8, 8, 3, 1).expect("seam");
    let transfer =
        ControlTransfer::new(setup(), 0x2000, 18, ControlDirection::In).expect("transfer");
    assert_eq!(seam.queue(&transfer), Ok(3));
    assert_eq!(seam.pending(), 3);
    assert_eq!(seam.endpoint(), 1);
    assert_eq!(seam.slot(), 3);

    let setup_trb = seam.ring().slot(0).expect("setup");
    assert_eq!(setup_trb.kind(), TrbType::SetupStage);
    assert!(setup_trb.chain());
    assert_eq!(setup_trb.endpoint(), 1);
    assert_eq!(setup_trb.slot(), 3);

    let data_trb = seam.ring().slot(1).expect("data");
    assert_eq!(data_trb.kind(), TrbType::DataStage);
    assert!(data_trb.chain());
    assert_eq!(data_trb.endpoint(), 1);

    let status_trb = seam.ring().slot(2).expect("status");
    assert_eq!(status_trb.kind(), TrbType::StatusStage);
    assert!(!status_trb.chain());
    assert_eq!(status_trb.endpoint(), 1);
    assert_eq!(seam.complete_transfer(), Some(0));
}

#[test]
fn a_no_data_transfer_has_two_trbs() {
    let mut seam = XhciSeam::new(8, 8, 1, 1).expect("seam");
    let mut setup = setup();
    setup[6] = 0;
    setup[7] = 0;
    let transfer = ControlTransfer::new(setup, 0, 0, ControlDirection::NoData).expect("transfer");
    assert_eq!(seam.queue(&transfer), Ok(2));
    assert_eq!(seam.pending(), 2);
    assert!(seam.ring().slot(0).expect("setup").chain());
    assert!(!seam.ring().slot(1).expect("status").chain());
}

#[test]
fn events_are_decoded_into_typed_completions() {
    let mut seam = XhciSeam::new(8, 8, 2, 1).expect("seam");
    let transfer =
        ControlTransfer::new(setup(), 0x2000, 18, ControlDirection::In).expect("transfer");
    seam.queue(&transfer).expect("queue");

    let event = Trb::transfer_event(2, 1, COMPLETION_SUCCESS, 0x3000).expect("event");
    seam.push_event(event).expect("push");
    let completion = seam.poll().expect("completion");
    let XhciEvent::Transfer(completion) = completion else {
        panic!("expected a transfer completion");
    };
    assert_eq!(completion.slot, 2);
    assert_eq!(completion.endpoint, 1);
    assert!(completion.success());
    assert!(!completion.stalled());
    assert_eq!(completion.trb_pointer, 0x3000);
    assert_eq!(seam.poll(), None);

    let event = Trb::command_completion(2, COMPLETION_SUCCESS, 0x4000).expect("command");
    seam.push_event(event).expect("push");
    let event = seam.poll().expect("command event");
    let XhciEvent::Command(command) = event else {
        panic!("expected a command completion");
    };
    assert_eq!(command.slot, 2);
    assert_eq!(command.command_pointer, 0x4000);

    seam.push_event(Trb::port_status_change(4)).expect("port");
    assert_eq!(seam.poll(), Some(XhciEvent::Port(4)));

    seam.push_event(Trb::link(0, true)).expect("link");
    assert!(matches!(seam.poll(), Some(XhciEvent::Other(TrbType::Link))));
}

#[test]
fn a_stall_completion_is_reported() {
    let mut seam = XhciSeam::new(8, 8, 1, 1).expect("seam");
    seam.push_event(Trb::transfer_event(1, 1, COMPLETION_STALL, 0).expect("event"))
        .expect("push");
    let Some(XhciEvent::Transfer(completion)) = seam.poll() else {
        panic!("expected a transfer");
    };
    assert!(completion.stalled());
    assert!(!completion.success());
}

#[test]
fn invalid_ring_and_slot_configurations_are_rejected() {
    assert!(matches!(
        XhciSeam::new(1, 8, 1, 1),
        Err(UsbError::Ring(raven_rp1_xhci::RingError::TooSmall(1)))
    ));
    assert!(matches!(
        XhciSeam::new(8, 8, 0, 1),
        Err(UsbError::State { .. })
    ));
    assert!(matches!(
        XhciSeam::new(8, 8, 1, 0),
        Err(UsbError::State { .. })
    ));
}

#[test]
fn a_full_ring_refuses_the_whole_sequence() {
    let mut seam = XhciSeam::new(4, 8, 1, 1).expect("seam");
    assert_eq!(seam.free_slots(), 3);
    let transfer =
        ControlTransfer::new(setup(), 0x2000, 18, ControlDirection::In).expect("transfer");
    assert_eq!(seam.queue(&transfer), Ok(3));
    assert_eq!(
        seam.queue(&transfer),
        Err(UsbError::Ring(raven_rp1_xhci::RingError::Full))
    );
    assert_eq!(seam.pending(), 3);
    assert_eq!(seam.complete_transfer(), Some(0));
    assert_eq!(seam.complete_transfer(), Some(1));
    assert_eq!(seam.complete_transfer(), Some(2));
    assert_eq!(seam.free_slots(), 3);
}
