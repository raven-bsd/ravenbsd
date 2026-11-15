// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! BCM54213PE PHY rules: MII registers, identifier decoding, link resolution and RGMII delays.
//!
//! The Pi 4 wires the PHY in `rgmii-rxid` mode: the PHY adds the RX clock delay and the MAC
//! does not add a TX delay. The board device tree is the authority for the mode; these helpers
//! turn that mode into the three read-modify-write values the PHY driver needs.

use crate::regs::{
    EXT_RGMII_OOB_ID_MODE_DISABLE, EXT_RGMII_OOB_OOB_DISABLE, EXT_RGMII_OOB_RGMII_LINK,
    EXT_RGMII_OOB_RGMII_MODE_EN, UMAC_CMD_SPEED_10, UMAC_CMD_SPEED_100, UMAC_CMD_SPEED_1000,
};

/// MII basic control register.
pub const MII_BMCR: u8 = 0x00;
/// MII basic status register.
pub const MII_BMSR: u8 = 0x01;
/// MII PHY identifier register 1.
pub const MII_PHYIDR1: u8 = 0x02;
/// MII PHY identifier register 2.
pub const MII_PHYIDR2: u8 = 0x03;
/// MII autonegotiation advertisement register.
pub const MII_ANAR: u8 = 0x04;
/// MII autonegotiation link-partner ability register.
pub const MII_ANLPAR: u8 = 0x05;
/// MII autonegotiation expansion register.
pub const MII_ANER: u8 = 0x06;
/// MII 1000BASE-T control register.
pub const MII_GTCR: u8 = 0x09;
/// MII 1000BASE-T status register.
pub const MII_GTSR: u8 = 0x0a;
/// Broadcom auxiliary control register.
pub const BRGPHY_MII_AUXCTL: u8 = 0x18;
/// Broadcom auxiliary status register.
pub const BRGPHY_MII_AUXSTS: u8 = 0x19;
/// Broadcom interrupt status register.
pub const BRGPHY_MII_ISR: u8 = 0x1a;
/// Broadcom interrupt mask register.
pub const BRGPHY_MII_IMR: u8 = 0x1b;
/// Broadcom shadow register 0x1c.
pub const BRGPHY_MII_SHADOW_1C: u8 = 0x1c;

/// `BMCR`: reset the PHY.
pub const BMCR_RESET: u16 = 0x8000;
/// `BMCR`: enable autonegotiation.
pub const BMCR_ANENABLE: u16 = 0x1000;
/// `BMCR`: restart autonegotiation.
pub const BMCR_ANRESTART: u16 = 0x0200;
/// `BMCR`: full duplex.
pub const BMCR_DUPLEX: u16 = 0x0100;
/// `BMCR`: 100 Mbit/s selection.
pub const BMCR_SPEED100: u16 = 0x2000;

/// `BMSR`: link is up.
pub const BMSR_LINK: u16 = 0x0004;
/// `BMSR`: autonegotiation complete.
pub const BMSR_ACOMP: u16 = 0x0020;
/// `BMSR`: autonegotiation capable.
pub const BMSR_ANEG: u16 = 0x0008;
/// `BMSR`: extended status available.
pub const BMSR_EXTCAP: u16 = 0x0001;
/// `BMSR`: 100BASE-T4 capable.
pub const BMSR_100T4: u16 = 0x0080;
/// `BMSR`: 100BASE-TX full duplex capable.
pub const BMSR_100TXFDX: u16 = 0x0100;
/// `BMSR`: 100BASE-TX half duplex capable.
pub const BMSR_100TXHDX: u16 = 0x0200;
/// `BMSR`: 10BASE-T full duplex capable.
pub const BMSR_10TFDX: u16 = 0x0400;
/// `BMSR`: 10BASE-T half duplex capable.
pub const BMSR_10THDX: u16 = 0x0800;

