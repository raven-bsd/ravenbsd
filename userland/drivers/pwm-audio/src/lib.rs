// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable Raspberry Pi 4 PWM1 audio driver skeleton.
//!
//! [`Seam`] is the board boundary: it owns the PWM/CPRMAN/GPIO/DMA register windows and the DMA
//! buffer. [`PwmAudio`] performs the bring-up sequence, builds the four-period DMA control-block
//! ring and consumes the `audio:` PCM ring through [`raven_proto_audio`]. The real target mapping
//! of `MmioRegion`/`DmaRegion`, the `platd` clock/pin leases and the audiod profile wiring remain
//! board-integration steps; no hardware result is claimed here.

#![no_std]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

#[cfg(test)]
extern crate std;

use raven_proto_audio::ring::{RingConsumer, RingHeader, RingLayout};
use raven_proto_audio::{Format, SampleFormat, StreamState};
use raven_pwm::clock::{
    CM_BUSY_POLLS, CM_CTL_START, CM_DIV_START, PWM_CLOCK_HZ, clock_busy, clock_stop_value,
};
use raven_pwm::dma::{
    CONTROL_BLOCK_LEN, CS_ABORT, CS_ACTIVE, CS_COMPLETION_CLEAR, CS_RESET_START, DmaError,
    RingGeometry,
};
use raven_pwm::duty::{DutyError, STEREO_FRAME_BYTES, SampleOrder, convert_period, silence_period};
use raven_pwm::gpio::configure as configure_pins;
use raven_pwm::registers::{
    CM_PWMCTL, CM_PWMDIV, CTL, CTL_AUDIO, CTL_CLRF1, DMAC, DMAC_AUDIO, GPIO_GPFSEL4,
    GPIO_GPPUPPDN2, PWM1_BASE, PWM1_SIZE, RNG1, RNG2, STA, STA_CLEAR_ERRORS,
};

/// Largest period this skeleton accepts (the B41 default is 1024 frames).
pub const MAX_PERIOD_FRAMES: u32 = 1024;
/// Largest PCM period buffer in bytes.
pub const MAX_PERIOD_BYTES: usize = MAX_PERIOD_FRAMES as usize * 4;
/// Largest duty word buffer.
pub const MAX_DUTY_WORDS: usize = MAX_PERIOD_FRAMES as usize * 2;
/// Default PCM period size from B41.
pub const DEFAULT_PERIOD_FRAMES: u32 = 1024;
/// Default number of periods from B41.
pub const DEFAULT_PERIOD_COUNT: u16 = 4;
/// Default DMA control-block offset in the test/profile window.
pub const DEFAULT_CB_OFFSET: usize = 0x8000;
/// Default duty-buffer offset.
pub const DEFAULT_DUTY_OFFSET: usize = 0x0000;
/// Default DMA window length.
pub const DEFAULT_DMA_LEN: usize = 0x10000;
/// MMIO window length used by the driver.
pub const MMIO_LEN: usize = PWM1_SIZE as usize;

/// The register window an access targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Window {
    /// PWM1 registers at `0xfe20c800`.
    Pwm,
    /// CPRMAN registers at `0xfe101000`.
    Clock,
    /// GPIO registers at `0xfe200000`.
    Gpio,
    /// DMA controller registers at `0xfe007000`.
    Dma,
}

/// A refused board access.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeamError {
    /// The offset leaves its window.
    Bounds,
    /// The access is not naturally aligned.
    Alignment,
    /// The underlying device access failed.
    Io,
    /// The platform did not grant the PWM pins.
    PinsRefused,
    /// The platform returned an unusable clock.
    ClockRefused,
}

/// The board boundary for the PWM audio driver.
pub trait Seam {
    /// Read one 32-bit register from `window`.
    fn read32(&mut self, window: Window, offset: usize) -> Result<u32, SeamError>;
    /// Write one 32-bit register to `window`.
    fn write32(&mut self, window: Window, offset: usize, value: u32) -> Result<(), SeamError>;
    /// Write bytes into the driver DMA window.
    fn dma_write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), SeamError>;
    /// Write 32-bit words into the driver DMA window.
    fn dma_write_u32(&mut self, offset: usize, words: &[u32]) -> Result<(), SeamError>;
    /// Device-visible base address of the DMA window.
    fn dma_bus_addr(&self) -> u64;
    /// DMA window length.
    fn dma_len(&self) -> usize;
    /// Claim GPIO40/41 through `platd`.
    fn claim_pwm_pins(&mut self) -> Result<(), SeamError>;
    /// Return the PWM clock rate granted by `platd`.
    fn pwm_clock_hz(&mut self) -> Result<u32, SeamError>;
    /// Wait between bounded register polls.
    fn delay_us(&mut self, _microseconds: u32) {}
}

