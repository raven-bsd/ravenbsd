// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! EMMC2 driver core and board seam.
//!
//! The driver owns the register sequence; [`raven_pi4`] owns the register map, command encoding,
//! card state machine, timing rules, DMA descriptors and transfer state. The board boundary is the
//! [`Host`] trait: `main.rs` implements it with the `raven_driver::MmioRegion` grant, a clock from
//! the launcher and the S21d DMA window; the host tests implement it with a register fake.
//!
//! [`probe`] reads the controller version and capabilities, resolves the base clock and produces
//! the identification/default/high-speed plans. [`bring_up`] performs reset, power and the
//! identification clock. [`init_card`] drives [`raven_pi4::init::Init`] to a selected 4-bit card
//! and then programs the 25 MHz default clock and the four-bit host control.
//!
//! The [`block`] module maps `raven-proto-block` requests to CMD17/18/24/25 and serves them through
//! a [`block::BlockDevice`] backend; [`block::MemoryDisk`] is the host-testable backend. The
//! `main.rs` program adds the SDMA/IRQ `disk:` path for the board and a host-backed `test` mode for
//! the qtest. The real-card result remains UNVERIFIED until a board log exists.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod block;
pub mod options;
pub mod pattern;

use raven_pi4::command::{Command, CommandFailure};
use raven_pi4::fdt::DmaWindow;
use raven_pi4::init::{Card, Init, InitError, InitStep};
use raven_pi4::registers::{
    CLOCK_INTERNAL_ENABLE, CLOCK_INTERNAL_STABLE, CLOCK_SD_ENABLE, Capabilities,
    DEFAULT_BASE_CLOCK_HZ, ERROR_CMD_CRC, ERROR_CMD_END_BIT, ERROR_CMD_INDEX, ERROR_CMD_TIMEOUT,
    ERROR_DATA_CRC, ERROR_DATA_TIMEOUT, NormalInterrupt, POWER_BUS, POWER_VOLTAGE_3V3,
    PresentState, Quirks, Register, SOFTWARE_RESET_ALL,
};
use raven_pi4::timing::{BusWidth, ClockPlan, DmaMode, SpecVersion, Timing, TimingError};

pub use block::{BlockDevice, BlockError, BlockPlan, BlockService, MemoryDisk};

/// Time a single command may take before the driver reports a timeout.
pub const COMMAND_TIMEOUT_NS: u64 = 1_000_000_000;

/// Time a host-controller reset may take.
pub const RESET_TIMEOUT_NS: u64 = 100_000_000;

/// Time the internal clock may take to become stable.
pub const CLOCK_TIMEOUT_NS: u64 = 100_000_000;

/// Timeout-control value: the largest timeout the controller implements.
pub const TIMEOUT_CONTROL_MAX: u8 = 0x0e;

/// Capabilities the model expects from EMMC2 when the register is not trustworthy.
///
/// 3.3 V, high speed, DMA and a 1024-byte maximum block, matching the B42 extraction of the
/// NetBSD `sc_caps` value. The real register contents are UNVERIFIED until a board log exists.
pub const EMMC2_MODEL_CAPABILITIES: u32 = (1 << 24) | (1 << 22) | (1 << 21) | (1 << 16);

/// Where the base clock came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSource {
    /// The `platd` mailbox clock query (`VCPROP_CLK_EMMC2`, id 12).
    Mailbox,
    /// The capabilities register, used only when its contents are declared valid.
    Capabilities,
    /// The documented 50 MHz fallback.
    ModelDefault,
}

/// The controller facts measured before the card is touched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostPlan {
    /// Raw host-controller version register.
    pub version_raw: u16,
    /// Specification generation used for the divider encoding.
    pub spec: SpecVersion,
    /// Raw capabilities register, kept for the board log.
    pub raw_capabilities: u32,
    /// Capabilities the driver will use (the model value when the register is not trustworthy).
    pub capabilities: Capabilities,
    /// Where the base clock came from.
    pub clock_source: ClockSource,
    /// The resolved base clock in Hz.
    pub base_hz: u32,
    /// Timing at the identification clock and one-bit bus.
    pub timing: Timing,
    /// DMA window from the device tree and the kernel driver objects.
    pub dma: DmaWindow,
}

/// A card that finished initialisation, with the host timing that is now programmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardReady {
    /// The selected card.
    pub card: Card,
    /// The controller facts measured at probe time.
    pub plan: HostPlan,
    /// The timing after the default and four-bit transitions.
    pub timing: Timing,
}

/// Why the driver could not continue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverError {
    /// The capabilities register does not advertise 3.3 V.
    UnsupportedVoltage {
        /// The capabilities word.
        capabilities: u32,
    },
    /// A speed or bus-width transition is invalid.
    Timing(TimingError),
    /// The controller did not clear the software-reset bit in time.
    ResetTimeout,
    /// The internal clock did not become stable in time.
    ClockStableTimeout,
    /// The controller reports no card in the slot.
    NoCard,
    /// A command failed in the controller or the card state machine.
    Init(InitError),
}

