// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic combined soak for the Pi 4 GENET frame model and the PWM audio ring.
//!
//! The soak runs entirely against the host-testable `raven-genet`, `raven-pwm` and `pwm-audio`
//! models. It touches no MMIO, DMA or board resource: the PWM driver uses a fake seam and the
//! GENET path uses the in-memory descriptor model. The same code is the `/init` of the
//! `pi4-soak` qtest fixture, so the host test and the guest run exercise the same loops.
//!
//! The loops interleave network RX/TX ownership, malformed completions, link flaps, interrupt
//! decode, the bounded recovery budget and audio periods with periodic underruns.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use pwm_audio::{PwmAudio, PwmConfig, Seam, SeamError, Window};
use raven_genet::frame::{FrameError, RxPath, TxPath};
use raven_genet::irq::InterruptStatus;
use raven_genet::link::LinkTracker;
use raven_genet::phy::{AUXSTS_RES_1000FD, BMSR_ACOMP, BMSR_LINK};
use raven_genet::recovery::{ErrorRecovery, RecoveryAction};
use raven_genet::regs::{IRQ_RXDMA_DONE, IRQ_TXDMA_DONE};
use raven_genet::ring::{
    RX_STATUS_BUFLEN_SHIFT, RX_STATUS_CRC_ERR, RX_STATUS_EOP, RX_STATUS_OWN, RX_STATUS_SOP,
    RxStatus, TX_STATUS_OWN, TxStatus,
};
use raven_proto_audio::ring::{RingHeader, RingLayout, RingProducer};
use raven_proto_audio::{Format, SampleFormat};
use raven_pwm::dma::{CS_ABORT, CS_ACTIVE, CS_RESET_START};
use raven_pwm::duty::SampleOrder;
use raven_pwm::registers::{CM_CTL_BUSY, CM_CTL_ENAB, CM_PWMCTL};

/// Iterations in the default soak.
pub const DEFAULT_ITERATIONS: usize = 1024;
/// Upper bound that keeps the report arithmetic and the guest run bounded.
pub const MAX_ITERATIONS: usize = 65_536;

const RX_SLOTS: u16 = 64;
const AUDIO_PERIOD_FRAMES: u32 = 1024;
const AUDIO_PERIODS: u16 = 4;
const AUDIO_FRAME_BYTES: usize = 4;
const AUDIO_REGION_LEN: usize =
    128 + AUDIO_PERIOD_FRAMES as usize * AUDIO_FRAME_BYTES * AUDIO_PERIODS as usize;
const DMA_WINDOW_LEN: usize = 0x1_0000;
const DMA_BUS_BASE: u64 = 0x1000_0000;
const UNDERRUN_EVERY: usize = 16;
const IRQ_EVERY: usize = 32;

static RUN_LOCK: AtomicBool = AtomicBool::new(false);
static DMA_WRITES: AtomicUsize = AtomicUsize::new(0);
static DMA_BYTES: AtomicUsize = AtomicUsize::new(0);

struct RunGuard;

impl Drop for RunGuard {
    fn drop(&mut self) {
        RUN_LOCK.store(false, Ordering::Release);
    }
}

/// Counters and checks from one complete soak.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Report {
    /// Requested iterations.
    pub iterations: usize,
    /// RX completions, including malformed frames.
    pub rx_completed: usize,
    /// TX completions reclaimed with a valid status.
    pub tx_completed: usize,
    /// RX completions whose frame status was malformed.
    pub rx_errors: usize,
    /// TX completions rejected as malformed.
    pub tx_errors: usize,
    /// Still-owned RX descriptors observed and recovered.
    pub owned_checks: usize,
    /// Debounced link transitions.
    pub link_events: usize,
    /// Combined RX/TX interrupt-status checks.
    pub irq_checks: usize,
    /// Ring resets granted by the recovery policy.
    pub recovery_resets: u32,
    /// True when the recovery policy reached its fault state.
    pub recovery_fault: bool,
    /// Audio periods that carried PCM frames.
    pub audio_periods: usize,
    /// PCM frames converted and written to the duty ring.
    pub audio_frames: usize,
    /// Audio periods replaced by silence after an underrun.
    pub audio_underruns: usize,
    /// DMA writes issued by the PWM driver (four duty + four CB writes in prepare).
    pub audio_writes: usize,
    /// DMA bytes accepted by the fake window.
    pub audio_bytes: usize,
}