/// Why the PWM driver could not continue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverError {
    /// The board seam refused an access.
    Seam(SeamError),
    /// A DMA geometry or control block was invalid.
    Dma(DmaError),
    /// The PCM ring layout or header was invalid.
    Layout,
    /// The format is not stereo S16LE.
    UnsupportedFormat,
    /// The requested period is larger than the skeleton accepts.
    PeriodTooLarge(u32),
    /// The PWM range is zero or unsupported.
    BadRange(u32),
    /// The clock is not the documented 100 MHz.
    UnsupportedClock(u32),
    /// The driver has not been prepared.
    NotPrepared,
    /// The driver is not running.
    NotRunning,
    /// The PCM ring underran.
    Underrun,
}

impl From<SeamError> for DriverError {
    fn from(error: SeamError) -> Self {
        Self::Seam(error)
    }
}

impl From<DmaError> for DriverError {
    fn from(error: DmaError) -> Self {
        Self::Dma(error)
    }
}

impl From<DutyError> for DriverError {
    fn from(_: DutyError) -> Self {
        Self::Layout
    }
}

/// The static configuration of one PWM audio stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PwmConfig {
    /// PCM stream format; only stereo S16LE is supported.
    pub format: Format,
    /// Exact PCM ring region length.
    pub pcm_len: usize,
    /// Frames per period.
    pub period_frames: u32,
    /// Number of periods in the PCM ring.
    pub period_count: u16,
    /// Control-block offset in the DMA window.
    pub cb_offset: usize,
    /// Duty buffer offset in the DMA window.
    pub duty_offset: usize,
    /// DMA channel allocated by the platform.
    pub dma_channel: u8,
    /// PWM range (`RNG1`/`RNG2`).
    pub range: u32,
    /// FIFO channel order.
    pub order: SampleOrder,
}

/// One service result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PeriodReport {
    /// Frames written to the duty ring.
    pub frames: usize,
    /// True when the PCM ring underran and silence was played.
    pub underrun: bool,
}

/// A typed driver state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// No register or DMA access has happened.
    New,
    /// The clock, pins, PWM registers and CB ring are programmed.
    Prepared,
    /// The PWM channels and DMA are running.
    Running,
    /// The PWM channels and DMA are stopped.
    Stopped,
}

/// The host-testable PWM audio driver.
pub struct PwmAudio<S: Seam> {
    seam: S,
    config: PwmConfig,
    ring: RingLayout,
    geometry: RingGeometry,
    state: State,
    duty_index: u16,
    pcm_period: [u8; MAX_PERIOD_BYTES],
    duty_period: [u32; MAX_DUTY_WORDS],
}

impl<S: Seam> PwmAudio<S> {
    /// Build the driver from a board seam and a configuration.
    pub fn new(seam: S, config: PwmConfig) -> Result<Self, DriverError> {
        if config.format.channels != 2 || config.format.sample != SampleFormat::S16Le {
            return Err(DriverError::UnsupportedFormat);
        }
        if config.period_frames == 0 || config.period_frames > MAX_PERIOD_FRAMES {
            return Err(DriverError::PeriodTooLarge(config.period_frames));
        }
        if config.range == 0 {
            return Err(DriverError::BadRange(config.range));
        }
        let ring = RingLayout::new(
            config.pcm_len,
            config.format,
            config.period_frames,
            config.period_count,
        )
        .map_err(|_| DriverError::Layout)?;
        let bus = seam.dma_bus_addr();
        if bus > u32::MAX as u64 {
            return Err(DriverError::Dma(DmaError::AddressOverflow));
        }
        let geometry = RingGeometry::new(
            bus as u32,
            seam.dma_len(),
            config.cb_offset,
            config.duty_offset,
            config.period_frames,
            config.period_count,
            config.dma_channel,
        )?;
        Ok(Self {
            seam,
            config,
            ring,
            geometry,
            state: State::New,
            duty_index: 0,
            pcm_period: [0u8; MAX_PERIOD_BYTES],
            duty_period: [0u32; MAX_DUTY_WORDS],
        })
    }

    /// The current driver state.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// The validated PCM ring layout.
    #[must_use]
    pub const fn ring(&self) -> RingLayout {
        self.ring
    }

    /// The DMA geometry.
    #[must_use]
    pub const fn geometry(&self) -> RingGeometry {
        self.geometry
    }

