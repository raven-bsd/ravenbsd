// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RP1 multi-function device as read from the device tree.
//!
//! The RP1 node is a `simple-bus` behind the BCM2712 PCIe root complex. This module copies the
//! parts the MFD bring-up needs: the GPIO/pinctrl block, the clock provider and its assigned
//! rates, the child function windows (Ethernet, USB, SD, DMA, ADC, PIO, SRAM, mailbox) and the
//! firmware shared-memory window. The addresses are RP1-local; the caller translates them through
//! the [`raven_pci::Rp1Window`] from the R1 model.
//!
//! Everything here is data. The register encodings, the pin-mux values and the power/reset
//! sequence are UNVERIFIED; `docs/pi5-rp1-mfd.md` lists the board steps.

use core::fmt;
use core::str;

use raven_fdt_lite::{Fdt, FdtError, Node, translate_ranges};
use raven_pci::Region;

use crate::clocks::{ClockSpec, ClockTree};
use crate::gpio::{Bias, PinController, PinRule, Pinctrl};
use crate::name::{Name, NameError};

/// Largest number of GPIO banks the model holds (the RP1 tree has three).
pub const MAX_BANKS: usize = 3;
/// Largest number of child function windows the model holds.
pub const MAX_FUNCTIONS: usize = 16;
/// Largest number of GPIO bank interrupts the model holds.
pub const MAX_INTERRUPTS: usize = 3;

/// One GPIO bank interrupt.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupt {
    /// Interrupt number inside the RP1 interrupt domain.
    pub id: u32,
    /// Two-cell interrupt flags from the tree.
    pub flags: u32,
}

/// The GPIO/pinctrl block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GpioBlock {
    /// Bank register windows, `None` past `bank_count`.
    pub banks: [Option<Region>; MAX_BANKS],
    /// Number of banks.
    pub bank_count: u8,
    /// Number of pins from `gpio-ranges` (54 on the Pi 5).
    pub pins: u16,
    /// Bank interrupts, `Interrupt { id: 0, flags: 0 }` past `interrupt_count`.
    pub interrupts: [Interrupt; MAX_INTERRUPTS],
    /// Number of bank interrupts.
    pub interrupt_count: u8,
}

impl GpioBlock {
    /// The window of bank `index`, or `None`.
    #[must_use]
    pub fn bank(&self, index: u8) -> Option<Region> {
        self.banks.get(usize::from(index)).copied().flatten()
    }
}

/// The clock provider and the rates the firmware assigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockBlock {
    /// Register window.
    pub region: Region,
    /// The clocks the tree lists, with their assigned rates. Parent links are not in the tree,
    /// so every clock starts without one.
    pub tree: ClockTree,
}

/// The RP1 firmware interface windows (mailbox and shared memory), when the tree describes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FirmwareBlock {
    /// Mailbox window (RP1-local).
    pub mailbox: Region,
    /// Shared-memory window (RP1-local).
    pub shmem: Region,
}

/// A child function of the MFD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionKind {
    /// Ethernet MAC.
    Ethernet,
    /// USB controller.
    Usb,
    /// SD/MMC controller.
    Mmc,
    /// DMA engine.
    Dma,
    /// ADC.
    Adc,
    /// Programmable I/O.
    Pio,
    /// Internal SRAM.
    Sram,
    /// Firmware mailbox.
    Mailbox,
    /// GPIO block.
    Gpio,
    /// Clock provider.
    Clocks,
    /// UART.
    Serial,
    /// A child the model does not classify.
    Other,
}

impl FunctionKind {
    fn of(node_name: &str) -> Self {
        let base = node_name
            .split_once('@')
            .map_or(node_name, |(base, _)| base);
        match base {
            "ethernet" => Self::Ethernet,
            "usb" => Self::Usb,
            "mmc" => Self::Mmc,
            "dma" => Self::Dma,
            "adc" => Self::Adc,
            "pio" => Self::Pio,
            "sram" => Self::Sram,
            "mailbox" => Self::Mailbox,
            "gpio" => Self::Gpio,
            "clocks" => Self::Clocks,
            "serial" => Self::Serial,
            _ => Self::Other,
        }
    }
}

/// One child function window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FunctionWindow {
    /// Classified function.
    pub kind: FunctionKind,
    /// First `reg` window (RP1-local).
    pub region: Region,
    /// False when the node's `status` disables it.
    pub enabled: bool,
}