/// Reason a soak could not run to completion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoakError {
    /// The iteration count was zero.
    ZeroIterations,
    /// The iteration count exceeded [`MAX_ITERATIONS`].
    TooManyIterations,
    /// GENET ring or status state was inconsistent.
    Genet,
    /// The GENET recovery policy did not reach its expected budget.
    Recovery,
    /// The audio ring layout or header was inconsistent.
    Audio,
    /// The PWM driver rejected a bring-up or service step.
    AudioDriver,
    /// The audio period report did not match the injected schedule.
    AudioReport,
}

/// Run the combined GENET and PWM audio soak.
pub fn run(iterations: usize) -> Result<Report, SoakError> {
    if iterations == 0 {
        return Err(SoakError::ZeroIterations);
    }
    if iterations > MAX_ITERATIONS {
        return Err(SoakError::TooManyIterations);
    }

    while RUN_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let _guard = RunGuard;
    DMA_WRITES.store(0, Ordering::Relaxed);
    DMA_BYTES.store(0, Ordering::Relaxed);

    let layout = audio_layout()?;
    let mut region = [0u8; AUDIO_REGION_LEN];
    layout
        .write_header(&mut region, 0)
        .map_err(|_| SoakError::Audio)?;
    {
        let mut producer = RingProducer::new(&mut region, layout).map_err(|_| SoakError::Audio)?;
        producer.start();
    }

    let mut driver =
        PwmAudio::new(FakeSeam::new(), audio_config()).map_err(|_| SoakError::AudioDriver)?;
    driver.prepare().map_err(|_| SoakError::AudioDriver)?;
    driver.start().map_err(|_| SoakError::AudioDriver)?;

    let mut pcm = [0u8; AUDIO_PERIOD_FRAMES as usize * AUDIO_FRAME_BYTES];
    fill_pcm(&mut pcm);

    let mut rx = RxPath::new(RX_SLOTS).map_err(|_| SoakError::Genet)?;
    let mut tx = TxPath::new(RX_SLOTS).map_err(|_| SoakError::Genet)?;
    let mut link = LinkTracker::new(2);

    let mut rx_completed = 0usize;
    let mut tx_completed = 0usize;
    let mut rx_errors = 0usize;
    let mut tx_errors = 0usize;
    let mut owned_checks = 0usize;
    let mut link_events = 0usize;
    let mut irq_checks = 0usize;
    let mut audio_periods = 0usize;
    let mut audio_frames = 0usize;
    let mut audio_underruns = 0usize;

    for iteration in 0..iterations {
        let slot = (iteration % usize::from(RX_SLOTS)) as u64;
        rx.post_next(0x1000 + slot * 0x800)
            .map_err(|_| SoakError::Genet)?;
        let tx_post = tx
            .queue_next(0x10_0000 + slot * 0x800, 64)
            .map_err(|_| SoakError::Genet)?;

        let (bmsr, auxsts) = if iteration.is_multiple_of(2) {
            (BMSR_LINK | BMSR_ACOMP, AUXSTS_RES_1000FD)
        } else {
            (0, 0)
        };
        link.observe(bmsr, auxsts).map_err(|_| SoakError::Genet)?;
        if link
            .observe(bmsr, auxsts)
            .map_err(|_| SoakError::Genet)?
            .is_some()
        {
            link_events += 1;
        }

        if iteration.is_multiple_of(7) {
            let owned = RxStatus::from_raw(rx_frame(64).raw() | RX_STATUS_OWN);
            match rx.complete_next(owned) {
                Err(FrameError::StillOwned(_)) => owned_checks += 1,
                _ => return Err(SoakError::Genet),
            }
        }

        let rx_status = if iteration.is_multiple_of(8) {
            RxStatus::from_raw(rx_frame(64).raw() | RX_STATUS_CRC_ERR)
        } else {
            rx_frame(64)
        };
        let completion = rx
            .complete_next(rx_status)
            .map_err(|_| SoakError::Genet)?
            .ok_or(SoakError::Genet)?;
        rx_completed += 1;
        if completion.frame.is_err() {
            rx_errors += 1;
        }

        let tx_status = if iteration.is_multiple_of(9) {
            TxStatus::from_raw(0)
        } else {
            let descriptor = tx.descriptor(tx_post.index).ok_or(SoakError::Genet)?;
            completed_tx(descriptor.status)
        };
        match tx.reclaim_next(tx_status) {
            Ok(Some(_)) => tx_completed += 1,
            Err(FrameError::BadStatus(_)) => tx_errors += 1,
            _ => return Err(SoakError::Genet),
        }

        if iteration.is_multiple_of(IRQ_EVERY) {
            let status = InterruptStatus::from_raw(IRQ_RXDMA_DONE | IRQ_TXDMA_DONE);
            if status.rx_done() && status.tx_done() && !status.is_empty() {
                irq_checks += 1;
            } else {
                return Err(SoakError::Genet);
            }
        }

        let expected_underrun = iteration.is_multiple_of(UNDERRUN_EVERY);
        if !expected_underrun {
            let written = {
                let mut producer =
                    RingProducer::new(&mut region, layout).map_err(|_| SoakError::Audio)?;
                producer.write(&pcm).map_err(|_| SoakError::Audio)?
            };
            if written != AUDIO_PERIOD_FRAMES as usize {
                return Err(SoakError::Audio);
            }
        }
        let report = driver
            .service(&mut region)
            .map_err(|_| SoakError::AudioDriver)?;
        if report.underrun != expected_underrun {
            return Err(SoakError::AudioReport);
        }
        if expected_underrun {
            audio_underruns += 1;
        } else {
            if report.frames != AUDIO_PERIOD_FRAMES as usize {
                return Err(SoakError::AudioReport);
            }
            audio_periods += 1;
            audio_frames += report.frames;
        }
    }

    if rx.posted() != Ok(0) || tx.posted() != Ok(0) {
        return Err(SoakError::Genet);
    }

    let (recovery_resets, recovery_fault) = recovery_budget()?;

    let header = RingHeader::read_from(&region).map_err(|_| SoakError::Audio)?;
    if header.underruns as usize != audio_underruns {
        return Err(SoakError::Audio);
    }
    driver.stop().map_err(|_| SoakError::AudioDriver)?;

    Ok(Report {
        iterations,
        rx_completed,
        tx_completed,
        rx_errors,
        tx_errors,
        owned_checks,
        link_events,
        irq_checks,
        recovery_resets,
        recovery_fault,
        audio_periods,
        audio_frames,
        audio_underruns,
        audio_writes: DMA_WRITES.load(Ordering::Relaxed),
        audio_bytes: DMA_BYTES.load(Ordering::Relaxed),
    })
}

