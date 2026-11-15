// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RP1 USB controllers from the device tree.
//!
//! Each RP1 USB function is a Synopsys DesignWare USB3 core (`snps,dwc3`) with a 1 MiB register
//! window and an RP1 interrupt. This module copies the window, interrupt, role, LPM and quirk
//! properties. The xHCI register block inside the DesignWare wrapper is not described by the
//! tree; its offset is UNVERIFIED and the caller supplies the controller base to the xHCI model.

use core::fmt;

use raven_fdt_lite::{Fdt, FdtError, Node};
use raven_pci::Region;

/// Largest number of controllers the model holds.
pub const MAX_USB_CONTROLLERS: usize = 2;

/// The USB role from `dr_mode`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrMode {
    /// Host only.
    Host,
    /// Peripheral only.
    Peripheral,
    /// Dual role.
    Otg,
    /// A mode the model does not classify.
    Unknown,
}

impl DrMode {
    fn parse(text: &str) -> Self {
        match text {
            "host" => Self::Host,
            "peripheral" => Self::Peripheral,
            "otg" => Self::Otg,
            _ => Self::Unknown,
        }
    }
}

/// The Synopsys quirk flags the tree carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Quirks {
    /// Disable the RxDetect in P3 quirk.
    pub dis_rxdet_inp3: bool,
    /// Enhanced NAK for full-speed quirk.
    pub enhanced_nak_fs: bool,
    /// Disable park mode for super-speed.
    pub parkmode_disable_ss: bool,
    /// Disable park mode for full/low speed.
    pub parkmode_disable_fsls: bool,
}

/// One RP1 interrupt descriptor (two-cell format).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InterruptRef {
    /// Interrupt number inside the RP1 domain.
    pub id: u32,
    /// Interrupt flags.
    pub flags: u32,
}

/// One RP1 USB controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbController {
    /// Zero-based controller index in tree order.
    pub index: u8,
    /// Register window (RP1-local).
    pub window: Region,
    /// Interrupt line, when the tree has one.
    pub interrupt: Option<InterruptRef>,
    /// `dr_mode`.
    pub dr_mode: Option<DrMode>,
    /// `usb3-lpm-capable`.
    pub lpm: bool,
    /// `snps,axi-pipe-limit`.
    pub axi_pipe_limit: Option<u8>,
    /// `snps,tx-max-burst`.
    pub tx_max_burst: Option<u8>,
    /// `snps,tx-thr-num-pkt`.
    pub tx_thr_num_pkt: Option<u8>,
    /// Quirk flags.
    pub quirks: Quirks,
    /// False when the node's `status` disables it.
    pub enabled: bool,
}

/// Why the USB nodes could not be parsed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UsbError {
    /// No `snps,dwc3` node was found.
    MissingNode,
    /// A device-tree value is malformed.
    Fdt(FdtError),
    /// A node has no usable `reg` window.
    BadReg,
    /// A property has the wrong cell count.
    BadCells(&'static str),
    /// More than [`MAX_USB_CONTROLLERS`] nodes.
    TooManyControllers,
    /// A numeric property does not fit one byte.
    BadValue(&'static str),
}

impl fmt::Display for UsbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingNode => f.write_str("no snps,dwc3 node"),
            Self::Fdt(error) => write!(f, "device tree: {error}"),
            Self::BadReg => f.write_str("USB node has no usable reg window"),
            Self::BadCells(what) => write!(f, "malformed {what} cells"),
            Self::TooManyControllers => {
                write!(f, "more than {MAX_USB_CONTROLLERS} USB controllers")
            }
            Self::BadValue(what) => write!(f, "{what} does not fit one byte"),
        }
    }
}

impl core::error::Error for UsbError {}

impl From<FdtError> for UsbError {
    fn from(error: FdtError) -> Self {
        Self::Fdt(error)
    }
}

/// The controllers found in one tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsbControllers {
    controllers: [Option<UsbController>; MAX_USB_CONTROLLERS],
    count: u8,
}

