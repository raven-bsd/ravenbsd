// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bus-width, clock and timing plans for EMMC2.
//!
//! The Pi 4 card starts at 400 kHz on one data line, moves to 25 MHz after identification, then
//! switches the card and host to 4-bit and finally to 50 MHz high speed. The modes are typed so a
//! driver cannot select 4-bit before ACMD6 or high speed before the default mode. The SDHCI
//! divider arithmetic follows the specification: the 10-bit field of a 3.00 controller and the
//! eight-bit power-of-two field of an older one produce different register values.

use crate::registers::{
    CLOCK_DIVIDER_HIGH_SHIFT, CLOCK_DIVIDER_LOW_SHIFT, HOST_CONTROL_4BIT, HOST_CONTROL_DMA_MASK,
    HOST_CONTROL_DMA_SHIFT, HOST_CONTROL_HIGH_SPEED, Quirks,
};

/// SD Host Controller specification generation that selects the divider encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpecVersion {
    /// Version 1.00.
    V1,
    /// Version 2.00.
    V2,
    /// Version 3.00 and later, with the 10-bit divider.
    V3,
}

impl SpecVersion {
    /// Interpret the low byte of the host-controller version register.
    pub const fn from_register(raw: u8) -> Self {
        match raw {
            0 => Self::V1,
            1 => Self::V2,
            _ => Self::V3,
        }
    }

    /// True when the controller uses the 10-bit frequency-select field.
    pub const fn uses_ten_bit_divider(self) -> bool {
        matches!(self, Self::V3)
    }
}

/// Why a clock divider could not be produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockError {
    /// The base clock is zero, so no divider can produce a rate.
    BaseClockZero,
    /// The requested frequency is zero.
    TargetZero,
    /// Even the largest divider produces a rate above the request.
    TooSlow,
}

/// One programmed SD clock.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockPlan {
    /// The frequency-select bits to place in the clock-control register.
    pub control: u16,
    /// The divider value (1 for "base clock", 0 for the 3.00 bypass encoding).
    pub divider: u16,
    /// The resulting SD clock in Hz.
    pub actual_hz: u32,
    /// The requested frequency in Hz.
    pub target_hz: u32,
}

impl ClockPlan {
    /// Choose the smallest divider that does not exceed `target_hz`.
    pub fn for_frequency(
        base_hz: u32,
        target_hz: u32,
        spec: SpecVersion,
    ) -> Result<Self, ClockError> {
        if base_hz == 0 {
            return Err(ClockError::BaseClockZero);
        }
        if target_hz == 0 {
            return Err(ClockError::TargetZero);
        }
        if spec.uses_ten_bit_divider() {
            let raw = base_hz.div_ceil(target_hz);
            let divider = if raw > 1 { raw.div_ceil(2) } else { 0 };
            if divider > 0x3ff {
                return Err(ClockError::TooSlow);
            }
            let control = (((divider >> 8) & 0x3) as u16) << CLOCK_DIVIDER_HIGH_SHIFT
                | ((divider & 0xff) as u16) << CLOCK_DIVIDER_LOW_SHIFT;
            let actual_hz = if divider == 0 {
                base_hz
            } else {
                base_hz / (2 * divider)
            };
            return Ok(Self {
                control,
                divider: divider as u16,
                actual_hz,
                target_hz,
            });
        }

        let mut divider = 1u32;
        loop {
            let actual_hz = base_hz / divider;
            if actual_hz <= target_hz {
                let control = ((divider / 2) as u16) << CLOCK_DIVIDER_LOW_SHIFT;
                return Ok(Self {
                    control,
                    divider: divider as u16,
                    actual_hz,
                    target_hz,
                });
            }
            if divider >= 256 {
                return Err(ClockError::TooSlow);
            }
            divider <<= 1;
        }
    }
}

/// One of the three card speed modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpeedMode {
    /// Card identification at 400 kHz.
    Identification,
    /// Default speed at 25 MHz.
    Default,
    /// High speed at 50 MHz.
    HighSpeed,
}

impl SpeedMode {
    /// The SD clock this mode uses.
    pub const fn frequency_hz(self) -> u32 {
        match self {
            Self::Identification => 400_000,
            Self::Default => 25_000_000,
            Self::HighSpeed => 50_000_000,
        }
    }

    /// A short name for logs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Identification => "identification",
            Self::Default => "default",
            Self::HighSpeed => "high-speed",
        }
    }
}

/// Data bus width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BusWidth {
    /// One data line (DAT0).
    Bit1,
    /// Four data lines (DAT0..DAT3).
    Bit4,
}

impl BusWidth {
    /// The ACMD6 argument that selects this width.
    pub const fn acmd6_argument(self) -> u32 {
        match self {
            Self::Bit1 => 0,
            Self::Bit4 => 2,
        }
    }

    /// The host-control bits that select this width.
    pub const fn host_control_bits(self) -> u8 {
        match self {
            Self::Bit1 => 0,
            Self::Bit4 => HOST_CONTROL_4BIT,
        }
    }

    /// A short name for logs.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Bit1 => "1-bit",
            Self::Bit4 => "4-bit",
        }
    }
}

/// DMA path selected in the host-control register.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DmaMode {
    /// Programmed I/O through the data port.
    Pio,
    /// Simple DMA.
    Sdma,
    /// ADMA2 descriptor tables.
    Adma2,
}