/// Why the MFD description cannot be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MfdError {
    /// No RP1 anchor node (`gpio`, `clocks` or `ethernet`) was found.
    MissingRp1,
    /// A device-tree value is malformed.
    Fdt(FdtError),
    /// A bus declares address/size cell counts this model does not support.
    Cells,
    /// A `reg` window is empty or overflows.
    BadReg(&'static str),
    /// The GPIO node has more banks than the model holds.
    TooManyBanks,
    /// The GPIO node has more bank interrupts than the model holds.
    TooManyInterrupts,
    /// `gpio-ranges` is absent or not four cells.
    BadPins,
    /// `gpio-ranges` names a different phandle than the GPIO node.
    RangeOwner {
        /// The GPIO node's phandle.
        expected: u32,
        /// The phandle in `gpio-ranges`.
        found: u32,
    },
    /// The GPIO pin count is outside the model's range.
    PinCount(u32),
    /// `gpio-line-names` does not have one entry per pin.
    LineNames {
        /// Pins declared by `gpio-ranges`.
        pins: usize,
        /// Names present.
        names: usize,
    },
    /// A pinctrl group has no function, no pins or a malformed pin name.
    BadPinRule(&'static str),
    /// A `pins` entry names a pin outside the declared range.
    BadPin(u32),
    /// The pin-rule table is full.
    TooManyRules,
    /// A name is empty or too long.
    Name(NameError),
    /// The clock node has no phandle or an unexpected `#clock-cells`.
    BadClockNode(&'static str),
    /// `assigned-clocks` and `assigned-clock-rates` disagree.
    ClockCount {
        /// Clock IDs listed.
        clocks: usize,
        /// Rates listed.
        rates: usize,
    },
    /// `assigned-clocks` names a phandle other than the clock provider.
    ForeignClock(u32),
    /// A clock ID does not fit the model.
    BadClockId(u32),
    /// A rate does not fit 32 bits.
    BadClockRate(u64),
    /// The same clock ID appears twice.
    DuplicateClock(u8),
    /// The function table is full.
    TooManyFunctions,
    /// The firmware node exists but its mailbox or shared memory is missing.
    IncompleteFirmware,
}

impl fmt::Display for MfdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingRp1 => f.write_str("no RP1 node"),
            Self::Fdt(error) => write!(f, "device tree: {error}"),
            Self::Cells => f.write_str("unsupported address/size cell counts"),
            Self::BadReg(what) => write!(f, "malformed {what} reg"),
            Self::TooManyBanks => f.write_str("too many GPIO banks"),
            Self::TooManyInterrupts => f.write_str("too many GPIO interrupts"),
            Self::BadPins => f.write_str("gpio-ranges is absent or not four cells"),
            Self::RangeOwner { expected, found } => write!(
                f,
                "gpio-ranges phandle {found} differs from the GPIO node {expected}"
            ),
            Self::PinCount(pins) => write!(f, "pin count {pins} is not supported"),
            Self::LineNames { pins, names } => {
                write!(f, "gpio-line-names has {names} entries for {pins} pins")
            }
            Self::BadPinRule(what) => write!(f, "bad pinctrl rule: {what}"),
            Self::BadPin(pin) => write!(f, "pin {pin} is outside the declared range"),
            Self::TooManyRules => f.write_str("too many pinctrl rules"),
            Self::Name(error) => write!(f, "name: {error}"),
            Self::BadClockNode(what) => write!(f, "bad clock node: {what}"),
            Self::ClockCount { clocks, rates } => {
                write!(f, "assigned {clocks} clocks but {rates} rates")
            }
            Self::ForeignClock(phandle) => {
                write!(f, "assigned-clocks names foreign phandle {phandle}")
            }
            Self::BadClockId(id) => write!(f, "clock id {id} is out of range"),
            Self::BadClockRate(rate) => write!(f, "clock rate {rate} does not fit 32 bits"),
            Self::DuplicateClock(id) => write!(f, "clock {id} appears twice"),
            Self::TooManyFunctions => f.write_str("too many RP1 function windows"),
            Self::IncompleteFirmware => f.write_str("RP1 firmware node lacks a mailbox or shmem"),
        }
    }
}

impl core::error::Error for MfdError {}

impl From<FdtError> for MfdError {
    fn from(error: FdtError) -> Self {
        Self::Fdt(error)
    }
}

impl From<NameError> for MfdError {
    fn from(error: NameError) -> Self {
        Self::Name(error)
    }
}

