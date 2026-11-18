// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! First-party BCM2711 SPI0 / PL022-class master core.
//!
//! The core models the BCM2711 SPI0 register window at `0xFE20_4000`: `CS`, `FIFO`, `CLK`,
//! `DLEN`, `LTOH` and `DC`. It performs 8-bit full-duplex or write-only FIFO transfers, asserts
//! and deasserts CE0/CE1, programs the clock divisor and uses a bounded poll for every FIFO flag.
//! There is no busy loop without a timeout.
//!
//! The crate is `no_std` and has no MMIO of its own: [`RegisterBlock`] is the register seam. A
//! host [`MockRegisters`] models the FIFO and flags; the Pi 4 transport (E2) implements the trait
//! over the mapped window. This keeps every register access and error path host-testable without
//! opening SPI or claiming hardware.
//!
//! Pinmux and board integration are in `docs/design/pi4-spi.md`: SPI0 CE1 GPIO7, CE0 GPIO8,
//! MISO GPIO9, MOSI GPIO10 and SCLK GPIO11, all ALT0, 3.3 V logic.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

/// BCM2711 SPI0 physical base address.
pub const SPI0_BASE: u64 = 0xfe20_4000;
/// Bytes in the SPI0 register window modelled here.
pub const SPI0_WINDOW: usize = 0x18;
/// Nominal BCM2711 core clock used for divisor examples; the actual rate is a board input.
pub const CORE_CLOCK_HZ: u32 = 250_000_000;
/// Maximum value of the 16-bit clock divider.
pub const MAX_DIVISOR: u16 = 0xffff;
/// Default bounded poll count (each poll is one register read).
pub const DEFAULT_TIMEOUT_POLLS: u32 = 1_000_000;

/// `CS` register offset.
pub const REG_CS: usize = 0x00;
/// `FIFO` register offset.
pub const REG_FIFO: usize = 0x04;
/// `CLK` register offset.
pub const REG_CLK: usize = 0x08;
/// `DLEN` register offset.
pub const REG_DLEN: usize = 0x0c;
/// `LTOH` register offset.
pub const REG_LTOH: usize = 0x10;
/// `DC` register offset.
pub const REG_DC: usize = 0x14;

/// `CS`: chip-select field mask (CE0/CE1).
pub const CS_CS_MASK: u32 = 0x3;
/// `CS`: clock phase.
pub const CS_CPHA: u32 = 1 << 2;
/// `CS`: clock polarity.
pub const CS_CPOL: u32 = 1 << 3;
/// `CS`: clear the receive FIFO.
pub const CS_CLEAR_RX: u32 = 1 << 4;
/// `CS`: clear the transmit FIFO.
pub const CS_CLEAR_TX: u32 = 1 << 5;
/// `CS`: transfer active.
pub const CS_TA: u32 = 1 << 7;
/// `CS`: DMA enable.
pub const CS_DMAEN: u32 = 1 << 8;
/// `CS`: interrupt on done.
pub const CS_INTD: u32 = 1 << 9;
/// `CS`: interrupt on RX.
pub const CS_INTR: u32 = 1 << 10;
/// `CS`: automode chip select.
pub const CS_ADCS: u32 = 1 << 11;
/// `CS`: read enable.
pub const CS_REN: u32 = 1 << 12;
/// `CS`: LoSSI enable.
pub const CS_LEN: u32 = 1 << 13;
/// `CS`: LoSSI mono.
pub const CS_LMONO: u32 = 1 << 14;
/// `CS`: TE enable.
pub const CS_TE_EN: u32 = 1 << 15;
/// `CS`: transfer done.
pub const CS_DONE: u32 = 1 << 16;
/// `CS`: RX FIFO contains data.
pub const CS_RXD: u32 = 1 << 17;
/// `CS`: TX FIFO has room.
pub const CS_TXD: u32 = 1 << 18;
/// `CS`: RX FIFO needs service.
pub const CS_RXR: u32 = 1 << 19;
/// `CS`: TX FIFO needs service.
pub const CS_TXR: u32 = 1 << 20;

/// GPIO7: SPI0 CE1 (ALT0).
pub const PIN_CE1: u8 = 7;
/// GPIO8: SPI0 CE0 (ALT0).
pub const PIN_CE0: u8 = 8;
/// GPIO9: SPI0 MISO (ALT0).
pub const PIN_MISO: u8 = 9;
/// GPIO10: SPI0 MOSI (ALT0).
pub const PIN_MOSI: u8 = 10;
/// GPIO11: SPI0 SCLK (ALT0).
pub const PIN_SCLK: u8 = 11;
/// BCM2711 GPIO function selector value for ALT0.
pub const PINMUX_ALT0: u8 = 4;