impl DmaMode {
    /// The host-control DMA-select bits.
    pub const fn host_control_bits(self) -> u8 {
        match self {
            Self::Pio | Self::Sdma => 0,
            Self::Adma2 => 2 << HOST_CONTROL_DMA_SHIFT,
        }
    }
}

/// Why a timing transition was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimingError {
    /// The base clock is zero.
    BaseClockZero,
    /// The requested speed change is not the next legal step.
    InvalidSpeedTransition {
        /// The mode before the change.
        from: SpeedMode,
        /// The requested mode.
        to: SpeedMode,
    },
    /// 4-bit mode cannot be selected before the card leaves identification.
    InvalidBusWidth {
        /// The mode in which the change was attempted.
        speed: SpeedMode,
        /// The requested width.
        width: BusWidth,
    },
    /// The divider arithmetic failed.
    Clock(ClockError),
}

impl From<ClockError> for TimingError {
    fn from(error: ClockError) -> Self {
        Self::Clock(error)
    }
}

/// The current speed mode and bus width of one card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timing {
    base_hz: u32,
    spec: SpecVersion,
    speed: SpeedMode,
    width: BusWidth,
}

impl Timing {
    /// Start at 400 kHz on one data line.
    pub const fn new(base_hz: u32, spec: SpecVersion) -> Result<Self, TimingError> {
        if base_hz == 0 {
            return Err(TimingError::BaseClockZero);
        }
        Ok(Self {
            base_hz,
            spec,
            speed: SpeedMode::Identification,
            width: BusWidth::Bit1,
        })
    }

    /// The controller base clock in Hz.
    pub const fn base_hz(self) -> u32 {
        self.base_hz
    }

    /// The controller specification generation.
    pub const fn spec(self) -> SpecVersion {
        self.spec
    }

    /// The current speed mode.
    pub const fn speed(self) -> SpeedMode {
        self.speed
    }

    /// The current bus width.
    pub const fn width(self) -> BusWidth {
        self.width
    }

    /// The clock plan for the current mode.
    pub fn clock_plan(self) -> Result<ClockPlan, ClockError> {
        ClockPlan::for_frequency(self.base_hz, self.speed.frequency_hz(), self.spec)
    }

    /// Switch from identification to the 25 MHz default mode.
    pub fn switch_to_default(&mut self) -> Result<ClockPlan, TimingError> {
        if self.speed != SpeedMode::Identification {
            return Err(TimingError::InvalidSpeedTransition {
                from: self.speed,
                to: SpeedMode::Default,
            });
        }
        self.speed = SpeedMode::Default;
        self.clock_plan().map_err(TimingError::from)
    }

    /// Switch from default to the 50 MHz high-speed mode.
    pub fn switch_to_high_speed(&mut self) -> Result<ClockPlan, TimingError> {
        if self.speed != SpeedMode::Default {
            return Err(TimingError::InvalidSpeedTransition {
                from: self.speed,
                to: SpeedMode::HighSpeed,
            });
        }
        self.speed = SpeedMode::HighSpeed;
        self.clock_plan().map_err(TimingError::from)
    }

    /// Select the data bus width.
    ///
    /// 4-bit mode is legal only after the card leaves identification; 1-bit mode is always legal.
    pub fn set_bus_width(&mut self, width: BusWidth) -> Result<(), TimingError> {
        if width == BusWidth::Bit4 && self.speed == SpeedMode::Identification {
            return Err(TimingError::InvalidBusWidth {
                speed: self.speed,
                width,
            });
        }
        self.width = width;
        Ok(())
    }

    /// The host-control register value for the current timing and DMA mode.
    pub const fn host_control(self, quirks: Quirks, dma: DmaMode) -> u8 {
        let mut value = self.width.host_control_bits();
        if matches!(self.speed, SpeedMode::HighSpeed) && !quirks.no_high_speed_bit {
            value |= HOST_CONTROL_HIGH_SPEED;
        }
        value | dma.host_control_bits() & HOST_CONTROL_DMA_MASK
    }
}

/// One documented speed step for the Pi 4 card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpeedPoint {
    /// Speed mode.
    pub mode: SpeedMode,
    /// SD clock in Hz.
    pub frequency_hz: u32,
    /// Data bus width.
    pub width: BusWidth,
    /// What the step proves.
    pub purpose: &'static str,
}

/// The Pi 4 EMMC2 speed steps in order: identify, default, high speed.
pub const PI4_SPEED_STEPS: [SpeedPoint; 3] = [
    SpeedPoint {
        mode: SpeedMode::Identification,
        frequency_hz: 400_000,
        width: BusWidth::Bit1,
        purpose: "CMD0/CMD8/ACMD41/CMD2/CMD3 on one data line",
    },
    SpeedPoint {
        mode: SpeedMode::Default,
        frequency_hz: 25_000_000,
        width: BusWidth::Bit4,
        purpose: "ACMD6 moves the card to four data lines",
    },
    SpeedPoint {
        mode: SpeedMode::HighSpeed,
        frequency_hz: 50_000_000,
        width: BusWidth::Bit4,
        purpose: "CMD6 function 1 selects 50 MHz card timing",
    },
];

#[cfg(test)]
#[path = "timing_ut.rs"]
mod tests;