    /// Program pins, clock, PWM registers, the silence duty buffer and the CB ring.
    pub fn prepare(&mut self) -> Result<(), DriverError> {
        if self.state != State::New {
            return Err(DriverError::NotPrepared);
        }
        self.seam.claim_pwm_pins()?;
        let clock = self.seam.pwm_clock_hz()?;
        if clock != PWM_CLOCK_HZ {
            return Err(DriverError::UnsupportedClock(clock));
        }
        self.program_clock()?;
        self.program_pins()?;
        self.program_pwm()?;
        self.program_duty_buffer()?;
        self.program_control_blocks()?;
        self.state = State::Prepared;
        Ok(())
    }

    /// Enable the PWM channels and start the DMA channel.
    pub fn start(&mut self) -> Result<(), DriverError> {
        if self.state != State::Prepared {
            return Err(DriverError::NotPrepared);
        }
        let base = self
            .geometry
            .channel_base()
            .ok_or(DriverError::Dma(DmaError::InvalidChannel))?;
        let enable = self
            .seam
            .read32(Window::Dma, RingGeometry::enable_register())?
            | self.geometry.enable_bit();
        self.seam
            .write32(Window::Dma, RingGeometry::enable_register(), enable)?;
        self.seam.write32(Window::Dma, base, CS_RESET_START)?;
        self.wait_channel(base, CS_RESET_START, false)?;
        self.seam.write32(Window::Dma, base, CS_COMPLETION_CLEAR)?;
        self.seam.write32(
            Window::Dma,
            base + 0x04,
            self.geometry
                .cb_bus(0)
                .ok_or(DriverError::Dma(DmaError::NoBlock))?,
        )?;
        self.seam.write32(Window::Pwm, CTL, CTL_AUDIO)?;
        self.seam.write32(Window::Dma, base, CS_ACTIVE)?;
        self.state = State::Running;
        Ok(())
    }

    /// Stop the PWM channels and abort the DMA channel.
    pub fn stop(&mut self) -> Result<(), DriverError> {
        if self.state != State::Running && self.state != State::Prepared {
            return Err(DriverError::NotRunning);
        }
        self.seam.write32(Window::Pwm, CTL, 0)?;
        self.seam.write32(Window::Pwm, DMAC, 0)?;
        let base = self
            .geometry
            .channel_base()
            .ok_or(DriverError::Dma(DmaError::InvalidChannel))?;
        let cs = self.seam.read32(Window::Dma, base)?;
        self.seam
            .write32(Window::Dma, base, (cs & !CS_ACTIVE) | CS_ABORT)?;
        self.wait_channel(base, CS_ACTIVE, false)?;
        self.seam.write32(Window::Dma, base, CS_RESET_START)?;
        self.wait_channel(base, CS_RESET_START, false)?;
        let enable = self
            .seam
            .read32(Window::Dma, RingGeometry::enable_register())?
            & !self.geometry.enable_bit();
        self.seam
            .write32(Window::Dma, RingGeometry::enable_register(), enable)?;
        self.state = State::Stopped;
        Ok(())
    }

    /// Consume one PCM period and write its duty words to the DMA ring.
    ///
    /// The ring header state controls whether data is played: only `Running` and `Draining`
    /// produce a period. A short ring becomes a silence period and increments the header's
    /// underrun counter through `RingConsumer::read_period`.
    pub fn service(&mut self, pcm_region: &mut [u8]) -> Result<PeriodReport, DriverError> {
        if self.state != State::Running {
            return Err(DriverError::NotRunning);
        }
        let header = RingHeader::read_from(pcm_region).map_err(|_| DriverError::Layout)?;
        let layout =
            RingLayout::from_header(&header, pcm_region.len()).map_err(|_| DriverError::Layout)?;
        if layout != self.ring {
            return Err(DriverError::Layout);
        }
        let mut consumer =
            RingConsumer::new(pcm_region, layout).map_err(|_| DriverError::Layout)?;
        if matches!(consumer.state(), StreamState::Stopped) {
            return Ok(PeriodReport::default());
        }
        let pcm_bytes = self.config.period_frames as usize * STEREO_FRAME_BYTES;
        let frames = match consumer.read_period(&mut self.pcm_period[..pcm_bytes]) {
            Ok(frames) => {
                let used = frames.min(self.config.period_frames as usize) * STEREO_FRAME_BYTES;
                convert_period(
                    &self.pcm_period[..used],
                    self.config.range,
                    self.config.order,
                    &mut self.duty_period[..self.config.period_frames as usize * 2],
                )?;
                frames
            }
            Err(raven_proto_audio::RingError::Underrun) => {
                silence_period(
                    self.config.range,
                    self.config.period_frames as usize,
                    &mut self.duty_period[..self.config.period_frames as usize * 2],
                )?;
                return self.write_period(0, true);
            }
            Err(_) => return Err(DriverError::Layout),
        };
        self.write_period(frames, false)
    }