/// The RP1 multi-function device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mfd {
    /// GPIO/pinctrl block, when present.
    pub gpio: Option<GpioBlock>,
    /// Clock provider, when present.
    pub clocks: Option<ClockBlock>,
    /// Firmware mailbox/shared memory, when the tree describes it.
    pub firmware: Option<FirmwareBlock>,
    functions: [Option<FunctionWindow>; MAX_FUNCTIONS],
    function_count: usize,
    pinctrl: Pinctrl,
}

impl Mfd {
    /// Build the model from the live device tree.
    ///
    /// The anchor is the first RP1 child the parser recognises; the MFD node is its parent.
    ///
    /// # Errors
    ///
    /// Returns [`MfdError`] for a missing RP1 node, malformed `reg`/pins/clocks, an unsupported
    /// cell count or an inconsistent firmware node.
    pub fn from_fdt(fdt: &Fdt<'_>) -> Result<Self, MfdError> {
        let anchor = fdt
            .find_compatible(&["raspberrypi,rp1-gpio"])
            .or_else(|| fdt.find_compatible(&["raspberrypi,rp1-clocks"]))
            .or_else(|| fdt.find_compatible(&["raspberrypi,rp1-gem"]))
            .ok_or(MfdError::MissingRp1)?;
        let rp1 = anchor.parent().ok_or(MfdError::MissingRp1)?;
        if rp1.address_cells() > 2 || rp1.size_cells() > 2 {
            return Err(MfdError::Cells);
        }
        let mut mfd = Self {
            gpio: None,
            clocks: None,
            firmware: None,
            functions: [None; MAX_FUNCTIONS],
            function_count: 0,
            pinctrl: Pinctrl::new(),
        };
        for child in rp1.children() {
            if child.compatible_with("raspberrypi,rp1-gpio") {
                let block = parse_gpio(&child, &mut mfd.pinctrl)?;
                mfd.gpio = Some(block);
            } else if child.compatible_with("raspberrypi,rp1-clocks") {
                mfd.clocks = Some(parse_clocks(&child)?);
            } else {
                mfd.push_function(&child)?;
            }
        }
        mfd.firmware = parse_firmware(fdt, &mfd.functions)?;
        Ok(mfd)
    }

    /// The function windows, in tree order.
    pub fn functions(&self) -> impl Iterator<Item = &FunctionWindow> {
        self.functions.iter().flatten()
    }

    /// Number of function windows.
    #[must_use]
    pub const fn function_count(&self) -> usize {
        self.function_count
    }

    /// The first function of `kind`.
    #[must_use]
    pub fn function(&self, kind: FunctionKind) -> Option<&FunctionWindow> {
        self.functions().find(|window| window.kind == kind)
    }

    /// The pinctrl rules copied from the GPIO node.
    #[must_use]
    pub const fn pinctrl(&self) -> Pinctrl {
        self.pinctrl
    }

    /// Build the GPIO controller from the parsed pin count and rules.
    #[must_use]
    pub fn pin_controller(&self) -> Option<PinController> {
        let gpio = self.gpio.as_ref()?;
        let pins = u8::try_from(gpio.pins).ok()?;
        PinController::new(pins, self.pinctrl).ok()
    }

    fn push_function(&mut self, node: &Node<'_>) -> Result<(), MfdError> {
        let parent = node.parent().ok_or(MfdError::BadReg("function"))?;
        let mut reg = node
            .reg_checked(parent.address_cells(), parent.size_cells())
            .map_err(MfdError::Fdt)?;
        let Some((base, size)) = reg.next() else {
            // A child without a `reg` is not a function window (for example a pinctrl group).
            return Ok(());
        };
        let region = Region::new(base, size).ok_or(MfdError::BadReg("function"))?;
        let window = FunctionWindow {
            kind: FunctionKind::of(node.name()),
            region,
            enabled: enabled(node),
        };
        match self.functions.get_mut(self.function_count) {
            Some(slot) => {
                *slot = Some(window);
                self.function_count = self.function_count.saturating_add(1);
                Ok(())
            }
            None => Err(MfdError::TooManyFunctions),
        }
    }
}

fn enabled(node: &Node<'_>) -> bool {
    !matches!(
        node.property_str("status"),
        Some("disabled" | "reserved" | "fail" | "fail-sss")
    )
}