/// The two SPI0 chip selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChipSelect {
    /// CE0, GPIO8.
    Ce0,
    /// CE1, GPIO7.
    Ce1,
}

impl ChipSelect {
    /// The `CS[1:0]` value.
    #[must_use]
    pub const fn bits(self) -> u32 {
        match self {
            Self::Ce0 => 0,
            Self::Ce1 => 1,
        }
    }
}

/// A raw 32-bit register window.
///
/// Implementations must perform exactly one volatile 32-bit access per call and must not block.
/// The Pi 4 implementation (E2) wraps the mapped `0xFE20_4000` window; the host tests use
/// [`MockRegisters`].
pub trait RegisterBlock {
    /// Read the register at `offset`.
    fn read32(&mut self, offset: usize) -> u32;

    /// Write `value` to the register at `offset`.
    fn write32(&mut self, offset: usize, value: u32);
}

/// SPI core error.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// A FIFO flag did not reach the requested state within the poll timeout.
    Timeout,
    /// Full-duplex buffers have different lengths.
    LengthMismatch,
    /// The requested clock or core clock is zero.
    InvalidClock,
    /// The computed divider does not fit the 16-bit `CLK` field.
    DivisorTooLarge,
}

/// Clock divider for `target_hz` from `core_hz`.
///
/// The divider is rounded up, clamped to the hardware minimum of 2 and limited to
/// [`MAX_DIVISOR`]. This is the BCM SPI `CLK.CDIV` model; the real core clock is an E2 input.
pub fn divisor_for(core_hz: u32, target_hz: u32) -> Result<u16, Error> {
    if core_hz == 0 || target_hz == 0 {
        return Err(Error::InvalidClock);
    }
    let rounded = core_hz
        .checked_add(target_hz.saturating_sub(1))
        .ok_or(Error::DivisorTooLarge)?
        / target_hz;
    let divisor = rounded.max(2);
    u16::try_from(divisor).map_err(|_| Error::DivisorTooLarge)
}

/// SPI0 master core over a register window.
pub struct Spi<B: RegisterBlock> {
    registers: B,
    core_hz: u32,
    timeout_polls: u32,
}

impl<B: RegisterBlock> Spi<B> {
    /// Build a core around a register window and its core clock.
    pub const fn new(registers: B, core_hz: u32) -> Self {
        Self {
            registers,
            core_hz,
            timeout_polls: DEFAULT_TIMEOUT_POLLS,
        }
    }

    /// Set the bounded poll limit used for FIFO flags.
    pub fn set_timeout_polls(&mut self, polls: u32) {
        self.timeout_polls = polls;
    }

    /// Current poll limit.
    #[must_use]
    pub const fn timeout_polls(&self) -> u32 {
        self.timeout_polls
    }

    /// Borrow the register window.
    #[must_use]
    pub const fn registers(&self) -> &B {
        &self.registers
    }

    /// Borrow the register window mutably.
    pub fn registers_mut(&mut self) -> &mut B {
        &mut self.registers
    }

    /// Program the clock divider, LTOH/DC mode and an idle `CS`.
    ///
    /// Returns the divider written to `CLK`.
    pub fn configure(&mut self, target_hz: u32) -> Result<u16, Error> {
        let divisor = divisor_for(self.core_hz, target_hz)?;
        self.registers.write32(REG_CLK, u32::from(divisor));
        self.registers.write32(REG_LTOH, 0);
        self.registers.write32(REG_DC, 0);
        self.registers.write32(REG_CS, 0);
        Ok(divisor)
    }

    /// Full-duplex 8-bit transfer, asserting `chip` for the whole transfer.
    pub fn transfer(&mut self, chip: ChipSelect, tx: &[u8], rx: &mut [u8]) -> Result<(), Error> {
        if tx.len() != rx.len() {
            return Err(Error::LengthMismatch);
        }
        if tx.is_empty() {
            return Ok(());
        }
        self.begin(chip, tx.len())?;
        let mut index = 0usize;
        while index < tx.len() {
            let byte = tx.get(index).copied().ok_or(Error::LengthMismatch)?;
            self.wait_flag(CS_TXD)?;
            self.registers.write32(REG_FIFO, u32::from(byte));
            self.wait_flag(CS_RXD)?;
            let received = self.registers.read32(REG_FIFO);
            if let Some(slot) = rx.get_mut(index) {
                *slot = received as u8;
            }
            index = index.saturating_add(1);
        }
        self.end();
        Ok(())
    }