    fn write_period(&mut self, frames: usize, underrun: bool) -> Result<PeriodReport, DriverError> {
        let words = self.config.period_frames as usize * 2;
        let offset = self
            .geometry
            .duty_offset
            .checked_add(usize::from(self.duty_index) * self.geometry.period_bytes())
            .ok_or(DriverError::Dma(DmaError::RegionTooSmall))?;
        self.seam
            .dma_write_u32(offset, &self.duty_period[..words])?;
        self.duty_index = if self.duty_index + 1 >= self.config.period_count {
            0
        } else {
            self.duty_index + 1
        };
        Ok(PeriodReport { frames, underrun })
    }

    fn program_clock(&mut self) -> Result<(), DriverError> {
        let current = self.seam.read32(Window::Clock, CM_PWMCTL)?;
        self.seam
            .write32(Window::Clock, CM_PWMCTL, clock_stop_value(current))?;
        self.wait_clock(false)?;
        self.seam.write32(Window::Clock, CM_PWMDIV, CM_DIV_START)?;
        self.seam.write32(Window::Clock, CM_PWMCTL, CM_CTL_START)?;
        self.wait_clock(true)
    }

    fn wait_clock(&mut self, busy: bool) -> Result<(), DriverError> {
        for _ in 0..CM_BUSY_POLLS {
            if clock_busy(self.seam.read32(Window::Clock, CM_PWMCTL)?) == busy {
                return Ok(());
            }
            self.seam.delay_us(10);
        }
        Err(DriverError::UnsupportedClock(0))
    }

    fn wait_channel(&mut self, base: usize, bit: u32, set: bool) -> Result<(), DriverError> {
        for _ in 0..CM_BUSY_POLLS {
            let cs = self.seam.read32(Window::Dma, base)?;
            if ((cs & bit) != 0) == set {
                return Ok(());
            }
            self.seam.delay_us(10);
        }
        Err(DriverError::Dma(DmaError::ResetTimeout))
    }

    fn program_pins(&mut self) -> Result<(), DriverError> {
        let current_fsel = self.seam.read32(Window::Gpio, GPIO_GPFSEL4)?;
        let current_pull = self.seam.read32(Window::Gpio, GPIO_GPPUPPDN2)?;
        let plan = configure_pins(current_fsel, current_pull);
        self.seam
            .write32(Window::Gpio, GPIO_GPFSEL4, plan.gpfsel4)?;
        self.seam
            .write32(Window::Gpio, GPIO_GPPUPPDN2, plan.gppuppdn2)?;
        Ok(())
    }

    fn program_pwm(&mut self) -> Result<(), DriverError> {
        self.seam.write32(Window::Pwm, DMAC, 0)?;
        self.seam.write32(Window::Pwm, CTL, 0)?;
        self.seam.write32(Window::Pwm, CTL, CTL_CLRF1)?;
        self.seam.delay_us(100);
        self.seam.write32(Window::Pwm, STA, STA_CLEAR_ERRORS)?;
        self.seam.write32(Window::Pwm, DMAC, DMAC_AUDIO)?;
        self.seam.write32(Window::Pwm, RNG1, self.config.range)?;
        self.seam.write32(Window::Pwm, RNG2, self.config.range)?;
        Ok(())
    }

    fn program_duty_buffer(&mut self) -> Result<(), DriverError> {
        let words = self.config.period_frames as usize * 2;
        self.duty_period[..words].fill(self.config.range / 2);
        for index in 0..self.config.period_count {
            let offset = self
                .geometry
                .duty_offset
                .checked_add(usize::from(index) * self.geometry.period_bytes())
                .ok_or(DriverError::Dma(DmaError::RegionTooSmall))?;
            self.seam
                .dma_write_u32(offset, &self.duty_period[..words])?;
        }
        Ok(())
    }

    fn program_control_blocks(&mut self) -> Result<(), DriverError> {
        for index in 0..self.config.period_count {
            let block = self
                .geometry
                .control_block(index)
                .ok_or(DriverError::Dma(DmaError::NoBlock))?;
            let offset = self
                .geometry
                .cb_offset
                .checked_add(usize::from(index) * CONTROL_BLOCK_LEN)
                .ok_or(DriverError::Dma(DmaError::RegionTooSmall))?;
            self.seam.dma_write(offset, &block.encode())?;
        }
        Ok(())
    }
}

/// The PWM1 window facts exposed for the target main and documentation.
pub const fn pwm_window() -> (u64, usize) {
    (PWM1_BASE, MMIO_LEN)
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
