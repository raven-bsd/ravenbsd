// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the deterministic board bring-up record.

use super::*;
use crate::tests::{controller, ready_controller};
use raven_genet::mac::{MacAddress, MacOrigin};

fn sample_mac() -> MacSelection {
    MacSelection {
        address: MacAddress::new([0xd8, 0x3a, 0xdd, 0x12, 0x34, 0x56]),
        origin: MacOrigin::DeviceTree,
    }
}

#[test]
fn capture_requires_the_ready_stage() {
    let mut controller = controller();
    assert_eq!(
        BoardRecord::capture(&mut controller, sample_mac()),
        Err(DriverError::WrongStage {
            expected: Stage::Ready,
            actual: Stage::Attached,
        })
    );
}

#[test]
fn capture_reads_the_bring_up_values() {
    let mut controller = ready_controller();
    let record = BoardRecord::capture(&mut controller, sample_mac()).expect("record");
    assert_eq!(record.revision, 0x0600_0000);
    assert_eq!(record.port_ctrl, 3);
    assert_eq!(record.umac_mac0, 0xd83a_dd12);
    assert_eq!(record.umac_mac1, 0x3456);
    assert_eq!(record.phy_id.id1, 0x600d);
    assert_eq!(record.phy_id.id2, 0x84a0);
    assert_eq!(record.oob_ctrl, 0x50);
    assert_eq!(record.auxctl, 0x8207);
    assert_eq!(record.shadow_1c, 0x8c00);
}

#[test]
fn display_writes_the_exact_serial_markers() {
    let mut controller = ready_controller();
    let record = BoardRecord::capture(&mut controller, sample_mac()).expect("record");
    assert_eq!(
        std::format!("{record}"),
        "GENET-REV raw=0x06000000 major=6 minor=0 v5=true\n\
         GENET-PORT ctrl=0x00000003\n\
         GENET-MAC source=dtb addr=d8:3a:dd:12:34:56\n\
         GENET-UMAC mac0=0xd83add12 mac1=0x00003456\n\
         GENET-PHY id1=0x600d id2=0x84a0 oui=0x18c086 model=0x0a bcm54213pe=true\n\
         GENET-RGMII oob=0x00000050 auxctl=0x8207 shadow=0x8c00\n"
    );
}

#[test]
fn the_marker_list_is_complete() {
    assert_eq!(MARKERS.len(), 6);
    for marker in MARKERS {
        assert!(marker.starts_with("GENET-"));
    }
}

#[test]
fn a_stuck_mdio_transaction_times_out() {
    let mut controller = ready_controller();
    controller.seam.set_mdio_busy(true);
    assert_eq!(
        BoardRecord::capture(&mut controller, sample_mac()),
        Err(DriverError::MdioTimeout)
    );
}