impl From<TimingError> for DriverError {
    fn from(error: TimingError) -> Self {
        Self::Timing(error)
    }
}

impl From<InitError> for DriverError {
    fn from(error: InitError) -> Self {
        Self::Init(error)
    }
}

/// The board seam: register access, time and the two platform facts the driver needs.
///
/// A real implementation wraps the MMIO grant, the monotonic clock, the `platd` mailbox clock rate
/// and the S21d DMA window. It must never be called from an interrupt context.
pub trait Host {
    /// Read one SDHCI register. The value is right-aligned in the register width.
    fn read(&mut self, register: Register) -> u32;
    /// Write one SDHCI register. Bits outside the register width are ignored by the adapter.
    fn write(&mut self, register: Register, value: u32);
    /// Monotonic time in nanoseconds for the bounded polls.
    fn now_ns(&mut self) -> u64;
    /// The EMMC2 base clock from `platd`, or `None` when the mailbox has no answer.
    fn clock_hz(&mut self) -> Option<u32>;
    /// The EMMC2 DMA window from the device tree.
    fn dma_window(&self) -> DmaWindow;
}

/// Probe with the Pi 4 EMMC2 quirks.
pub fn probe<H: Host>(host: &mut H) -> Result<HostPlan, DriverError> {
    probe_with_quirks(host, Quirks::PI4_EMMC2)
}

/// Probe with an explicit quirk set.
///
/// When `quirks.host_capabilities_valid` is false the capabilities register is recorded but not
/// trusted, matching the NetBSD `SDHC_FLAG_HOSTCAPS` behaviour of the EMMC2 controller.
pub fn probe_with_quirks<H: Host>(host: &mut H, quirks: Quirks) -> Result<HostPlan, DriverError> {
    let version_raw = host.read(Register::HostControllerVersion) as u16;
    let spec = SpecVersion::from_register(version_raw as u8);
    let raw_capabilities = host.read(Register::Capabilities);
    let capabilities = if quirks.host_capabilities_valid {
        let capabilities = Capabilities::from_raw(raw_capabilities);
        if !capabilities.voltage_3v3() {
            return Err(DriverError::UnsupportedVoltage {
                capabilities: raw_capabilities,
            });
        }
        capabilities
    } else {
        Capabilities::from_raw(EMMC2_MODEL_CAPABILITIES)
    };

    let (base_hz, clock_source) = match host.clock_hz() {
        Some(hz) if hz > 0 => (hz, ClockSource::Mailbox),
        _ if quirks.host_capabilities_valid && capabilities.base_clock_hz() > 0 => {
            (capabilities.base_clock_hz(), ClockSource::Capabilities)
        }
        _ => (DEFAULT_BASE_CLOCK_HZ, ClockSource::ModelDefault),
    };

    let timing = Timing::new(base_hz, spec)?;
    Ok(HostPlan {
        version_raw,
        spec,
        raw_capabilities,
        capabilities,
        clock_source,
        base_hz,
        timing,
        dma: host.dma_window(),
    })
}

/// Reset the controller, apply 3.3 V power and program the identification clock.
pub fn bring_up<H: Host>(host: &mut H) -> Result<(HostPlan, Timing), DriverError> {
    let plan = probe(host)?;
    reset(host)?;
    host.write(Register::TimeoutControl, u32::from(TIMEOUT_CONTROL_MAX));
    host.write(
        Register::PowerControl,
        u32::from(POWER_BUS | POWER_VOLTAGE_3V3),
    );
    let clock = plan
        .timing
        .clock_plan()
        .map_err(|error| DriverError::Timing(TimingError::Clock(error)))?;
    program_clock(host, clock)?;
    Ok((plan, plan.timing))
}

/// Initialise the card and leave the host at 25 MHz on four data lines.
///
/// The real MMIO/DMA adapter, the interrupt path and the `block:` scheme are the integration step;
/// this function is fully exercised with a host-side register fake.
pub fn init_card<H: Host>(host: &mut H) -> Result<CardReady, DriverError> {
    let (plan, mut timing) = bring_up(host)?;
    let mut state = Init::new();
    let mut step = state.start();
    loop {
        match step {
            InitStep::Send(command) => {
                let outcome = issue(host, command);
                step = state.advance(outcome)?;
            }
            InitStep::Ready(card) => {
                let default = timing.switch_to_default()?;
                program_clock(host, default)?;
                timing.set_bus_width(BusWidth::Bit4)?;
                host.write(
                    Register::HostControl,
                    u32::from(timing.host_control(Quirks::PI4_EMMC2, DmaMode::Pio)),
                );
                return Ok(CardReady { card, plan, timing });
            }
        }
    }
}

