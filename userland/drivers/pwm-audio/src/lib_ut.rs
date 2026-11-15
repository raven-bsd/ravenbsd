// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the PWM1 audio driver skeleton.
//!
//! The fake seam models the four register windows, the CPRMAN busy bit and a 64 KiB DMA
//! window, so every bring-up and period-service rule runs without hardware.

use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use raven_proto_audio::ring::RingProducer;
use raven_pwm::dma::ControlBlock;
use raven_pwm::duty::sample_to_duty;
use raven_pwm::registers::{CM_CTL_BUSY, CM_CTL_ENAB, CM_PASSWD};

use super::*;

/// Frames per test period.
const PERIOD_FRAMES: u32 = 1024;
/// Periods in the test ring.
const PERIOD_COUNT: u16 = 4;
/// Exact test ring region length: header alignment plus four stereo S16LE periods.
const REGION_LEN: usize = 128 + PERIOD_FRAMES as usize * 4 * PERIOD_COUNT as usize;

/// Observable state shared with the driver under test.
struct FakeState {
    pwm: [u32; 0x40 / 4],
    clock: [u32; 0x100 / 4],
    gpio: [u32; 0x100 / 4],
    dma_regs: [u32; 0x1000 / 4],
    dma: Vec<u8>,
    writes: Vec<(Window, usize, u32)>,
    dma_writes: Vec<usize>,
    pins_claimed: bool,
    pins_allowed: bool,
    clock_hz: u32,
    fail: Option<SeamError>,
}

impl FakeState {
    fn new() -> Self {
        Self {
            pwm: [0; 0x40 / 4],
            clock: [0; 0x100 / 4],
            gpio: [0; 0x100 / 4],
            dma_regs: [0; 0x1000 / 4],
            dma: std::vec![0u8; DEFAULT_DMA_LEN],
            writes: Vec::new(),
            dma_writes: Vec::new(),
            pins_claimed: false,
            pins_allowed: true,
            clock_hz: PWM_CLOCK_HZ,
            fail: None,
        }
    }

    fn register_writes(&self, window: Window, offset: usize) -> Vec<u32> {
        self.writes
            .iter()
            .filter(|(target, at, _)| *target == window && *at == offset)
            .map(|(_, _, value)| *value)
            .collect()
    }

    fn last_register_write(&self, window: Window, offset: usize) -> Option<u32> {
        self.register_writes(window, offset).last().copied()
    }

    fn dma_word(&self, offset: usize) -> u32 {
        let bytes = &self.dma[offset..offset + 4];
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
    }
}

/// The fake board seam.
struct FakeSeam {
    state: Rc<RefCell<FakeState>>,
}

impl FakeSeam {
    fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(FakeState::new())),
        }
    }

    fn handle(&self) -> Rc<RefCell<FakeState>> {
        Rc::clone(&self.state)
    }
}

impl Seam for FakeSeam {
    fn read32(&mut self, window: Window, offset: usize) -> Result<u32, SeamError> {
        let state = self.state.borrow();
        if let Some(error) = state.fail {
            return Err(error);
        }
        let words = match window {
            Window::Pwm => &state.pwm[..],
            Window::Clock => &state.clock[..],
            Window::Gpio => &state.gpio[..],
            Window::Dma => &state.dma_regs[..],
        };
        if !offset.is_multiple_of(4) || offset + 4 > words.len() * 4 {
            return Err(SeamError::Bounds);
        }
        Ok(words[offset / 4])
    }

    fn write32(&mut self, window: Window, offset: usize, value: u32) -> Result<(), SeamError> {
        let mut state = self.state.borrow_mut();
        if let Some(error) = state.fail {
            return Err(error);
        }
        let len = match window {
            Window::Pwm => state.pwm.len() * 4,
            Window::Clock => state.clock.len() * 4,
            Window::Gpio => state.gpio.len() * 4,
            Window::Dma => state.dma_regs.len() * 4,
        };
        if !offset.is_multiple_of(4) || offset + 4 > len {
            return Err(SeamError::Bounds);
        }
        state.writes.push((window, offset, value));
        let stored = if window == Window::Clock && offset == CM_PWMCTL {
            // Model the CPRMAN handshake: BUSY follows ENAB after the write completes.
            if value & CM_CTL_ENAB != 0 {
                value | CM_CTL_BUSY
            } else {
                value & !CM_CTL_BUSY
            }
        } else {
            value
        };
        match window {
            Window::Pwm => state.pwm[offset / 4] = stored,
            Window::Clock => state.clock[offset / 4] = stored,
            Window::Gpio => state.gpio[offset / 4] = stored,
            Window::Dma => state.dma_regs[offset / 4] = stored,
        }
        Ok(())
    }