/// `AUXCTL`: miscellaneous shadow function selector.
pub const AUXCTL_SHADOW_MISC: u16 = 0x0007;
/// `AUXCTL`: shadow write enable.
pub const AUXCTL_MISC_WRITE_EN: u16 = 0x8000;
/// `AUXCTL`: shadow data mask.
pub const AUXCTL_MISC_DATA_MASK: u16 = 0x7ff8;
/// `AUXCTL`: RGMII RX clock skew enable.
pub const AUXCTL_MISC_RGMII_SKEW_EN: u16 = 0x0200;

/// Shadow 0x1c: clock alignment control selector.
pub const SHADOW_1C_CLK_CTRL: u16 = 0x0c00;
/// Shadow 0x1c: write enable.
pub const SHADOW_1C_WRITE_EN: u16 = 0x8000;
/// Shadow 0x1c: data mask.
pub const SHADOW_1C_DATA_MASK: u16 = 0x03ff;
/// Shadow 0x1c: GTX clock delay enable.
pub const SHADOW_1C_GTXCLK_EN: u16 = 0x0200;

/// `AUXSTS`: resolved autonegotiation result mask.
pub const AUXSTS_AN_RES_MASK: u16 = 0x0700;
/// `AUXSTS`: 1000BASE-T full duplex.
pub const AUXSTS_RES_1000FD: u16 = 0x0700;
/// `AUXSTS`: 1000BASE-T half duplex.
pub const AUXSTS_RES_1000HD: u16 = 0x0600;
/// `AUXSTS`: 100BASE-TX full duplex.
pub const AUXSTS_RES_100FD: u16 = 0x0500;
/// `AUXSTS`: 100BASE-T4.
pub const AUXSTS_RES_100T4: u16 = 0x0400;
/// `AUXSTS`: 100BASE-TX half duplex.
pub const AUXSTS_RES_100HD: u16 = 0x0300;
/// `AUXSTS`: 10BASE-T full duplex.
pub const AUXSTS_RES_10FD: u16 = 0x0200;
/// `AUXSTS`: 10BASE-T half duplex.
pub const AUXSTS_RES_10HD: u16 = 0x0100;

/// Broadcom OUI 4.
pub const BCM54213PE_OUI: u32 = 0x0018_c086;
/// BCM54213PE model number.
pub const BCM54213PE_MODEL: u8 = 0x0a;
/// Default PHY address on the Pi 4.
pub const BCM54213PE_ADDRESS: u8 = 1;

/// One of the four RGMII delay modes in the device tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhyMode {
    /// No PHY-added delay.
    Rgmii,
    /// PHY adds both RX and TX delays.
    RgmiiId,
    /// PHY adds the RX delay.
    RgmiiRxid,
    /// PHY adds the TX delay.
    RgmiiTxid,
}

impl PhyMode {
    /// Parse the device-tree `phy-mode` value.
    #[must_use]
    pub fn from_dt(value: &str) -> Option<Self> {
        match value {
            "rgmii" => Some(Self::Rgmii),
            "rgmii-id" => Some(Self::RgmiiId),
            "rgmii-rxid" => Some(Self::RgmiiRxid),
            "rgmii-txid" => Some(Self::RgmiiTxid),
            _ => None,
        }
    }

    /// The device-tree spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rgmii => "rgmii",
            Self::RgmiiId => "rgmii-id",
            Self::RgmiiRxid => "rgmii-rxid",
            Self::RgmiiTxid => "rgmii-txid",
        }
    }

    /// True when the PHY adds the RX clock delay.
    #[must_use]
    pub const fn rx_delay(self) -> bool {
        matches!(self, Self::RgmiiId | Self::RgmiiRxid)
    }

    /// True when the PHY adds the TX clock delay.
    #[must_use]
    pub const fn tx_delay(self) -> bool {
        matches!(self, Self::RgmiiId | Self::RgmiiTxid)
    }
}

/// The PHY identifier decoded from registers 2 and 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhyId {
    /// Register 2.
    pub id1: u16,
    /// Register 3.
    pub id2: u16,
}

impl PhyId {
    /// Build from the two identifier registers.
    #[must_use]
    pub const fn from_regs(id1: u16, id2: u16) -> Self {
        Self { id1, id2 }
    }

