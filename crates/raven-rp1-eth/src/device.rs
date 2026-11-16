// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The Ethernet node from the RP1 device tree.
//!
//! The Pi 5 node is `raspberrypi,rp1-gem` + `cdns,macb` with an RGMII PHY child. This module
//! copies the properties the bring-up needs: the register window, interrupt, PHY address and mode,
//! reset GPIO and duration, clock references, the local MAC address and the Cadence pipe hints.
//! The window and the values are OBSERVED; the register layout is not.

use core::fmt;
use core::str;

use raven_fdt_lite::{Fdt, FdtError, Node};
use raven_pci::Region;
use raven_rp1::{Name, NameError};

use crate::mac::MacAddress;

/// Largest number of clock references the model holds.
pub const MAX_ETH_CLOCKS: usize = 8;

/// The RGMII delay mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhyMode {
    /// No internal delay.
    Rgmii,
    /// Internal delay on both directions.
    RgmiiId,
    /// Internal delay on TX only.
    RgmiiTxId,
    /// Internal delay on RX only.
    RgmiiRxId,
    /// A mode the model does not classify; the tree's value is not kept.
    Unknown,
}

impl PhyMode {
    fn parse(text: &str) -> Self {
        match text {
            "rgmii" => Self::Rgmii,
            "rgmii-id" => Self::RgmiiId,
            "rgmii-txid" => Self::RgmiiTxId,
            "rgmii-rxid" => Self::RgmiiRxId,
            _ => Self::Unknown,
        }
    }
}

/// The Ethernet interrupt line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptRef {
    /// Interrupt number inside the RP1 interrupt domain.
    pub id: u32,
    /// Two-cell interrupt flags from the tree.
    pub flags: u32,
}

/// The PHY child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PhyRef {
    /// MDIO address.
    pub address: u8,
    /// True when the tree asks for power-down support.
    pub powerdown: bool,
}

/// One clock reference from the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockRef {
    /// Clock ID (the second cell of the reference pair).
    pub id: u8,
    /// Clock name from `clock-names`.
    pub name: Name,
}

/// The PHY reset GPIO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetGpio {
    /// GPIO number.
    pub pin: u8,
    /// True when the line is active-low (the tree's flag bit 0).
    pub active_low: bool,
}

/// Why the Ethernet node could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EthError {
    /// No RP1 Ethernet node was found.
    MissingNode,
    /// A device-tree value is malformed.
    Fdt(FdtError),
    /// The node has no usable `reg` window.
    BadReg,
    /// A property has the wrong cell count.
    BadCells(&'static str),
    /// `local-mac-address` is not six bytes.
    BadMac,
    /// More clock references than [`MAX_ETH_CLOCKS`].
    TooManyClocks,
    /// A clock ID does not fit one byte.
    BadClockId(u32),
    /// `clocks` and `clock-names` disagree.
    ClockNames {
        /// Clock references.
        clocks: usize,
        /// Names.
        names: usize,
    },
    /// The PHY address does not fit one byte.
    BadPhyAddress(u32),
    /// The reset GPIO number does not fit one byte.
    BadGpioPin(u32),
    /// A clock name is empty or too long.
    Name(NameError),
}

impl fmt::Display for EthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNode => f.write_str("no RP1 Ethernet node"),
            Self::Fdt(error) => write!(f, "device tree: {error}"),
            Self::BadReg => f.write_str("Ethernet node has no usable reg window"),
            Self::BadCells(what) => write!(f, "malformed {what} cells"),
            Self::BadMac => f.write_str("local-mac-address is not six bytes"),
            Self::TooManyClocks => write!(f, "more than {MAX_ETH_CLOCKS} clock references"),
            Self::BadClockId(id) => write!(f, "clock id {id} does not fit one byte"),
            Self::ClockNames { clocks, names } => {
                write!(f, "{clocks} clock references but {names} names")
            }
            Self::BadPhyAddress(address) => {
                write!(f, "PHY address {address} does not fit one byte")
            }
            Self::BadGpioPin(pin) => write!(f, "GPIO pin {pin} does not fit one byte"),
            Self::Name(error) => write!(f, "name: {error}"),
        }
    }
}

