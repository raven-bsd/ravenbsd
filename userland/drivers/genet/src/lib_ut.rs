// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the GENET driver seam and typed bring-up.

use super::*;
use raven_genet::dtb::DmaMap;
use raven_genet::phy::PhyMode;

const REG_WORDS: usize = 0x5000 / 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FakeSeam {
    registers: [u32; REG_WORDS],
    fail_reads: bool,
}

impl FakeSeam {
    pub(crate) fn new() -> Self {
        Self {
            registers: [0; REG_WORDS],
            fail_reads: false,
        }
    }

    pub(crate) fn info() -> GenetInfo {
        GenetInfo {
            base: 0xfd58_0000,
            size: 0x1_0000,
            mdio_offset: 0xe14,
            mdio_size: 8,
            interrupts: [157, 158],
            interrupt_flags: [4, 4],
            phy_address: 1,
            phy_mode: PhyMode::RgmiiRxid,
            mac: Some(MacAddress::new([0x02, 0, 0, 0, 0, 0x42])),
            dma: DmaMap::identity(),
        }
    }

    pub(crate) fn register(&self, offset: usize) -> u32 {
        self.registers[offset / 4]
    }

    pub(crate) fn set_register(&mut self, offset: usize, value: u32) {
        self.registers[offset / 4] = value;
    }
}

impl Seam for FakeSeam {
    fn read32(&mut self, offset: usize) -> Result<u32, SeamError> {
        if self.fail_reads {
            return Err(SeamError::Io);
        }
        if !offset.is_multiple_of(4) {
            return Err(SeamError::Alignment);
        }
        self.registers
            .get(offset / 4)
            .copied()
            .ok_or(SeamError::Bounds)
    }

    fn write32(&mut self, offset: usize, value: u32) -> Result<(), SeamError> {
        if !offset.is_multiple_of(4) {
            return Err(SeamError::Alignment);
        }
        match self.registers.get_mut(offset / 4) {
            Some(slot) => {
                *slot = value;
                Ok(())
            }
            None => Err(SeamError::Bounds),
        }
    }

    fn rx_buffer(&mut self, index: usize) -> Option<Buffer> {
        if index < 8 {
            Some(Buffer {
                bus_addr: 0x1000 + index as u64 * 0x800,
                len: 2048,
            })
        } else {
            None
        }
    }

    fn tx_buffer(&mut self, index: usize) -> Option<Buffer> {
        if index < 8 {
            Some(Buffer {
                bus_addr: 0x8000 + index as u64 * 0x800,
                len: 2048,
            })
        } else {
            None
        }
    }
}

pub(crate) fn controller() -> Controller<FakeSeam> {
    Controller::attach(FakeSeam::new(), FakeSeam::info()).expect("attach")
}

pub(crate) fn ready_controller() -> Controller<FakeSeam> {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 6 << regs::SYS_REV_MAJOR_SHIFT);
    controller.probe().expect("probe");
    controller
        .program_mac(MacAddress::new([0xd8, 0x3a, 0xdd, 0x12, 0x34, 0x56]))
        .expect("mac");
    controller.program_rgmii(true).expect("rgmii");
    controller.ready().expect("ready");
    controller
}

#[test]
fn a_valid_device_tree_attaches_at_the_first_stage() {
    let controller = controller();
    assert_eq!(controller.stage(), Stage::Attached);
    assert_eq!(controller.info().base, 0xfd58_0000);
    assert_eq!(controller.rx().count(), 256);
    assert_eq!(controller.tx().count(), 256);
}

#[test]
fn an_implausible_window_is_rejected_at_attach() {
    let mut info = FakeSeam::info();
    info.size = 0x1000;
    assert_eq!(
        Controller::attach(FakeSeam::new(), info),
        Err(DriverError::BadInfo)
    );
}

#[test]
fn a_mdio_window_outside_the_genet_window_is_rejected() {
    let mut info = FakeSeam::info();
    info.mdio_offset = 0xffff_0000;
    assert_eq!(
        Controller::attach(FakeSeam::new(), info),
        Err(DriverError::BadInfo)
    );
}

#[test]
fn probe_accepts_the_raw_major_six_revision() {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 6 << regs::SYS_REV_MAJOR_SHIFT);
    assert_eq!(controller.probe(), Ok(6 << regs::SYS_REV_MAJOR_SHIFT));
    assert_eq!(controller.stage(), Stage::Probed);
}

#[test]
fn probe_rejects_a_foreign_revision() {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 3 << regs::SYS_REV_MAJOR_SHIFT);
    assert_eq!(
        controller.probe(),
        Err(DriverError::WrongRevision(3 << regs::SYS_REV_MAJOR_SHIFT))
    );
    assert_eq!(controller.stage(), Stage::Attached);
}

#[test]
fn programming_mac_before_probe_is_rejected() {
    let mut controller = controller();
    assert_eq!(
        controller.program_mac(MacAddress::new([0x02, 0, 0, 0, 0, 1])),
        Err(DriverError::WrongStage {
            expected: Stage::Probed,
            actual: Stage::Attached,
        })
    );
}