    /// The canonical 24-bit OUI.
    #[must_use]
    pub const fn oui(self) -> u32 {
        let raw = ((self.id1 as u32) << 6) | ((self.id2 as u32) >> 10);
        reverse24(raw)
    }

    /// The six-bit model number.
    #[must_use]
    pub const fn model(self) -> u8 {
        ((self.id2 >> 4) & 0x3f) as u8
    }

    /// The four-bit revision.
    #[must_use]
    pub const fn revision(self) -> u8 {
        (self.id2 & 0x0f) as u8
    }

    /// True for a BCM54213PE.
    #[must_use]
    pub const fn is_bcm54213pe(self) -> bool {
        self.oui() == BCM54213PE_OUI && self.model() == BCM54213PE_MODEL
    }
}

const fn reverse_byte(value: u8) -> u8 {
    let mut input = value;
    let mut output = 0u8;
    let mut index = 0u8;
    while index < 8 {
        output = (output << 1) | (input & 1);
        input >>= 1;
        index += 1;
    }
    output
}

const fn reverse24(value: u32) -> u32 {
    ((reverse_byte((value >> 16) as u8) as u32) << 16)
        | ((reverse_byte((value >> 8) as u8) as u32) << 8)
        | reverse_byte(value as u8) as u32
}

/// A resolved link speed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// 10 Mbit/s.
    Ten,
    /// 100 Mbit/s.
    Hundred,
    /// 1000 Mbit/s.
    Thousand,
}

impl Speed {
    /// The `UMAC_CMD_SPEED` field value.
    #[must_use]
    pub const fn umac_bits(self) -> u32 {
        match self {
            Self::Ten => UMAC_CMD_SPEED_10,
            Self::Hundred => UMAC_CMD_SPEED_100,
            Self::Thousand => UMAC_CMD_SPEED_1000,
        }
    }

    /// Decode the `UMAC_CMD_SPEED` field.
    #[must_use]
    pub const fn from_umac_bits(bits: u32) -> Option<Self> {
        match bits {
            UMAC_CMD_SPEED_10 => Some(Self::Ten),
            UMAC_CMD_SPEED_100 => Some(Self::Hundred),
            UMAC_CMD_SPEED_1000 => Some(Self::Thousand),
            _ => None,
        }
    }
}

/// Link duplex.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duplex {
    /// Half duplex.
    Half,
    /// Full duplex.
    Full,
}

/// The link state decoded from `BMSR` and the Broadcom `AUXSTS` register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkStatus {
    /// True when `BMSR_LINK` is set.
    pub up: bool,
    /// Resolved speed when the link is up.
    pub speed: Option<Speed>,
    /// Resolved duplex when the link is up.
    pub duplex: Option<Duplex>,
    /// True when autonegotiation completed.
    pub autoneg_complete: bool,
}

/// Why a PHY status pair does not resolve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// The link is up but the auxiliary resolution field is not a known value.
    UnknownResolution(u16),
}

impl LinkStatus {
    /// Resolve `BMSR` and `AUXSTS` into a typed link state.
    pub fn resolve(bmsr: u16, auxsts: u16) -> Result<Self, LinkError> {
        let autoneg_complete = bmsr & BMSR_ACOMP != 0;
        if bmsr & BMSR_LINK == 0 {
            return Ok(Self {
                up: false,
                speed: None,
                duplex: None,
                autoneg_complete,
            });
        }
        let (speed, duplex) = match auxsts & AUXSTS_AN_RES_MASK {
            AUXSTS_RES_1000FD => (Speed::Thousand, Duplex::Full),
            AUXSTS_RES_1000HD => (Speed::Thousand, Duplex::Half),
            AUXSTS_RES_100FD => (Speed::Hundred, Duplex::Full),
            AUXSTS_RES_100T4 => (Speed::Hundred, Duplex::Full),
            AUXSTS_RES_100HD => (Speed::Hundred, Duplex::Half),
            AUXSTS_RES_10FD => (Speed::Ten, Duplex::Full),
            AUXSTS_RES_10HD => (Speed::Ten, Duplex::Half),
            other => return Err(LinkError::UnknownResolution(other)),
        };
        Ok(Self {
            up: true,
            speed: Some(speed),
            duplex: Some(duplex),
            autoneg_complete,
        })
    }