fn parse_gpio(node: &Node<'_>, pinctrl: &mut Pinctrl) -> Result<GpioBlock, MfdError> {
    let parent = node.parent().ok_or(MfdError::BadReg("GPIO"))?;
    let reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(MfdError::Fdt)?;
    let mut banks = [None; MAX_BANKS];
    let mut bank_count = 0u8;
    for (base, size) in reg {
        let region = Region::new(base, size).ok_or(MfdError::BadReg("GPIO bank"))?;
        let Some(slot) = banks.get_mut(usize::from(bank_count)) else {
            return Err(MfdError::TooManyBanks);
        };
        *slot = Some(region);
        bank_count = bank_count.saturating_add(1);
    }
    if bank_count == 0 {
        return Err(MfdError::BadReg("GPIO has no bank"));
    }

    let pins = parse_pins(node)?;
    check_line_names(node, pins)?;
    parse_pin_rules(node, pins, pinctrl)?;

    let mut interrupts = [Interrupt { id: 0, flags: 0 }; MAX_INTERRUPTS];
    let mut interrupt_count = 0u8;
    if let Some(value) = node.property("interrupts") {
        if !value.len().is_multiple_of(8) {
            return Err(MfdError::BadPinRule("malformed GPIO interrupts"));
        }
        let mut cells = node.cells("interrupts", 4);
        while let (Some(id), Some(flags)) = (cells.next(), cells.next()) {
            let Some(slot) = interrupts.get_mut(usize::from(interrupt_count)) else {
                return Err(MfdError::TooManyInterrupts);
            };
            *slot = Interrupt {
                id: u32::try_from(id).map_err(|_| MfdError::BadPinRule("interrupt id"))?,
                flags: u32::try_from(flags).map_err(|_| MfdError::BadPinRule("interrupt flags"))?,
            };
            interrupt_count = interrupt_count.saturating_add(1);
        }
    }

    Ok(GpioBlock {
        banks,
        bank_count,
        pins,
        interrupts,
        interrupt_count,
    })
}

fn parse_pins(node: &Node<'_>) -> Result<u16, MfdError> {
    let mut cells = node.cells("gpio-ranges", 4);
    let Some(owner) = cells.next() else {
        return Err(MfdError::BadPins);
    };
    let Some(_pin_base) = cells.next() else {
        return Err(MfdError::BadPins);
    };
    let Some(_gpio_base) = cells.next() else {
        return Err(MfdError::BadPins);
    };
    let Some(count) = cells.next() else {
        return Err(MfdError::BadPins);
    };
    if cells.next().is_some() {
        return Err(MfdError::BadPins);
    }
    if let Some(phandle) = node.u32_property("phandle")
        && owner != u64::from(phandle)
    {
        return Err(MfdError::RangeOwner {
            expected: phandle,
            found: owner as u32,
        });
    }
    let pins = u16::try_from(count).map_err(|_| MfdError::PinCount(u32::MAX))?;
    if pins == 0 || pins > crate::gpio::MAX_PINS as u16 {
        return Err(MfdError::PinCount(u32::from(pins)));
    }
    Ok(pins)
}

fn check_line_names(node: &Node<'_>, pins: u16) -> Result<(), MfdError> {
    let Some(value) = node.property("gpio-line-names") else {
        return Ok(());
    };
    let names = value
        .split(|byte| *byte == 0)
        .filter(|entry| !entry.is_empty())
        .count();
    if names != usize::from(pins) {
        return Err(MfdError::LineNames {
            pins: usize::from(pins),
            names,
        });
    }
    Ok(())
}

fn parse_pin_rules(node: &Node<'_>, pins: u16, pinctrl: &mut Pinctrl) -> Result<(), MfdError> {
    for group in node.children() {
        for config in group.children() {
            let function = Name::new(
                config
                    .property_str("function")
                    .ok_or(MfdError::BadPinRule("no function"))?,
            )?;
            let bias = bias_of(&config);
            let value = config
                .property("pins")
                .ok_or(MfdError::BadPinRule("no pins"))?;
            for entry in value
                .split(|byte| *byte == 0)
                .filter(|entry| !entry.is_empty())
            {
                let text = str::from_utf8(entry)
                    .map_err(|_| MfdError::BadPinRule("pin name is not UTF-8"))?;
                let pin = parse_pin(text, pins)?;
                if !pinctrl.push(PinRule {
                    pin,
                    function,
                    bias,
                }) {
                    return Err(MfdError::TooManyRules);
                }
            }
        }
    }
    Ok(())
}