#[test]
fn the_full_sequence_programs_the_documented_registers() {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 6 << regs::SYS_REV_MAJOR_SHIFT);
    controller.probe().expect("probe");

    let address = MacAddress::new([0xd8, 0x3a, 0xdd, 0x12, 0x34, 0x56]);
    controller.program_mac(address).expect("mac");
    assert_eq!(controller.stage(), Stage::MacProgrammed);
    assert_eq!(controller.seam.register(regs::UMAC_MAC0), 0xd83a_dd12);
    assert_eq!(controller.seam.register(regs::UMAC_MAC1), 0x3456);

    let oob = controller.program_rgmii(true).expect("rgmii");
    assert_eq!(controller.stage(), Stage::RgmiiProgrammed);
    assert_eq!(controller.seam.register(regs::SYS_PORT_CTRL), 3);
    assert_eq!(
        oob & regs::EXT_RGMII_OOB_RGMII_LINK,
        regs::EXT_RGMII_OOB_RGMII_LINK
    );
    assert_eq!(
        oob & regs::EXT_RGMII_OOB_RGMII_MODE_EN,
        regs::EXT_RGMII_OOB_RGMII_MODE_EN
    );
    assert_eq!(oob & regs::EXT_RGMII_OOB_OOB_DISABLE, 0);
    assert_eq!(controller.seam.register(regs::EXT_RGMII_OOB_CTRL), oob);

    controller.ready().expect("ready");
    assert_eq!(controller.stage(), Stage::Ready);
}

#[test]
fn an_invalid_mac_is_rejected_before_a_register_write() {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 6 << regs::SYS_REV_MAJOR_SHIFT);
    controller.probe().expect("probe");
    assert_eq!(
        controller.program_mac(MacAddress::new([0; 6])),
        Err(DriverError::InvalidMac)
    );
    assert_eq!(controller.stage(), Stage::Probed);
    assert_eq!(controller.seam.register(regs::UMAC_MAC0), 0);
}

#[test]
fn ready_is_rejected_before_the_rgmii_step() {
    let mut controller = controller();
    controller
        .seam
        .set_register(regs::SYS_REV_CTRL, 6 << regs::SYS_REV_MAJOR_SHIFT);
    controller.probe().expect("probe");
    controller
        .program_mac(MacAddress::new([0x02, 0, 0, 0, 0, 1]))
        .expect("mac");
    assert_eq!(
        controller.ready(),
        Err(DriverError::WrongStage {
            expected: Stage::RgmiiProgrammed,
            actual: Stage::MacProgrammed,
        })
    );
}

#[test]
fn the_ring_setup_is_the_pi4_default() {
    let setup = Controller::<FakeSeam>::ring_setup();
    assert_eq!(setup.desc_count, 256);
    assert_eq!(setup.buffer_len, 2048);
    assert_eq!(setup.end_address, 767);
    assert_eq!(setup.xon_xoff, 0x0005_0010);
}

#[test]
fn rx_and_tx_descriptors_use_the_seam_buffers() {
    let mut controller = controller();
    let rx = controller
        .rx_descriptor(1, RxStatus::from_raw(0))
        .expect("rx descriptor");
    assert_eq!(rx.address, 0x1800);
    let tx = controller
        .tx_descriptor(2, TxStatus::single(64).expect("status"))
        .expect("tx descriptor");
    assert_eq!(tx.address, 0x9000);
}

#[test]
fn a_missing_packet_buffer_is_reported() {
    let mut controller = controller();
    assert_eq!(
        controller.rx_descriptor(8, RxStatus::from_raw(0)),
        Err(DriverError::MissingBuffer(8))
    );
    assert_eq!(
        controller.tx_descriptor(8, TxStatus::single(64).expect("status")),
        Err(DriverError::MissingBuffer(8))
    );
}

#[test]
fn the_ring_indices_advance_and_wrap() {
    let mut controller = controller();
    controller.publish_rx(3).expect("publish rx");
    assert_eq!(controller.rx().used(), Ok(3));
    controller.retire_rx(2).expect("retire rx");
    assert_eq!(controller.rx().used(), Ok(1));
    controller.publish_tx(2).expect("publish tx");
    controller.retire_tx(2).expect("retire tx");
    assert!(controller.tx().is_empty());
    assert_eq!(
        controller.retire_tx(1),
        Err(DriverError::Ring(RingError::Empty))
    );
}

#[test]
fn a_seam_read_error_is_mapped_into_the_driver_error() {
    let mut controller = controller();
    controller.seam.fail_reads = true;
    assert_eq!(controller.probe(), Err(DriverError::Seam(SeamError::Io)));
}

#[test]
fn an_unaligned_fake_access_is_rejected() {
    let mut seam = FakeSeam::new();
    assert_eq!(seam.read32(1), Err(SeamError::Alignment));
    assert_eq!(seam.write32(1, 0), Err(SeamError::Alignment));
}