impl UsbControllers {
    /// Find every `snps,dwc3` node.
    ///
    /// # Errors
    ///
    /// Returns [`UsbError`] for a missing root/node, a malformed `reg`/`interrupts` property or
    /// more than [`MAX_USB_CONTROLLERS`] nodes.
    pub fn from_fdt(fdt: &Fdt<'_>) -> Result<Self, UsbError> {
        let root = fdt.root().ok_or(UsbError::MissingNode)?;
        let mut controllers = [None; MAX_USB_CONTROLLERS];
        let mut count = 0u8;
        collect(&root, &mut controllers, &mut count)?;
        if count == 0 {
            return Err(UsbError::MissingNode);
        }
        Ok(Self { controllers, count })
    }

    /// Number of controllers.
    #[must_use]
    pub const fn count(&self) -> u8 {
        self.count
    }

    /// One controller by index.
    #[must_use]
    pub fn get(&self, index: u8) -> Option<&UsbController> {
        self.controllers
            .get(usize::from(index))
            .and_then(Option::as_ref)
    }

    /// Iterate the controllers.
    pub fn iter(&self) -> impl Iterator<Item = &UsbController> {
        self.controllers.iter().flatten()
    }

    /// Number of controllers whose `status` is not disabled.
    #[must_use]
    pub fn enabled_count(&self) -> u8 {
        self.iter().filter(|controller| controller.enabled).count() as u8
    }
}

fn collect(
    node: &Node<'_>,
    out: &mut [Option<UsbController>; MAX_USB_CONTROLLERS],
    count: &mut u8,
) -> Result<(), UsbError> {
    if node.compatible_with("snps,dwc3") {
        if usize::from(*count) >= MAX_USB_CONTROLLERS {
            return Err(UsbError::TooManyControllers);
        }
        let controller = parse_controller(node, *count)?;
        if let Some(slot) = out.get_mut(usize::from(*count)) {
            *slot = Some(controller);
        }
        *count = count.saturating_add(1);
    }
    for child in node.children() {
        collect(&child, out, count)?;
    }
    Ok(())
}

fn parse_controller(node: &Node<'_>, index: u8) -> Result<UsbController, UsbError> {
    let parent = node.parent().ok_or(UsbError::BadReg)?;
    let mut reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(UsbError::Fdt)?;
    let (base, size) = reg.next().ok_or(UsbError::BadReg)?;
    let window = Region::new(base, size).ok_or(UsbError::BadReg)?;
    Ok(UsbController {
        index,
        window,
        interrupt: parse_interrupt(node)?,
        dr_mode: node.property_str("dr_mode").map(DrMode::parse),
        lpm: node.property("usb3-lpm-capable").is_some(),
        axi_pipe_limit: small_u32(node, "snps,axi-pipe-limit")?,
        tx_max_burst: small_u32(node, "snps,tx-max-burst")?,
        tx_thr_num_pkt: small_u32(node, "snps,tx-thr-num-pkt")?,
        quirks: Quirks {
            dis_rxdet_inp3: node.property("snps,dis_rxdet_inp3_quirk").is_some(),
            enhanced_nak_fs: node.property("snps,enhanced-nak-fs-quirk").is_some(),
            parkmode_disable_ss: node.property("snps,parkmode-disable-ss-quirk").is_some(),
            parkmode_disable_fsls: node.property("snps,parkmode-disable-fsls-quirk").is_some(),
        },
        enabled: !matches!(
            node.property_str("status"),
            Some("disabled" | "reserved" | "fail" | "fail-sss")
        ),
    })
}

fn parse_interrupt(node: &Node<'_>) -> Result<Option<InterruptRef>, UsbError> {
    let Some(value) = node.property("interrupts") else {
        return Ok(None);
    };
    if !value.len().is_multiple_of(8) {
        return Err(UsbError::BadCells("interrupts"));
    }
    let mut cells = node.cells("interrupts", 4);
    let (Some(id), Some(flags)) = (cells.next(), cells.next()) else {
        return Ok(None);
    };
    Ok(Some(InterruptRef {
        id: u32::try_from(id).map_err(|_| UsbError::BadCells("interrupt id"))?,
        flags: u32::try_from(flags).map_err(|_| UsbError::BadCells("interrupt flags"))?,
    }))
}

fn small_u32(node: &Node<'_>, property: &'static str) -> Result<Option<u8>, UsbError> {
    match node.u32_property(property) {
        Some(value) => u8::try_from(value)
            .map(Some)
            .map_err(|_| UsbError::BadValue(property)),
        None => Ok(None),
    }
}

#[cfg(test)]
#[path = "device_ut.rs"]
mod tests;