fn recovery_budget() -> Result<(u32, bool), SoakError> {
    let mut recovery = ErrorRecovery::new(4, 3);
    let mut resets = 0u32;
    let mut fault = false;
    let mut round = 0;
    while round < 4 {
        let mut error = 0;
        while error < 4 {
            match recovery.record_error() {
                RecoveryAction::ResetRings => resets += 1,
                RecoveryAction::Fault => fault = true,
                RecoveryAction::None => {}
            }
            error += 1;
        }
        round += 1;
    }
    if resets != 3 || !fault || recovery.resets() != 3 {
        return Err(SoakError::Recovery);
    }
    Ok((resets, fault))
}

fn audio_layout() -> Result<RingLayout, SoakError> {
    RingLayout::new(
        AUDIO_REGION_LEN,
        audio_format(),
        AUDIO_PERIOD_FRAMES,
        AUDIO_PERIODS,
    )
    .map_err(|_| SoakError::Audio)
}

fn audio_format() -> Format {
    Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    }
}

fn audio_config() -> PwmConfig {
    PwmConfig {
        format: audio_format(),
        pcm_len: AUDIO_REGION_LEN,
        period_frames: AUDIO_PERIOD_FRAMES,
        period_count: AUDIO_PERIODS,
        cb_offset: 0x8000,
        duty_offset: 0,
        dma_channel: 5,
        range: 2083,
        order: SampleOrder::RightLeft,
    }
}

fn fill_pcm(buffer: &mut [u8]) {
    let mut index = 0usize;
    while index + AUDIO_FRAME_BYTES <= buffer.len() {
        let sample = (index as i16).wrapping_mul(131);
        buffer[index..index + 2].copy_from_slice(&sample.to_le_bytes());
        buffer[index + 2..index + 4].copy_from_slice(&sample.wrapping_neg().to_le_bytes());
        index += AUDIO_FRAME_BYTES;
    }
}

fn rx_frame(length: u32) -> RxStatus {
    RxStatus::from_raw((length << RX_STATUS_BUFLEN_SHIFT) | RX_STATUS_SOP | RX_STATUS_EOP)
}