impl core::error::Error for EthError {}

impl From<FdtError> for EthError {
    fn from(error: FdtError) -> Self {
        Self::Fdt(error)
    }
}

impl From<NameError> for EthError {
    fn from(error: NameError) -> Self {
        Self::Name(error)
    }
}

/// The parsed Ethernet node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EthernetNode {
    /// Register window (RP1-local).
    pub window: Region,
    /// True when the node lists the `cdns,macb` compatible.
    pub cadence: bool,
    /// Interrupt line, when the tree has one.
    pub interrupt: Option<InterruptRef>,
    /// Clock references, `None` past `clock_count`.
    pub clocks: [Option<ClockRef>; MAX_ETH_CLOCKS],
    /// Number of clock references.
    pub clock_count: u8,
    /// RGMII mode, when the tree has one.
    pub phy_mode: Option<PhyMode>,
    /// Local MAC address, when it is not the all-zero placeholder.
    pub mac: Option<MacAddress>,
    /// `cdns,aw2w-max-pipe`.
    pub aw2w_max_pipe: Option<u8>,
    /// `cdns,ar2r-max-pipe`.
    pub ar2r_max_pipe: Option<u8>,
    /// `cdns,use-aw2b-fill`.
    pub use_aw2b_fill: bool,
    /// PHY reset line.
    pub reset: Option<ResetGpio>,
    /// PHY reset duration in milliseconds.
    pub reset_duration_ms: Option<u32>,
    /// PHY child.
    pub phy: Option<PhyRef>,
}

impl EthernetNode {
    /// Parse the Ethernet node from the device tree.
    ///
    /// # Errors
    ///
    /// Returns [`EthError`] for a missing node, a malformed `reg`/interrupt/clock/reset property
    /// or an invalid MAC/PHY address.
    pub fn from_fdt(fdt: &Fdt<'_>) -> Result<Self, EthError> {
        let node = fdt
            .find_compatible(&["raspberrypi,rp1-gem"])
            .or_else(|| fdt.find_compatible(&["cdns,macb"]))
            .ok_or(EthError::MissingNode)?;
        let parent = node.parent().ok_or(EthError::BadReg)?;
        let mut reg = node
            .reg_checked(parent.address_cells(), parent.size_cells())
            .map_err(EthError::Fdt)?;
        let (base, size) = reg.next().ok_or(EthError::BadReg)?;
        let window = Region::new(base, size).ok_or(EthError::BadReg)?;

        let interrupt = parse_interrupt(&node)?;
        let (clocks, clock_count) = parse_clocks(&node)?;
        let mac = parse_mac(&node)?;
        let reset = parse_reset(&node)?;
        let phy = parse_phy(&node)?;

        Ok(Self {
            window,
            cadence: node.compatible_with("cdns,macb"),
            interrupt,
            clocks,
            clock_count,
            phy_mode: node.property_str("phy-mode").map(PhyMode::parse),
            mac,
            aw2w_max_pipe: small_u32(&node, "cdns,aw2w-max-pipe")?,
            ar2r_max_pipe: small_u32(&node, "cdns,ar2r-max-pipe")?,
            use_aw2b_fill: node.property("cdns,use-aw2b-fill").is_some(),
            reset,
            reset_duration_ms: node.u32_property("phy-reset-duration"),
            phy,
        })
    }
}

fn parse_interrupt(node: &Node<'_>) -> Result<Option<InterruptRef>, EthError> {
    let Some(value) = node.property("interrupts") else {
        return Ok(None);
    };
    if !value.len().is_multiple_of(8) {
        return Err(EthError::BadCells("interrupts"));
    }
    let mut cells = node.cells("interrupts", 4);
    let (Some(id), Some(flags)) = (cells.next(), cells.next()) else {
        return Ok(None);
    };
    Ok(Some(InterruptRef {
        id: u32::try_from(id).map_err(|_| EthError::BadCells("interrupt id"))?,
        flags: u32::try_from(flags).map_err(|_| EthError::BadCells("interrupt flags"))?,
    }))
}