/// True when the controller's present state reports a card in the slot.
///
/// The EMMC2 `Card Inserted` bit is UNVERIFIED on silicon. A launcher that knows the slot is
/// non-removable may initialise unconditionally with [`init_card`]; a removable-slot integration
/// should confirm the bit on the board first.
pub fn card_inserted<H: Host>(host: &mut H) -> bool {
    PresentState::from_raw(host.read(Register::PresentState)).card_inserted()
}

/// Initialise a card only when the controller reports one present.
///
/// This is [`init_card`] with an explicit empty-slot gate for a removable-card launcher. It does
/// not power-cycle the card; call it again after the slot reports removal and reinsertion.
pub fn init_card_checked<H: Host>(host: &mut H) -> Result<CardReady, DriverError> {
    if !card_inserted(host) {
        return Err(DriverError::NoCard);
    }
    init_card(host)
}

fn reset<H: Host>(host: &mut H) -> Result<(), DriverError> {
    host.write(Register::SoftwareReset, u32::from(SOFTWARE_RESET_ALL));
    let deadline = host.now_ns().saturating_add(RESET_TIMEOUT_NS);
    let mut attempts = 0u32;
    loop {
        if host.read(Register::SoftwareReset) & u32::from(SOFTWARE_RESET_ALL) == 0 {
            return Ok(());
        }
        attempts = attempts.saturating_add(1);
        if attempts > 100_000 || host.now_ns() >= deadline {
            return Err(DriverError::ResetTimeout);
        }
    }
}

fn program_clock<H: Host>(host: &mut H, plan: ClockPlan) -> Result<(), DriverError> {
    let control = CLOCK_INTERNAL_ENABLE | plan.control;
    host.write(Register::ClockControl, u32::from(control));
    let deadline = host.now_ns().saturating_add(CLOCK_TIMEOUT_NS);
    let mut attempts = 0u32;
    loop {
        if host.read(Register::ClockControl) & u32::from(CLOCK_INTERNAL_STABLE) != 0 {
            break;
        }
        attempts = attempts.saturating_add(1);
        if attempts > 100_000 || host.now_ns() >= deadline {
            return Err(DriverError::ClockStableTimeout);
        }
    }
    host.write(Register::ClockControl, u32::from(control | CLOCK_SD_ENABLE));
    Ok(())
}

fn issue<H: Host>(host: &mut H, command: Command) -> Result<[u32; 4], CommandFailure> {
    wait_command_line(host)?;
    let encoded = command.encode(None).map_err(|_| CommandFailure::Other)?;
    host.write(Register::Argument, encoded.argument);
    host.write(Register::BlockSize, u32::from(encoded.block_size));
    host.write(Register::BlockCount, u32::from(encoded.block_count));
    host.write(Register::TransferMode, u32::from(encoded.transfer_mode));
    host.write(Register::Command, u32::from(encoded.command));

    let deadline = host.now_ns().saturating_add(COMMAND_TIMEOUT_NS);
    loop {
        let normal = NormalInterrupt::from_raw(host.read(Register::NormalInterruptStatus) as u16);
        let errors = host.read(Register::ErrorInterruptStatus) as u16;
        if errors != 0 {
            host.write(Register::ErrorInterruptStatus, u32::from(errors));
            host.write(Register::NormalInterruptStatus, u32::from(normal.raw()));
            return Err(map_error(errors));
        }
        if normal.command_complete() {
            let response = [
                host.read(Register::Response0),
                host.read(Register::Response1),
                host.read(Register::Response2),
                host.read(Register::Response3),
            ];
            host.write(Register::NormalInterruptStatus, u32::from(normal.raw()));
            return Ok(response);
        }
        if normal.error_interrupt() {
            host.write(Register::NormalInterruptStatus, u32::from(normal.raw()));
            return Err(CommandFailure::Other);
        }
        if host.now_ns() >= deadline {
            return Err(CommandFailure::Timeout);
        }
    }
}

fn wait_command_line<H: Host>(host: &mut H) -> Result<(), CommandFailure> {
    let deadline = host.now_ns().saturating_add(COMMAND_TIMEOUT_NS);
    let mut attempts = 0u32;
    loop {
        let state = PresentState::from_raw(host.read(Register::PresentState));
        if !state.cmd_inhibit() && !state.dat_inhibit() {
            return Ok(());
        }
        attempts = attempts.saturating_add(1);
        if attempts > 100_000 || host.now_ns() >= deadline {
            return Err(CommandFailure::Other);
        }
    }
}

fn map_error(error: u16) -> CommandFailure {
    if error & ERROR_CMD_TIMEOUT != 0 {
        CommandFailure::Timeout
    } else if error & ERROR_CMD_CRC != 0 {
        CommandFailure::Crc
    } else if error & ERROR_CMD_END_BIT != 0 {
        CommandFailure::EndBit
    } else if error & ERROR_CMD_INDEX != 0 {
        CommandFailure::Index
    } else if error & ERROR_DATA_TIMEOUT != 0 {
        CommandFailure::DataTimeout
    } else if error & ERROR_DATA_CRC != 0 {
        CommandFailure::DataCrc
    } else {
        CommandFailure::Other
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