    fn dma_write(&mut self, offset: usize, bytes: &[u8]) -> Result<(), SeamError> {
        let mut state = self.state.borrow_mut();
        if let Some(error) = state.fail {
            return Err(error);
        }
        let end = offset.checked_add(bytes.len()).ok_or(SeamError::Bounds)?;
        if end > state.dma.len() {
            return Err(SeamError::Bounds);
        }
        state.dma[offset..end].copy_from_slice(bytes);
        state.dma_writes.push(offset);
        Ok(())
    }

    fn dma_write_u32(&mut self, offset: usize, words: &[u32]) -> Result<(), SeamError> {
        let mut state = self.state.borrow_mut();
        if let Some(error) = state.fail {
            return Err(error);
        }
        if !offset.is_multiple_of(4) {
            return Err(SeamError::Alignment);
        }
        let bytes = words.len() * 4;
        let end = offset.checked_add(bytes).ok_or(SeamError::Bounds)?;
        if end > state.dma.len() {
            return Err(SeamError::Bounds);
        }
        for (index, word) in words.iter().enumerate() {
            let start = offset + index * 4;
            state.dma[start..start + 4].copy_from_slice(&word.to_le_bytes());
        }
        state.dma_writes.push(offset);
        Ok(())
    }

    fn dma_bus_addr(&self) -> u64 {
        0x1000_0000
    }

    fn dma_len(&self) -> usize {
        DEFAULT_DMA_LEN
    }

    fn claim_pwm_pins(&mut self) -> Result<(), SeamError> {
        let mut state = self.state.borrow_mut();
        if !state.pins_allowed {
            return Err(SeamError::PinsRefused);
        }
        state.pins_claimed = true;
        Ok(())
    }

    fn pwm_clock_hz(&mut self) -> Result<u32, SeamError> {
        let state = self.state.borrow();
        if let Some(error) = state.fail {
            return Err(error);
        }
        Ok(state.clock_hz)
    }
}

fn format() -> Format {
    Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    }
}

fn layout() -> RingLayout {
    RingLayout::new(REGION_LEN, format(), PERIOD_FRAMES, PERIOD_COUNT).expect("layout")
}

fn config() -> PwmConfig {
    PwmConfig {
        format: format(),
        pcm_len: REGION_LEN,
        period_frames: PERIOD_FRAMES,
        period_count: PERIOD_COUNT,
        cb_offset: DEFAULT_CB_OFFSET,
        duty_offset: DEFAULT_DUTY_OFFSET,
        dma_channel: 5,
        range: 2083,
        order: SampleOrder::RightLeft,
    }
}

fn new_driver() -> (PwmAudio<FakeSeam>, Rc<RefCell<FakeState>>) {
    let seam = FakeSeam::new();
    let handle = seam.handle();
    let driver = PwmAudio::new(seam, config()).expect("driver");
    (driver, handle)
}

fn prepared_driver() -> (PwmAudio<FakeSeam>, Rc<RefCell<FakeState>>) {
    let (mut driver, handle) = new_driver();
    driver.prepare().expect("prepare");
    (driver, handle)
}

fn ring_region() -> Vec<u8> {
    let layout = layout();
    let mut region = std::vec![0u8; layout.region_len()];
    layout.write_header(&mut region, 0).expect("header");
    region
}

fn write_period(region: &mut [u8], left: i16, right: i16) {
    let mut data = std::vec![0u8; PERIOD_FRAMES as usize * STEREO_FRAME_BYTES];
    for frame in data.chunks_exact_mut(STEREO_FRAME_BYTES) {
        frame[0..2].copy_from_slice(&left.to_le_bytes());
        frame[2..4].copy_from_slice(&right.to_le_bytes());
    }
    let layout = layout();
    let mut producer = RingProducer::new(region, layout).expect("producer");
    assert_eq!(producer.write(&data), Ok(PERIOD_FRAMES as usize));
}

fn start_stream(region: &mut [u8]) {
    let layout = layout();
    let mut producer = RingProducer::new(region, layout).expect("producer");
    producer.start();
}

fn running_driver() -> (PwmAudio<FakeSeam>, Rc<RefCell<FakeState>>, Vec<u8>) {
    let (mut driver, handle) = prepared_driver();
    driver.start().expect("start");
    let mut region = ring_region();
    start_stream(&mut region);
    (driver, handle, region)
}