fn parse_clocks(node: &Node<'_>) -> Result<([Option<ClockRef>; MAX_ETH_CLOCKS], u8), EthError> {
    let mut out = [None; MAX_ETH_CLOCKS];
    let clock_cells = node.property("clocks").map_or(0, |value| value.len() / 8);
    let names = node.property("clock-names");
    let name_count = names.map_or(0, count_strings);
    if clock_cells != name_count {
        return Err(EthError::ClockNames {
            clocks: clock_cells,
            names: name_count,
        });
    }
    if clock_cells > MAX_ETH_CLOCKS {
        return Err(EthError::TooManyClocks);
    }
    if let Some(value) = node.property("clocks")
        && !value.len().is_multiple_of(8)
    {
        return Err(EthError::BadCells("clocks"));
    }
    let mut cells = node.cells("clocks", 4);
    let mut labels = names.map(|value| {
        value
            .split(|byte| *byte == 0)
            .filter(|entry| !entry.is_empty())
    });
    let mut count = 0u8;
    while let (Some(_phandle), Some(id)) = (cells.next(), cells.next()) {
        let id = u8::try_from(id).map_err(|_| EthError::BadClockId(id as u32))?;
        let Some(label) = labels.as_mut().and_then(Iterator::next) else {
            return Err(EthError::ClockNames {
                clocks: clock_cells,
                names: name_count,
            });
        };
        let name =
            Name::new(str::from_utf8(label).map_err(|_| EthError::BadCells("clock-names"))?)?;
        if let Some(slot) = out.get_mut(usize::from(count)) {
            *slot = Some(ClockRef { id, name });
            count = count.saturating_add(1);
        }
    }
    Ok((out, count))
}

fn count_strings(value: &[u8]) -> usize {
    value
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .count()
}

fn parse_mac(node: &Node<'_>) -> Result<Option<MacAddress>, EthError> {
    let Some(value) = node.property("local-mac-address") else {
        return Ok(None);
    };
    let address = MacAddress::from_bytes(value).map_err(|_| EthError::BadMac)?;
    // The firmware tree carries the all-zero placeholder until the bootloader fills it.
    if address.octets() == [0; 6] {
        return Ok(None);
    }
    Ok(Some(address))
}

fn parse_reset(node: &Node<'_>) -> Result<Option<ResetGpio>, EthError> {
    let Some(value) = node.property("phy-reset-gpios") else {
        return Ok(None);
    };
    if value.len() != 12 {
        return Err(EthError::BadCells("phy-reset-gpios"));
    }
    let mut cells = node.cells("phy-reset-gpios", 4);
    let Some(_phandle) = cells.next() else {
        return Ok(None);
    };
    let Some(pin) = cells.next() else {
        return Err(EthError::BadCells("phy-reset-gpios"));
    };
    let Some(flags) = cells.next() else {
        return Err(EthError::BadCells("phy-reset-gpios"));
    };
    Ok(Some(ResetGpio {
        pin: u8::try_from(pin).map_err(|_| EthError::BadGpioPin(pin as u32))?,
        active_low: flags & 1 != 0,
    }))
}

fn parse_phy(node: &Node<'_>) -> Result<Option<PhyRef>, EthError> {
    let Some(phy) = node
        .children()
        .find(|child| child.name().starts_with("ethernet-phy@"))
    else {
        return Ok(None);
    };
    let mut reg = phy
        .reg_checked(node.address_cells(), node.size_cells())
        .map_err(EthError::Fdt)?;
    let (address, _size) = reg.next().ok_or(EthError::BadCells("ethernet-phy reg"))?;
    Ok(Some(PhyRef {
        address: u8::try_from(address).map_err(|_| EthError::BadPhyAddress(address as u32))?,
        powerdown: phy.property("brcm,powerdown-enable").is_some(),
    }))
}

fn small_u32(node: &Node<'_>, property: &str) -> Result<Option<u8>, EthError> {
    match node.u32_property(property) {
        Some(value) => u8::try_from(value)
            .map(Some)
            .map_err(|_| EthError::BadCells("cdns pipe width")),
        None => Ok(None),
    }
}

#[cfg(test)]
#[path = "device_ut.rs"]
mod tests;