    /// Write-only 8-bit transfer; received bytes are drained and discarded.
    pub fn write(&mut self, chip: ChipSelect, tx: &[u8]) -> Result<(), Error> {
        if tx.is_empty() {
            return Ok(());
        }
        self.begin(chip, tx.len())?;
        let mut index = 0usize;
        while index < tx.len() {
            let byte = tx.get(index).copied().ok_or(Error::LengthMismatch)?;
            self.wait_flag(CS_TXD)?;
            self.registers.write32(REG_FIFO, u32::from(byte));
            self.wait_flag(CS_RXD)?;
            let _ = self.registers.read32(REG_FIFO);
            index = index.saturating_add(1);
        }
        self.end();
        Ok(())
    }

    fn begin(&mut self, chip: ChipSelect, length: usize) -> Result<(), Error> {
        let length = u32::try_from(length).map_err(|_| Error::LengthMismatch)?;
        self.registers.write32(REG_CS, CS_CLEAR_RX | CS_CLEAR_TX);
        self.registers.write32(REG_DLEN, length);
        self.registers
            .write32(REG_CS, (chip.bits() & CS_CS_MASK) | CS_TA);
        Ok(())
    }

    fn end(&mut self) {
        self.registers.write32(REG_CS, 0);
    }

    fn wait_flag(&mut self, flag: u32) -> Result<(), Error> {
        let mut polls = 0u32;
        while polls < self.timeout_polls {
            if self.registers.read32(REG_CS) & flag != 0 {
                return Ok(());
            }
            polls = polls.saturating_add(1);
        }
        Err(Error::Timeout)
    }
}

/// A simple host mock of the SPI0 register window.
///
/// Writes are looped back into the RX FIFO, with the TXD/RXD flags derived from the queues. The
/// `writes` log preserves the exact register order for CS/divisor assertions.
#[cfg(test)]
pub(crate) struct MockRegisters {
    pub(crate) cs: u32,
    pub(crate) fifo: [u8; 16],
    pub(crate) tx_len: usize,
    pub(crate) rx_len: usize,
    pub(crate) tx_head: usize,
    pub(crate) rx_head: usize,
    pub(crate) writes: std::vec::Vec<(usize, u32)>,
    pub(crate) tx_space: bool,
    pub(crate) rx_enabled: bool,
}

#[cfg(test)]
impl MockRegisters {
    pub(crate) fn new() -> Self {
        Self {
            cs: 0,
            fifo: [0; 16],
            tx_len: 0,
            rx_len: 0,
            tx_head: 0,
            rx_head: 0,
            writes: std::vec::Vec::new(),
            tx_space: true,
            rx_enabled: true,
        }
    }
}

#[cfg(test)]
impl RegisterBlock for MockRegisters {
    fn read32(&mut self, offset: usize) -> u32 {
        match offset {
            REG_CS => {
                let mut status = self.cs & !(CS_TXD | CS_RXD);
                if self.tx_space {
                    status |= CS_TXD;
                }
                if self.rx_enabled && self.rx_len > 0 {
                    status |= CS_RXD;
                }
                status
            }
            REG_FIFO => {
                if self.rx_len == 0 {
                    return 0;
                }
                let index = self.rx_head % self.fifo.len();
                let byte = self.fifo[index];
                self.rx_head = self.rx_head.saturating_add(1);
                self.rx_len = self.rx_len.saturating_sub(1);
                u32::from(byte)
            }
            _ => 0,
        }
    }

    fn write32(&mut self, offset: usize, value: u32) {
        self.writes.push((offset, value));
        match offset {
            REG_CS => {
                if value & CS_CLEAR_RX != 0 {
                    self.rx_len = 0;
                    self.rx_head = 0;
                }
                if value & CS_CLEAR_TX != 0 {
                    self.tx_len = 0;
                    self.tx_head = 0;
                }
                self.cs = value & !(CS_CLEAR_RX | CS_CLEAR_TX);
            }
            REG_FIFO => {
                let byte = value as u8;
                if self.tx_len < self.fifo.len() {
                    let index = (self.tx_head + self.tx_len) % self.fifo.len();
                    self.fifo[index] = byte;
                    self.tx_len = self.tx_len.saturating_add(1);
                }
                if self.rx_len < self.fifo.len() {
                    let index = (self.rx_head + self.rx_len) % self.fifo.len();
                    self.fifo[index] = byte;
                    self.rx_len = self.rx_len.saturating_add(1);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