#[test]
fn new_rejects_unsupported_formats() {
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.format.channels = 1;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::UnsupportedFormat)
    );
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.format.sample = SampleFormat::S24Le;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::UnsupportedFormat)
    );
}

#[test]
fn new_rejects_bad_period_range_and_layout() {
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.period_frames = 0;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::PeriodTooLarge(0))
    );
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.period_frames = MAX_PERIOD_FRAMES + 1;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::PeriodTooLarge(MAX_PERIOD_FRAMES + 1))
    );
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.range = 0;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::BadRange(0))
    );
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.pcm_len -= 4;
    assert_eq!(PwmAudio::new(seam, bad).err(), Some(DriverError::Layout));
}

#[test]
fn new_rejects_bad_dma_geometry() {
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.dma_channel = 15;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::Dma(DmaError::InvalidChannel))
    );
    let seam = FakeSeam::new();
    let mut bad = config();
    bad.cb_offset = 1;
    assert_eq!(
        PwmAudio::new(seam, bad).err(),
        Some(DriverError::Dma(DmaError::Unaligned))
    );
}

#[test]
fn prepare_programs_the_clock_pins_pwm_and_dma_ring() {
    let (driver, handle) = prepared_driver();
    assert_eq!(driver.state(), State::Prepared);
    let state = handle.borrow();
    assert!(state.pins_claimed);

    assert_eq!(
        state.register_writes(Window::Clock, CM_PWMCTL),
        [CM_PASSWD, CM_CTL_START]
    );
    assert_eq!(
        state.last_register_write(Window::Clock, CM_PWMDIV),
        Some(CM_DIV_START)
    );
    assert_eq!(
        state.last_register_write(Window::Gpio, GPIO_GPFSEL4),
        Some(0x24)
    );
    assert_eq!(
        state.last_register_write(Window::Gpio, GPIO_GPPUPPDN2),
        Some(0)
    );
    assert_eq!(state.last_register_write(Window::Pwm, CTL), Some(CTL_CLRF1));
    assert_eq!(
        state.last_register_write(Window::Pwm, STA),
        Some(STA_CLEAR_ERRORS)
    );
    assert_eq!(
        state.last_register_write(Window::Pwm, DMAC),
        Some(DMAC_AUDIO)
    );
    assert_eq!(state.last_register_write(Window::Pwm, RNG1), Some(2083));
    assert_eq!(state.last_register_write(Window::Pwm, RNG2), Some(2083));

    assert_eq!(state.dma_writes.len(), 8);
    for index in 0..PERIOD_COUNT as usize {
        let offset = DEFAULT_CB_OFFSET + index * CONTROL_BLOCK_LEN;
        let block = ControlBlock::decode(&state.dma[offset..offset + CONTROL_BLOCK_LEN])
            .expect("control block");
        assert_eq!(block.ti, raven_pwm::dma::TI_AUDIO);
        assert_eq!(block.source, 0x1000_0000 + index as u32 * 8192);
        assert_eq!(block.dest, raven_pwm::dma::FIFO_BUS);
        assert_eq!(block.length, 8192);
        let expected_next = if index + 1 == PERIOD_COUNT as usize {
            0x1000_8000
        } else {
            0x1000_8000 + (index as u32 + 1) * CONTROL_BLOCK_LEN as u32
        };
        assert_eq!(block.next, expected_next);
    }
    assert_eq!(state.dma_word(0), 1041);
    assert_eq!(state.dma_word(8192), 1041);
}

#[test]
fn prepare_requires_the_documented_clock() {
    let (mut driver, handle) = new_driver();
    handle.borrow_mut().clock_hz = 50_000_000;
    assert_eq!(
        driver.prepare(),
        Err(DriverError::UnsupportedClock(50_000_000))
    );
    assert_eq!(driver.state(), State::New);
}

#[test]
fn prepare_requires_the_pin_lease() {
    let (mut driver, handle) = new_driver();
    handle.borrow_mut().pins_allowed = false;
    assert_eq!(
        driver.prepare(),
        Err(DriverError::Seam(SeamError::PinsRefused))
    );
    assert_eq!(driver.state(), State::New);
}

#[test]
fn prepare_runs_only_once() {
    let (mut driver, _) = prepared_driver();
    assert_eq!(driver.prepare(), Err(DriverError::NotPrepared));
}

