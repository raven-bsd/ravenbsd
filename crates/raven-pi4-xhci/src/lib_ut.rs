// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn public_model_is_reexported() {
    assert_eq!(XHCI_VENDOR_VIA, 0x1106);
    assert_eq!(MAX_PORTS, 8);
    assert_eq!(MAX_RING_ENTRIES, 64);
    assert_eq!(Trb::enable_slot().kind(), TrbType::EnableSlot);
    assert!(PortMachine::new().changes() == PortChanges::NONE);
}
