// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Typed board bring-up record for the serial log.
//!
//! The record captures the values the O2/O3 model can read back through the seam: the GENET
//! revision and port control, the selected MAC source and the UMAC registers, the PHY identifier,
//! and the RGMII OOB/AUXCTL/shadow read-backs. Its [`Display`] output is deterministic; the
//! evidence script greps the `GENET-*` markers and hashes the complete log.

use core::fmt;

use crate::{Controller, DriverError, Seam, Stage};
use raven_genet::mac::{MacOrigin, MacSelection};
use raven_genet::mdio::{MDIO_POLL_INTERVAL_US, MdioState, MdioTransaction};
use raven_genet::phy::{BRGPHY_MII_AUXCTL, BRGPHY_MII_SHADOW_1C, MII_PHYIDR1, MII_PHYIDR2, PhyId};
use raven_genet::regs;

/// Stable marker for the revision line.
pub const MARKER_REVISION: &str = "GENET-REV";
/// Stable marker for the port-control line.
pub const MARKER_PORT: &str = "GENET-PORT";
/// Stable marker for the MAC-source line.
pub const MARKER_MAC: &str = "GENET-MAC";
/// Stable marker for the UMAC MAC-register line.
pub const MARKER_UMAC: &str = "GENET-UMAC";
/// Stable marker for the PHY identifier line.
pub const MARKER_PHY: &str = "GENET-PHY";
/// Stable marker for the RGMII read-back line.
pub const MARKER_RGMII: &str = "GENET-RGMII";

/// Every marker the evidence script requires.
pub const MARKERS: [&str; 6] = [
    MARKER_REVISION,
    MARKER_PORT,
    MARKER_MAC,
    MARKER_UMAC,
    MARKER_PHY,
    MARKER_RGMII,
];

/// One read-only board bring-up record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoardRecord {
    /// Raw `SYS_REV_CTRL`.
    pub revision: u32,
    /// Raw `SYS_PORT_CTRL`.
    pub port_ctrl: u32,
    /// Selected MAC and its source.
    pub mac: MacSelection,
    /// Raw `UMAC_MAC0`.
    pub umac_mac0: u32,
    /// Raw `UMAC_MAC1`.
    pub umac_mac1: u32,
    /// Decoded MDIO identifier.
    pub phy_id: PhyId,
    /// Raw `EXT_RGMII_OOB_CTRL`.
    pub oob_ctrl: u32,
    /// MDIO register 0x18 read-back.
    pub auxctl: u16,
    /// MDIO shadow register 0x1c read-back.
    pub shadow_1c: u16,
}

impl BoardRecord {
    /// Capture the host-observable bring-up values after [`Stage::Ready`].
    pub fn capture<S: Seam>(
        controller: &mut Controller<S>,
        mac: MacSelection,
    ) -> Result<Self, DriverError> {
        if controller.stage() != Stage::Ready {
            return Err(DriverError::WrongStage {
                expected: Stage::Ready,
                actual: controller.stage(),
            });
        }
        let phy = controller.info().phy_address;
        let revision = controller.seam.read32(regs::SYS_REV_CTRL)?;
        let port_ctrl = controller.seam.read32(regs::SYS_PORT_CTRL)?;
        let umac_mac0 = controller.seam.read32(regs::UMAC_MAC0)?;
        let umac_mac1 = controller.seam.read32(regs::UMAC_MAC1)?;
        let oob_ctrl = controller.seam.read32(regs::EXT_RGMII_OOB_CTRL)?;
        let id1 = read_mdio(&mut controller.seam, phy, MII_PHYIDR1)?;
        let id2 = read_mdio(&mut controller.seam, phy, MII_PHYIDR2)?;
        let auxctl = read_mdio(&mut controller.seam, phy, BRGPHY_MII_AUXCTL)?;
        let shadow_1c = read_mdio(&mut controller.seam, phy, BRGPHY_MII_SHADOW_1C)?;
        Ok(Self {
            revision,
            port_ctrl,
            mac,
            umac_mac0,
            umac_mac1,
            phy_id: PhyId::from_regs(id1, id2),
            oob_ctrl,
            auxctl,
            shadow_1c,
        })
    }
}

impl fmt::Display for BoardRecord {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let raw_major = regs::revision_major(self.revision);
        let source = match self.mac.origin {
            MacOrigin::DeviceTree => "dtb",
            MacOrigin::Mailbox => "mailbox",
            MacOrigin::Hardware => "hardware",
        };
        writeln!(
            formatter,
            "{MARKER_REVISION} raw=0x{:08x} major={} minor={} v5={}",
            self.revision,
            raw_major,
            regs::revision_minor(self.revision),
            regs::revision_is_v5(self.revision)
        )?;
        writeln!(formatter, "{MARKER_PORT} ctrl=0x{:08x}", self.port_ctrl)?;
        writeln!(
            formatter,
            "{MARKER_MAC} source={source} addr={}",
            self.mac.address
        )?;
        writeln!(
            formatter,
            "{MARKER_UMAC} mac0=0x{:08x} mac1=0x{:08x}",
            self.umac_mac0, self.umac_mac1
        )?;
        writeln!(
            formatter,
            "{MARKER_PHY} id1=0x{:04x} id2=0x{:04x} oui=0x{:06x} model=0x{:02x} bcm54213pe={}",
            self.phy_id.id1,
            self.phy_id.id2,
            self.phy_id.oui(),
            self.phy_id.model(),
            self.phy_id.is_bcm54213pe()
        )?;
        writeln!(
            formatter,
            "{MARKER_RGMII} oob=0x{:08x} auxctl=0x{:04x} shadow=0x{:04x}",
            self.oob_ctrl, self.auxctl, self.shadow_1c
        )
    }
}

pub(crate) fn read_mdio<S: Seam>(seam: &mut S, phy: u8, register: u8) -> Result<u16, DriverError> {
    let mut transaction = MdioTransaction::start_read(phy, register)?;
    seam.write32(regs::MDIO_CMD, transaction.command().register_value())?;
    loop {
        let value = seam.read32(regs::MDIO_CMD)?;
        match transaction.observe(value) {
            MdioState::Waiting => seam.delay_us(MDIO_POLL_INTERVAL_US),
            MdioState::ReadComplete(value) => return Ok(value),
            MdioState::WriteComplete | MdioState::TimedOut => {
                return Err(DriverError::MdioTimeout);
            }
        }
    }
}

#[cfg(test)]
#[path = "board_ut.rs"]
mod tests;