fn completed_tx(status: TxStatus) -> TxStatus {
    TxStatus::from_raw(status.raw() & !TX_STATUS_OWN)
}

/// The fake PWM board seam. It models the CPRMAN BUSY handshake and the self-clearing DMA
/// channel CS bits, but keeps no DMA payload: the soak checks the ring and counters, not bytes.
struct FakeSeam {
    pwm: [u32; 0x40 / 4],
    clock: [u32; 0x100 / 4],
    gpio: [u32; 0x100 / 4],
    dma_regs: [u32; 0x1000 / 4],
}

impl FakeSeam {
    const fn new() -> Self {
        Self {
            pwm: [0; 0x40 / 4],
            clock: [0; 0x100 / 4],
            gpio: [0; 0x100 / 4],
            dma_regs: [0; 0x1000 / 4],
        }
    }

    fn dma_range(&self, offset: usize, length: usize) -> Result<(), SeamError> {
        let end = offset.checked_add(length).ok_or(SeamError::Bounds)?;
        if end > DMA_WINDOW_LEN {
            return Err(SeamError::Bounds);
        }
        Ok(())
    }
}

impl Seam for FakeSeam {
    fn read32(&mut self, window: Window, offset: usize) -> Result<u32, SeamError> {
        let words = match window {
            Window::Pwm => &self.pwm[..],
            Window::Clock => &self.clock[..],
            Window::Gpio => &self.gpio[..],
            Window::Dma => &self.dma_regs[..],
        };
        let end = offset.checked_add(4).ok_or(SeamError::Bounds)?;
        if !offset.is_multiple_of(4) || end > words.len() * 4 {
            return Err(SeamError::Bounds);
        }
        Ok(words[offset / 4])
    }

    fn write32(&mut self, window: Window, offset: usize, value: u32) -> Result<(), SeamError> {
        let len = match window {
            Window::Pwm => self.pwm.len() * 4,
            Window::Clock => self.clock.len() * 4,
            Window::Gpio => self.gpio.len() * 4,
            Window::Dma => self.dma_regs.len() * 4,
        };
        let end = offset.checked_add(4).ok_or(SeamError::Bounds)?;
        if !offset.is_multiple_of(4) || end > len {
            return Err(SeamError::Bounds);
        }
        let stored = if window == Window::Clock && offset == CM_PWMCTL {
            if value & CM_CTL_ENAB != 0 {
                value | CM_CTL_BUSY
            } else {
                value & !CM_CTL_BUSY
            }
        } else if window == Window::Dma && offset < 0xf00 && offset.is_multiple_of(0x100) {
            let mut cs = value;
            if value & CS_RESET_START != 0 {
                cs &= !(CS_RESET_START | CS_ACTIVE);
            } else if value & CS_ABORT != 0 {
                cs &= !(CS_ABORT | CS_ACTIVE);
            }
            cs
        } else {
            value
        };
        match window {
            Window::Pwm => self.pwm[offset / 4] = stored,
            Window::Clock => self.clock[offset / 4] = stored,
            Window::Gpio => self.gpio[offset / 4] = stored,
            Window::Dma => self.dma_regs[offset / 4] = stored,
        }
        Ok(())
    }

    fn dma_write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), SeamError> {
        self.dma_range(offset, bytes.len())?;
        DMA_WRITES.fetch_add(1, Ordering::Relaxed);
        DMA_BYTES.fetch_add(bytes.len(), Ordering::Relaxed);
        Ok(())
    }

    fn dma_write_u32(&mut self, offset: usize, words: &[u32]) -> Result<(), SeamError> {
        if !offset.is_multiple_of(4) {
            return Err(SeamError::Alignment);
        }
        let bytes = words.len().checked_mul(4).ok_or(SeamError::Bounds)?;
        self.dma_range(offset, bytes)?;
        DMA_WRITES.fetch_add(1, Ordering::Relaxed);
        DMA_BYTES.fetch_add(bytes, Ordering::Relaxed);
        Ok(())
    }

    fn dma_bus_addr(&self) -> u64 {
        DMA_BUS_BASE
    }

    fn dma_len(&self) -> usize {
        DMA_WINDOW_LEN
    }

    fn claim_pwm_pins(&mut self) -> Result<(), SeamError> {
        Ok(())
    }

    fn pwm_clock_hz(&mut self) -> Result<u32, SeamError> {
        Ok(raven_pwm::clock::PWM_CLOCK_HZ)
    }
}

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