    /// The `UMAC_CMD_SPEED` field for the current link, if any.
    #[must_use]
    pub const fn umac_speed_bits(self) -> Option<u32> {
        match self.speed {
            Some(speed) => Some(speed.umac_bits()),
            None => None,
        }
    }
}

/// One stage of the PHY bring-up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhyInitStep {
    /// Reset the PHY.
    Reset,
    /// Read the identifier registers.
    Identify,
    /// Program the RGMII delay shadow registers.
    SetRgmiiDelay,
    /// Start autonegotiation.
    StartAutoneg,
    /// Poll the link state.
    WaitLink,
    /// The link is ready.
    Ready,
}

/// The ordered bring-up sequence.
pub const PHY_INIT_STEPS: [PhyInitStep; 6] = [
    PhyInitStep::Reset,
    PhyInitStep::Identify,
    PhyInitStep::SetRgmiiDelay,
    PhyInitStep::StartAutoneg,
    PhyInitStep::WaitLink,
    PhyInitStep::Ready,
];

/// A small typed cursor over [`PHY_INIT_STEPS`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhyInit {
    index: usize,
}

impl PhyInit {
    /// Start at the first step.
    #[must_use]
    pub const fn new() -> Self {
        Self { index: 0 }
    }

    /// The current step, or `None` after [`PhyInitStep::Ready`].
    #[must_use]
    pub fn current(self) -> Option<PhyInitStep> {
        PHY_INIT_STEPS.get(self.index).copied()
    }

    /// Advance one step and return the new current step.
    pub fn advance(&mut self) -> Option<PhyInitStep> {
        if self.index + 1 < PHY_INIT_STEPS.len() {
            self.index += 1;
        }
        self.current()
    }

    /// True when the sequence reached `Ready`.
    #[must_use]
    pub fn is_ready(self) -> bool {
        self.current() == Some(PhyInitStep::Ready)
    }
}

impl Default for PhyInit {
    fn default() -> Self {
        Self::new()
    }
}

/// The `AUXCTL` write that sets or clears the RGMII RX skew.
#[must_use]
pub const fn rgmii_auxctl_write(current: u16, mode: PhyMode) -> u16 {
    let mut value = current & AUXCTL_MISC_DATA_MASK;
    if mode.rx_delay() {
        value |= AUXCTL_MISC_RGMII_SKEW_EN;
    } else {
        value &= !AUXCTL_MISC_RGMII_SKEW_EN;
    }
    AUXCTL_MISC_WRITE_EN | AUXCTL_SHADOW_MISC | value
}

/// The shadow 0x1c write that sets or clears the GTX clock delay.
#[must_use]
pub const fn rgmii_shadow_1c_write(current: u16, mode: PhyMode) -> u16 {
    let mut value = current & SHADOW_1C_DATA_MASK;
    if mode.tx_delay() {
        value |= SHADOW_1C_GTXCLK_EN;
    } else {
        value &= !SHADOW_1C_GTXCLK_EN;
    }
    SHADOW_1C_WRITE_EN | SHADOW_1C_CLK_CTRL | value
}

/// Update `EXT_RGMII_OOB_CTRL` for the mode and link state.
#[must_use]
pub const fn ext_rgmii_oob_ctrl(current: u32, mode: PhyMode, link_up: bool) -> u32 {
    let mut value = current & !EXT_RGMII_OOB_OOB_DISABLE;
    if link_up {
        value |= EXT_RGMII_OOB_RGMII_LINK;
    } else {
        value &= !EXT_RGMII_OOB_RGMII_LINK;
    }
    value |= EXT_RGMII_OOB_RGMII_MODE_EN;
    if matches!(mode, PhyMode::Rgmii) {
        value |= EXT_RGMII_OOB_ID_MODE_DISABLE;
    } else {
        value &= !EXT_RGMII_OOB_ID_MODE_DISABLE;
    }
    value
}

#[cfg(test)]
#[path = "phy_ut.rs"]
mod tests;