#[test]
fn start_and_stop_toggle_the_pwm_and_dma_channel() {
    let (mut driver, handle, _) = running_driver();
    assert_eq!(driver.state(), State::Running);
    let state = handle.borrow();
    assert_eq!(state.last_register_write(Window::Pwm, CTL), Some(CTL_AUDIO));
    assert_eq!(
        state.last_register_write(Window::Dma, 0x504),
        Some(0x1000_8000)
    );
    assert_eq!(
        state.last_register_write(Window::Dma, 0x500),
        Some(CS_ACTIVE)
    );
    assert_eq!(
        state.last_register_write(Window::Dma, RingGeometry::enable_register()),
        Some(1 << 5)
    );
    drop(state);

    driver.stop().expect("stop");
    assert_eq!(driver.state(), State::Stopped);
    let state = handle.borrow();
    assert_eq!(state.last_register_write(Window::Pwm, CTL), Some(0));
    assert_eq!(state.last_register_write(Window::Pwm, DMAC), Some(0));
    assert_eq!(
        state.last_register_write(Window::Dma, 0x500),
        Some(CS_RESET_START)
    );
    assert_eq!(
        state.last_register_write(Window::Dma, RingGeometry::enable_register()),
        Some(0)
    );
    assert_eq!(driver.stop(), Err(DriverError::NotRunning));
}

#[test]
fn stop_is_allowed_before_start() {
    let (mut driver, _) = prepared_driver();
    driver.stop().expect("stop prepared");
    assert_eq!(driver.state(), State::Stopped);
}

#[test]
fn service_converts_a_pcm_period_into_right_then_left_duty_words() {
    let (mut driver, handle, mut region) = running_driver();
    write_period(&mut region, 1000, -1000);
    let report = driver.service(&mut region).expect("service");
    assert_eq!(
        report,
        PeriodReport {
            frames: PERIOD_FRAMES as usize,
            underrun: false
        }
    );
    let state = handle.borrow();
    assert_eq!(
        state.dma_word(0),
        sample_to_duty(-1000, 2083).expect("right")
    );
    assert_eq!(state.dma_word(4), sample_to_duty(1000, 2083).expect("left"));
}

#[test]
fn service_advances_and_wraps_the_duty_slots() {
    let (mut driver, handle, mut region) = running_driver();
    for period in 0..PERIOD_COUNT {
        write_period(&mut region, period as i16, -(period as i16));
        driver.service(&mut region).expect("service");
    }
    write_period(&mut region, 1000, -1000);
    driver.service(&mut region).expect("wrapped service");
    let state = handle.borrow();
    // The fifth period reuses slot zero, so the first word is the new right sample.
    assert_eq!(state.dma_writes.last(), Some(&0));
    assert_eq!(
        state.dma_word(0),
        sample_to_duty(-1000, 2083).expect("right")
    );
}

#[test]
fn service_plays_silence_and_counts_an_underrun() {
    let (mut driver, handle, mut region) = running_driver();
    let report = driver.service(&mut region).expect("underrun service");
    assert_eq!(
        report,
        PeriodReport {
            frames: 0,
            underrun: true
        }
    );
    let header = RingHeader::read_from(&region).expect("header");
    assert_eq!(header.underruns, 1);
    let state = handle.borrow();
    assert_eq!(state.dma_word(0), 1041);
    assert_eq!(state.dma_word(4), 1041);
}

#[test]
fn service_leaves_a_stopped_stream_alone() {
    let (mut driver, handle, mut region) = running_driver();
    {
        let layout = layout();
        let mut producer = RingProducer::new(&mut region, layout).expect("producer");
        producer.stop();
    }
    let before = handle.borrow().dma_writes.len();
    assert_eq!(
        driver.service(&mut region).expect("stopped service"),
        PeriodReport::default()
    );
    assert_eq!(handle.borrow().dma_writes.len(), before);
}

#[test]
fn service_requires_the_running_state() {
    let (mut driver, _) = prepared_driver();
    let mut region = ring_region();
    assert_eq!(driver.service(&mut region), Err(DriverError::NotRunning));
}

#[test]
fn service_rejects_a_corrupt_or_mismatched_ring() {
    let (mut driver, _, mut region) = running_driver();
    region[0] = 0;
    assert_eq!(driver.service(&mut region), Err(DriverError::Layout));

    let other = RingLayout::new(REGION_LEN, format(), PERIOD_FRAMES / 2, PERIOD_COUNT * 2)
        .expect("other layout");
    let mut region = std::vec![0u8; REGION_LEN];
    other.write_header(&mut region, 0).expect("header");
    start_stream(&mut region);
    assert_eq!(driver.service(&mut region), Err(DriverError::Layout));
}