fn parse_pin(text: &str, pins: u16) -> Result<u8, MfdError> {
    let Some(number) = text.strip_prefix("gpio") else {
        return Err(MfdError::BadPinRule("pin name does not start with gpio"));
    };
    let number: u32 = number
        .parse()
        .map_err(|_| MfdError::BadPinRule("pin number is not decimal"))?;
    let pin = u8::try_from(number).map_err(|_| MfdError::BadPin(number))?;
    if u16::from(pin) >= pins {
        return Err(MfdError::BadPin(number));
    }
    Ok(pin)
}

fn bias_of(node: &Node<'_>) -> Bias {
    if node.property("bias-disable").is_some() {
        return Bias::Disable;
    }
    if node.property("bias-pull-up").is_some() {
        return Bias::PullUp;
    }
    if node.property("bias-pull-down").is_some() {
        return Bias::PullDown;
    }
    if node.property("bias-bus-hold").is_some() {
        return Bias::BusHold;
    }
    Bias::Unspecified
}

fn parse_clocks(node: &Node<'_>) -> Result<ClockBlock, MfdError> {
    let parent = node.parent().ok_or(MfdError::BadReg("clock"))?;
    let mut reg = node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(MfdError::Fdt)?;
    let (base, size) = reg.next().ok_or(MfdError::BadReg("clock"))?;
    let region = Region::new(base, size).ok_or(MfdError::BadReg("clock"))?;

    let clocks = match node.property("assigned-clocks") {
        Some(value) => {
            if !value.len().is_multiple_of(8) {
                return Err(MfdError::BadClockNode("assigned-clocks is not whole pairs"));
            }
            value.len() / 8
        }
        None => 0,
    };
    let rates = match node.property("assigned-clock-rates") {
        Some(value) => value.len() / 4,
        None => 0,
    };
    if clocks == 0 && rates == 0 {
        // A minimal tree may describe the provider without assigned rates.
        return Ok(ClockBlock {
            region,
            tree: ClockTree::new(),
        });
    }
    if clocks != rates {
        return Err(MfdError::ClockCount { clocks, rates });
    }
    if node.u32_property("#clock-cells") != Some(1) {
        return Err(MfdError::BadClockNode("#clock-cells is not 1"));
    }
    let phandle = node
        .u32_property("phandle")
        .ok_or(MfdError::BadClockNode("no phandle"))?;

    let mut tree = ClockTree::new();
    let mut pairs = node.cells("assigned-clocks", 4);
    let mut values = node.cells("assigned-clock-rates", 4);
    while let (Some(owner), Some(id)) = (pairs.next(), pairs.next()) {
        let rate = values
            .next()
            .ok_or(MfdError::ClockCount { clocks, rates })?;
        if owner != u64::from(phandle) {
            return Err(MfdError::ForeignClock(owner as u32));
        }
        let id = u8::try_from(id).map_err(|_| MfdError::BadClockId(id as u32))?;
        let rate = u32::try_from(rate).map_err(|_| MfdError::BadClockRate(rate))?;
        tree.insert(ClockSpec::new(id, rate, u32::MAX))
            .map_err(|_| MfdError::DuplicateClock(id))?;
    }
    Ok(ClockBlock { region, tree })
}

fn parse_firmware(
    fdt: &Fdt<'_>,
    functions: &[Option<FunctionWindow>; MAX_FUNCTIONS],
) -> Result<Option<FirmwareBlock>, MfdError> {
    if fdt.find_compatible(&["raspberrypi,rp1-firmware"]).is_none() {
        return Ok(None);
    }
    let mailbox = functions
        .iter()
        .flatten()
        .find(|window| window.kind == FunctionKind::Mailbox)
        .map(|window| window.region)
        .ok_or(MfdError::IncompleteFirmware)?;
    let shmem_node = fdt
        .find_compatible(&["raspberrypi,rp1-shmem"])
        .ok_or(MfdError::IncompleteFirmware)?;
    let parent = shmem_node.parent().ok_or(MfdError::IncompleteFirmware)?;
    let mut reg = shmem_node
        .reg_checked(parent.address_cells(), parent.size_cells())
        .map_err(MfdError::Fdt)?;
    let (local, size) = reg.next().ok_or(MfdError::IncompleteFirmware)?;
    let address = translate_ranges(&parent, local).ok_or(MfdError::IncompleteFirmware)?;
    let shmem = Region::new(address, size).ok_or(MfdError::IncompleteFirmware)?;
    Ok(Some(FirmwareBlock { mailbox, shmem }))
}

#[cfg(test)]
#[path = "mfd_ut.rs"]
mod tests;
