// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared PCM ring between `audiod` and a sound driver.
//!
//! A region contains an 80-byte [`RingHeader`], padding to a 64-byte boundary, and one ring of
//! PCM frames:
//!
//! ```text
//! offset 0                      RingHeader (80 bytes)
//! offset 128 (64-byte aligned)  PCM frames: capacity_frames * frame_bytes
//! ```
//!
//! Indices count **frames**, not bytes: `write_index` is advanced by the producer (`audiod`) and
//! `read_index` by the consumer (the driver). The same layout is used for the client ring
//! attached with `SET_BUFFER`, where the client is the producer and `audiod` the consumer.
//!
//! [`RingProducer`] and [`RingConsumer`] are single-producer/single-consumer helpers over
//! `&mut [u8]`; see `README.md` for the acquire/release rules and position semantics.

use core::fmt;
use core::mem::{offset_of, size_of};
use core::sync::atomic::{Ordering, fence};

use crate::format::{Format, MAX_CHANNELS, MAX_RATE, MIN_RATE, SampleFormat};
use crate::stream::StreamState;
use crate::wire::{Reader, WireError, Writer};

/// Ring magic, ASCII `RAVENAUD`.
pub const RING_MAGIC: [u8; 8] = *b"RAVENAUD";
/// Ring layout version.
pub const RING_VERSION: u16 = 1;
/// Size of the ring header in bytes.
pub const HEADER_LEN: usize = 80;
/// Alignment of the PCM data area in bytes.
pub const DATA_ALIGN: usize = 64;
/// Largest ring capacity in frames (one million frames is ~21 s at 48 kHz).
pub const MAX_FRAMES: u32 = 1_048_576;
/// Largest number of periods in one ring.
pub const MAX_PERIODS: u16 = 256;
/// Largest accepted shared region in bytes.
pub const MAX_REGION: usize = 64 * 1024 * 1024;

/// Ring header at the start of the shared region.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingHeader {
    /// [`RING_MAGIC`].
    pub magic: [u8; 8],
    /// [`RING_VERSION`].
    pub version: u16,
    /// Size of this header in bytes.
    pub header_len: u16,
    /// Ring capacity in frames.
    pub capacity_frames: u32,
    /// Frames per period.
    pub period_frames: u32,
    /// Number of periods in the ring.
    pub period_count: u16,
    /// Bytes per frame.
    pub frame_bytes: u16,
    /// Sample rate in Hz.
    pub rate: u32,
    /// Channel count.
    pub channels: u8,
    /// [`SampleFormat`] wire code.
    pub sample_format: u8,
    /// [`StreamState`] wire code; written by `audiod`.
    pub state: u8,
    /// Reserved; must be zero.
    pub reserved: u8,
    /// Frames consumed by the driver; written by the driver.
    pub read_index: u64,
    /// Frames produced by `audiod`; written by `audiod`.
    pub write_index: u64,
    /// Read index captured at the last `START`; written by `audiod`.
    pub start_index: u64,
    /// Periods the driver could not fill; written by the driver.
    pub underruns: u32,
    /// Reserved; must be zero.
    pub reserved2: u32,
    /// Doorbell event id (S26 user event).
    pub doorbell: u64,
    /// Byte offset of the PCM data area.
    pub data_offset: u32,
    /// Length of the PCM data area in bytes.
    pub data_len: u32,
}

const _: () = assert!(size_of::<RingHeader>() == HEADER_LEN);
const _: () = assert!(offset_of!(RingHeader, capacity_frames) == 12);
const _: () = assert!(offset_of!(RingHeader, read_index) == 32);
const _: () = assert!(offset_of!(RingHeader, write_index) == 40);
const _: () = assert!(offset_of!(RingHeader, start_index) == 48);
const _: () = assert!(offset_of!(RingHeader, underruns) == 56);
const _: () = assert!(offset_of!(RingHeader, doorbell) == 64);
const _: () = assert!(offset_of!(RingHeader, data_offset) == 72);

const OFF_READ: usize = offset_of!(RingHeader, read_index);
const OFF_WRITE: usize = offset_of!(RingHeader, write_index);
const OFF_START: usize = offset_of!(RingHeader, start_index);
const OFF_UNDERRUNS: usize = offset_of!(RingHeader, underruns);
const OFF_STATE: usize = offset_of!(RingHeader, state);
const OFF_DOORBELL: usize = offset_of!(RingHeader, doorbell);

/// Why a ring layout or region was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutError {
    /// The rate or channel count is outside the documented bounds.
    BadFormat,
    /// The sample format is reserved (`S24LE`, `S32LE`, `F32LE`).
    UnsupportedFormat,
    /// A period or period count is zero.
    ZeroPeriod,
    /// The period count is above [`MAX_PERIODS`].
    TooManyPeriods,
    /// The capacity is above [`MAX_FRAMES`].
    TooManyFrames,
    /// The region cannot hold the header and the configured frames.
    RegionTooSmall,
    /// The region has bytes after the PCM area (the size must be exact).
    NotAligned,
    /// The header magic, version, fields or indices are inconsistent.
    BadHeader,
}

impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::BadFormat => "invalid sample rate or channel count",
            Self::UnsupportedFormat => "sample format not supported",
            Self::ZeroPeriod => "period size and count must be non-zero",
            Self::TooManyPeriods => "too many periods",
            Self::TooManyFrames => "too many frames",
            Self::RegionTooSmall => "shared region is too small",
            Self::NotAligned => "shared region size is not exact",
            Self::BadHeader => "bad ring header",
        })
    }
}

/// Why a ring operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RingError {
    /// The byte count is not a whole number of frames.
    PartialFrame,
    /// The caller buffer is smaller than one period.
    BufferTooSmall,
    /// The ring held fewer than one period when a period was requested.
    Underrun,
    /// The region no longer matches the validated layout.
    Layout,
}

impl fmt::Display for RingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::PartialFrame => "byte count is not a whole number of frames",
            Self::BufferTooSmall => "buffer is smaller than one period",
            Self::Underrun => "ring underrun",
            Self::Layout => "ring layout mismatch",
        })
    }
}

/// Validated geometry for one shared PCM region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingLayout {
    region_len: u32,
    capacity_frames: u32,
    period_frames: u32,
    period_count: u16,
    frame_bytes: u16,
    rate: u32,
    channels: u8,
    sample_format: u8,
    data_offset: u32,
    data_len: u32,
}

impl RingLayout {
    /// Compute the layout for a region of `region_len` bytes.
    ///
    /// `period_frames * period_count` is the capacity; the region must be exactly
    /// `align_up(HEADER_LEN, 64) + capacity * frame_bytes` bytes. `format.check()` must pass, so
    /// reserved formats cannot be configured.
    pub fn new(
        region_len: usize,
        format: Format,
        period_frames: u32,
        period_count: u16,
    ) -> Result<Self, LayoutError> {
        format.check_bounds().map_err(|_| LayoutError::BadFormat)?;
        if !format.is_supported() {
            return Err(LayoutError::UnsupportedFormat);
        }
        if period_frames == 0 || period_count == 0 {
            return Err(LayoutError::ZeroPeriod);
        }
        if period_count > MAX_PERIODS {
            return Err(LayoutError::TooManyPeriods);
        }
        let capacity_frames = period_frames
            .checked_mul(u32::from(period_count))
            .ok_or(LayoutError::TooManyFrames)?;
        if capacity_frames > MAX_FRAMES {
            return Err(LayoutError::TooManyFrames);
        }
        let frame_bytes = format.bytes_per_frame();
        if frame_bytes == 0 {
            return Err(LayoutError::BadFormat);
        }
        let data_len = capacity_frames
            .checked_mul(u32::from(frame_bytes))
            .ok_or(LayoutError::TooManyFrames)?;
        let data_offset = align_up(HEADER_LEN, DATA_ALIGN);
        let needed = data_offset
            .checked_add(data_len as usize)
            .ok_or(LayoutError::TooManyFrames)?;
        if needed > MAX_REGION {
            return Err(LayoutError::TooManyFrames);
        }
        if region_len < needed {
            return Err(LayoutError::RegionTooSmall);
        }
        if region_len > needed {
            return Err(LayoutError::NotAligned);
        }
        Ok(Self {
            region_len: region_len as u32,
            capacity_frames,
            period_frames,
            period_count,
            frame_bytes,
            rate: format.rate,
            channels: format.channels,
            sample_format: format.sample.code(),
            data_offset: data_offset as u32,
            data_len,
        })
    }

    /// Rebuild a layout from a validated header.
    pub fn from_header(header: &RingHeader, region_len: usize) -> Result<Self, LayoutError> {
        header.validate(region_len)?;
        Ok(Self {
            region_len: region_len as u32,
            capacity_frames: header.capacity_frames,
            period_frames: header.period_frames,
            period_count: header.period_count,
            frame_bytes: header.frame_bytes,
            rate: header.rate,
            channels: header.channels,
            sample_format: header.sample_format,
            data_offset: header.data_offset,
            data_len: header.data_len,
        })
    }

    /// Region length the layout was computed for.
    pub const fn region_len(&self) -> usize {
        self.region_len as usize
    }

    /// Ring capacity in frames.
    pub const fn capacity_frames(&self) -> u32 {
        self.capacity_frames
    }

    /// Frames per period.
    pub const fn period_frames(&self) -> u32 {
        self.period_frames
    }

    /// Number of periods.
    pub const fn period_count(&self) -> u16 {
        self.period_count
    }

    /// Bytes per frame.
    pub const fn frame_bytes(&self) -> u16 {
        self.frame_bytes
    }

    /// Stream format described by this layout.
    pub const fn format(&self) -> Format {
        Format {
            rate: self.rate,
            channels: self.channels,
            sample: match SampleFormat::from_code(self.sample_format) {
                Some(sample) => sample,
                // `new` and `from_header` only accept known codes.
                None => SampleFormat::S16Le,
            },
        }
    }

    /// Byte offset of the PCM data area.
    pub const fn data_offset(&self) -> usize {
        self.data_offset as usize
    }

    /// Length of the PCM data area in bytes.
    pub const fn data_len(&self) -> usize {
        self.data_len as usize
    }

    /// Byte offset of absolute `frame` inside the region, or `None` for an empty ring.
    pub const fn frame_offset(&self, frame: u64) -> Option<usize> {
        if self.capacity_frames == 0 {
            return None;
        }
        let slot = (frame % self.capacity_frames as u64) as usize;
        Some(self.data_offset as usize + slot * self.frame_bytes as usize)
    }

    /// Build the header for this layout without writing it.
    pub const fn header(&self, doorbell: u64) -> RingHeader {
        RingHeader {
            magic: RING_MAGIC,
            version: RING_VERSION,
            header_len: HEADER_LEN as u16,
            capacity_frames: self.capacity_frames,
            period_frames: self.period_frames,
            period_count: self.period_count,
            frame_bytes: self.frame_bytes,
            rate: self.rate,
            channels: self.channels,
            sample_format: self.sample_format,
            state: StreamState::Stopped.code(),
            reserved: 0,
            read_index: 0,
            write_index: 0,
            start_index: 0,
            underruns: 0,
            reserved2: 0,
            doorbell,
            data_offset: self.data_offset,
            data_len: self.data_len,
        }
    }

    /// Write the header into the start of `region`.
    pub fn write_header(&self, region: &mut [u8], doorbell: u64) -> Result<(), LayoutError> {
        if region.len() < self.region_len as usize {
            return Err(LayoutError::RegionTooSmall);
        }
        self.header(doorbell)
            .write_to(region)
            .map(|_| ())
            .map_err(|_| LayoutError::RegionTooSmall)
    }
}

impl RingHeader {
    /// Encode into `out`.
    pub fn write_to(&self, out: &mut [u8]) -> Result<usize, WireError> {
        let mut writer = Writer::new(out);
        writer.put_bytes(&self.magic)?;
        writer.put_u16(self.version)?;
        writer.put_u16(self.header_len)?;
        writer.put_u32(self.capacity_frames)?;
        writer.put_u32(self.period_frames)?;
        writer.put_u16(self.period_count)?;
        writer.put_u16(self.frame_bytes)?;
        writer.put_u32(self.rate)?;
        writer.put_u8(self.channels)?;
        writer.put_u8(self.sample_format)?;
        writer.put_u8(self.state)?;
        writer.put_u8(self.reserved)?;
        writer.put_u64(self.read_index)?;
        writer.put_u64(self.write_index)?;
        writer.put_u64(self.start_index)?;
        writer.put_u32(self.underruns)?;
        writer.put_u32(self.reserved2)?;
        writer.put_u64(self.doorbell)?;
        writer.put_u32(self.data_offset)?;
        writer.put_u32(self.data_len)?;
        Ok(writer.position())
    }

    /// Read and validate the header at the start of `region`.
    pub fn read_from(region: &[u8]) -> Result<Self, LayoutError> {
        let mut reader = Reader::new(region);
        let magic = reader
            .get_bytes(8)
            .map_err(|_| LayoutError::RegionTooSmall)?;
        if magic != RING_MAGIC {
            return Err(LayoutError::BadHeader);
        }
        let version = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let header_len = reader.get_u16().map_err(|_| LayoutError::BadHeader)?;
        let header = Self {
            magic: RING_MAGIC,
            version,
            header_len,
            capacity_frames: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            period_frames: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            period_count: reader.get_u16().map_err(|_| LayoutError::BadHeader)?,
            frame_bytes: reader.get_u16().map_err(|_| LayoutError::BadHeader)?,
            rate: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            channels: reader.get_u8().map_err(|_| LayoutError::BadHeader)?,
            sample_format: reader.get_u8().map_err(|_| LayoutError::BadHeader)?,
            state: reader.get_u8().map_err(|_| LayoutError::BadHeader)?,
            reserved: reader.get_u8().map_err(|_| LayoutError::BadHeader)?,
            read_index: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            write_index: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            start_index: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            underruns: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            reserved2: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            doorbell: reader.get_u64().map_err(|_| LayoutError::BadHeader)?,
            data_offset: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
            data_len: reader.get_u32().map_err(|_| LayoutError::BadHeader)?,
        };
        header.validate(region.len())?;
        Ok(header)
    }

    /// Validate every field against a region of `region_len` bytes.
    pub fn validate(&self, region_len: usize) -> Result<(), LayoutError> {
        if self.magic != RING_MAGIC
            || self.version != RING_VERSION
            || usize::from(self.header_len) != HEADER_LEN
            || self.reserved != 0
            || self.reserved2 != 0
        {
            return Err(LayoutError::BadHeader);
        }
        let sample = SampleFormat::from_code(self.sample_format).ok_or(LayoutError::BadHeader)?;
        if !sample.is_supported() {
            return Err(LayoutError::UnsupportedFormat);
        }
        let format = Format {
            rate: self.rate,
            channels: self.channels,
            sample,
        };
        if self.rate < MIN_RATE
            || self.rate > MAX_RATE
            || self.channels == 0
            || self.channels > MAX_CHANNELS
            || self.frame_bytes != format.bytes_per_frame()
        {
            return Err(LayoutError::BadFormat);
        }
        if self.period_frames == 0 || self.period_count == 0 {
            return Err(LayoutError::ZeroPeriod);
        }
        if self.period_count > MAX_PERIODS {
            return Err(LayoutError::TooManyPeriods);
        }
        let capacity = self
            .period_frames
            .checked_mul(u32::from(self.period_count))
            .ok_or(LayoutError::TooManyFrames)?;
        if capacity != self.capacity_frames || capacity > MAX_FRAMES {
            return Err(LayoutError::TooManyFrames);
        }
        let data_len = capacity
            .checked_mul(u32::from(self.frame_bytes))
            .ok_or(LayoutError::TooManyFrames)?;
        if data_len != self.data_len
            || self.data_offset < HEADER_LEN as u32
            || !self.data_offset.is_multiple_of(DATA_ALIGN as u32)
            || self.data_offset as usize + self.data_len as usize != region_len
        {
            return Err(LayoutError::BadHeader);
        }
        if StreamState::from_code(self.state).is_none() {
            return Err(LayoutError::BadHeader);
        }
        if self.read_index > self.write_index
            || self.write_index - self.read_index > u64::from(capacity)
            || self.start_index > self.read_index
        {
            return Err(LayoutError::BadHeader);
        }
        Ok(())
    }

    /// Stream format described by this header.
    pub const fn format(&self) -> Format {
        Format {
            rate: self.rate,
            channels: self.channels,
            sample: match SampleFormat::from_code(self.sample_format) {
                Some(sample) => sample,
                None => SampleFormat::S16Le,
            },
        }
    }

    /// Current playback state.
    pub const fn state(&self) -> StreamState {
        match StreamState::from_code(self.state) {
            Some(state) => state,
            None => StreamState::Stopped,
        }
    }
}

/// Producer half of one PCM ring.
pub struct RingProducer<'a> {
    region: &'a mut [u8],
    layout: RingLayout,
}

impl<'a> RingProducer<'a> {
    /// Attach a producer to `region`.
    pub fn new(region: &'a mut [u8], layout: RingLayout) -> Result<Self, LayoutError> {
        if region.len() < layout.region_len() {
            return Err(LayoutError::RegionTooSmall);
        }
        Ok(Self { region, layout })
    }

    /// Validated geometry.
    pub const fn layout(&self) -> RingLayout {
        self.layout
    }

    /// Ring capacity in frames.
    pub const fn capacity_frames(&self) -> u32 {
        self.layout.capacity_frames()
    }

    /// Frames per period.
    pub const fn period_frames(&self) -> u32 {
        self.layout.period_frames()
    }

    /// Bytes per frame.
    pub const fn frame_bytes(&self) -> u16 {
        self.layout.frame_bytes()
    }

    /// Current write index.
    pub fn write_index(&self) -> u64 {
        load_u64(self.region, OFF_WRITE)
    }

    /// Current read index.
    pub fn read_index(&self) -> u64 {
        load_u64(self.region, OFF_READ)
    }

    /// Frames waiting for the consumer.
    pub fn available_frames(&self) -> u32 {
        fence(Ordering::Acquire);
        self.write_index()
            .wrapping_sub(self.read_index())
            .min(u64::from(self.capacity_frames())) as u32
    }

    /// Free frames in the ring.
    pub fn free_frames(&self) -> u32 {
        self.capacity_frames() - self.available_frames()
    }

    /// Frames played since the last `START`.
    pub fn position_frames(&self) -> u64 {
        self.read_index()
            .saturating_sub(load_u64(self.region, OFF_START))
    }

    /// Periods completed, derived from the read index.
    pub fn periods_played(&self) -> u64 {
        self.read_index() / u64::from(self.layout.period_frames())
    }

    /// Periods written, derived from the write index.
    pub fn periods_written(&self) -> u64 {
        self.write_index() / u64::from(self.layout.period_frames())
    }

    /// Current playback state.
    pub fn state(&self) -> StreamState {
        match StreamState::from_code(load_u8(self.region, OFF_STATE)) {
            Some(state) => state,
            None => StreamState::Stopped,
        }
    }

    /// Current underrun count.
    pub fn underruns(&self) -> u32 {
        load_u32(self.region, OFF_UNDERRUNS)
    }

    /// Doorbell event id.
    pub fn doorbell(&self) -> u64 {
        load_u64(self.region, OFF_DOORBELL)
    }

    /// Set the playback state. `audiod` owns this field.
    pub fn set_state(&mut self, state: StreamState) {
        fence(Ordering::Release);
        store_u8(self.region, OFF_STATE, state.code());
    }

    /// Start or resume: snapshot the read index and enter `Running`.
    ///
    /// `position_frames` restarts at zero for the new start interval.
    pub fn start(&mut self) {
        let read = load_u64(self.region, OFF_READ);
        store_u64(self.region, OFF_START, read);
        fence(Ordering::Release);
        store_u8(self.region, OFF_STATE, StreamState::Running.code());
    }

    /// Stop, keeping buffered data. The position stops increasing.
    pub fn stop(&mut self) {
        self.set_state(StreamState::Stopped);
    }

    /// Play out the buffered data, then stop.
    pub fn drain(&mut self) {
        self.set_state(StreamState::Draining);
    }

    /// Write whole frames and return the frame count written.
    ///
    /// A short write is normal: at most [`Self::free_frames`] frames are accepted. A payload that
    /// is not a whole number of frames is rejected with [`RingError::PartialFrame`].
    pub fn write(&mut self, data: &[u8]) -> Result<usize, RingError> {
        let frame_bytes = usize::from(self.layout.frame_bytes());
        if frame_bytes == 0 || !data.len().is_multiple_of(frame_bytes) {
            return Err(RingError::PartialFrame);
        }
        let requested = data.len() / frame_bytes;
        let frames = requested.min(self.free_frames() as usize);
        if frames == 0 {
            return Ok(0);
        }
        let capacity = self.layout.capacity_frames() as usize;
        let index = self.write_index();
        let mut slot = (index % self.layout.capacity_frames() as u64) as usize;
        let mut source = 0usize;
        let mut remaining = frames;
        while remaining > 0 {
            let chunk = remaining.min(capacity - slot);
            let source_end = (source + chunk) * frame_bytes;
            let destination = self.layout.data_offset() + slot * frame_bytes;
            let destination_end = destination + chunk * frame_bytes;
            let Some(bytes) = self.region.get_mut(destination..destination_end) else {
                return Err(RingError::Layout);
            };
            bytes.copy_from_slice(&data[source * frame_bytes..source_end]);
            source += chunk;
            remaining -= chunk;
            slot = 0;
        }
        // Publish the frame data before the index that makes it visible.
        fence(Ordering::Release);
        store_u64(self.region, OFF_WRITE, index.wrapping_add(frames as u64));
        Ok(frames)
    }
}

/// Consumer half of one PCM ring.
pub struct RingConsumer<'a> {
    region: &'a mut [u8],
    layout: RingLayout,
}

impl<'a> RingConsumer<'a> {
    /// Attach a consumer to `region`.
    pub fn new(region: &'a mut [u8], layout: RingLayout) -> Result<Self, LayoutError> {
        if region.len() < layout.region_len() {
            return Err(LayoutError::RegionTooSmall);
        }
        Ok(Self { region, layout })
    }

    /// Validated geometry.
    pub const fn layout(&self) -> RingLayout {
        self.layout
    }

    /// Ring capacity in frames.
    pub const fn capacity_frames(&self) -> u32 {
        self.layout.capacity_frames()
    }

    /// Frames per period.
    pub const fn period_frames(&self) -> u32 {
        self.layout.period_frames()
    }

    /// Bytes per frame.
    pub const fn frame_bytes(&self) -> u16 {
        self.layout.frame_bytes()
    }

    /// Current read index.
    pub fn read_index(&self) -> u64 {
        load_u64(self.region, OFF_READ)
    }

    /// Frames waiting to be consumed.
    pub fn available_frames(&self) -> u32 {
        fence(Ordering::Acquire);
        let write = load_u64(self.region, OFF_WRITE);
        let read = load_u64(self.region, OFF_READ);
        write
            .wrapping_sub(read)
            .min(u64::from(self.capacity_frames())) as u32
    }

    /// Frames played since the last `START`.
    pub fn position_frames(&self) -> u64 {
        self.read_index()
            .saturating_sub(load_u64(self.region, OFF_START))
    }

    /// Periods completed, derived from the read index.
    pub fn periods_completed(&self) -> u64 {
        self.read_index() / u64::from(self.layout.period_frames())
    }

    /// Current playback state.
    pub fn state(&self) -> StreamState {
        match StreamState::from_code(load_u8(self.region, OFF_STATE)) {
            Some(state) => state,
            None => StreamState::Stopped,
        }
    }

    /// Current underrun count.
    pub fn underruns(&self) -> u32 {
        load_u32(self.region, OFF_UNDERRUNS)
    }

    /// Doorbell event id.
    pub fn doorbell(&self) -> u64 {
        load_u64(self.region, OFF_DOORBELL)
    }

    /// Read whole frames into `out` and return the frame count read.
    ///
    /// A short read is normal. `out` must be a whole number of frames.
    pub fn read(&mut self, out: &mut [u8]) -> Result<usize, RingError> {
        let frame_bytes = usize::from(self.layout.frame_bytes());
        if frame_bytes == 0 || !out.len().is_multiple_of(frame_bytes) {
            return Err(RingError::PartialFrame);
        }
        let requested = out.len() / frame_bytes;
        let frames = requested.min(self.available_frames() as usize);
        self.copy_out(out, frames)
    }

    /// Read exactly one period, or report and count an underrun.
    ///
    /// On [`RingError::Underrun`] nothing is consumed and the header's underrun counter grows by
    /// one; the caller plays silence for the period. `out` must hold at least one period.
    pub fn read_period(&mut self, out: &mut [u8]) -> Result<usize, RingError> {
        let frame_bytes = usize::from(self.layout.frame_bytes());
        if frame_bytes == 0 || !out.len().is_multiple_of(frame_bytes) {
            return Err(RingError::PartialFrame);
        }
        let period = self.layout.period_frames() as usize;
        if out.len() < period * frame_bytes {
            return Err(RingError::BufferTooSmall);
        }
        if self.available_frames() < self.layout.period_frames() {
            self.note_underrun();
            return Err(RingError::Underrun);
        }
        self.copy_out(out, period)
    }

    /// Journal one underrun. The driver owns the counter; `audiod` observes it.
    fn note_underrun(&mut self) {
        let count = load_u32(self.region, OFF_UNDERRUNS);
        fence(Ordering::Release);
        store_u32(self.region, OFF_UNDERRUNS, count.wrapping_add(1));
    }

    fn copy_out(&mut self, out: &mut [u8], frames: usize) -> Result<usize, RingError> {
        if frames == 0 {
            return Ok(0);
        }
        let frame_bytes = usize::from(self.layout.frame_bytes());
        let capacity = self.layout.capacity_frames() as usize;
        let index = self.read_index();
        let mut slot = (index % self.layout.capacity_frames() as u64) as usize;
        let mut destination = 0usize;
        let mut remaining = frames;
        while remaining > 0 {
            let chunk = remaining.min(capacity - slot);
            let source = self.layout.data_offset() + slot * frame_bytes;
            let source_end = source + chunk * frame_bytes;
            let Some(bytes) = self.region.get(source..source_end) else {
                return Err(RingError::Layout);
            };
            let destination_end = (destination + chunk) * frame_bytes;
            out[destination * frame_bytes..destination_end].copy_from_slice(bytes);
            destination += chunk;
            remaining -= chunk;
            slot = 0;
        }
        // Finish reading the frames before freeing their slots.
        fence(Ordering::Release);
        store_u64(self.region, OFF_READ, index.wrapping_add(frames as u64));
        Ok(frames)
    }
}

fn align_up(value: usize, alignment: usize) -> usize {
    value.div_ceil(alignment) * alignment
}

fn load_u8(region: &[u8], offset: usize) -> u8 {
    region.get(offset).copied().unwrap_or(0)
}

fn load_u32(region: &[u8], offset: usize) -> u32 {
    match region.get(offset..offset.saturating_add(4)) {
        Some(bytes) => u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]),
        None => 0,
    }
}

fn load_u64(region: &[u8], offset: usize) -> u64 {
    match region.get(offset..offset.saturating_add(8)) {
        Some(bytes) => u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]),
        None => 0,
    }
}

fn store_u8(region: &mut [u8], offset: usize, value: u8) {
    if let Some(byte) = region.get_mut(offset) {
        *byte = value;
    }
}

fn store_u32(region: &mut [u8], offset: usize, value: u32) {
    if let Some(bytes) = region.get_mut(offset..offset.saturating_add(4)) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}

fn store_u64(region: &mut [u8], offset: usize, value: u64) {
    if let Some(bytes) = region.get_mut(offset..offset.saturating_add(8)) {
        bytes.copy_from_slice(&value.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FORMAT: Format = Format {
        rate: 48_000,
        channels: 2,
        sample: SampleFormat::S16Le,
    };
    const PERIOD: u32 = 1024;
    const PERIODS: u16 = 4;
    const FRAME_BYTES: usize = 4;
    const CAPACITY: usize = PERIOD as usize * PERIODS as usize;
    const DATA_OFFSET: usize = 128;
    const REGION_LEN: usize = DATA_OFFSET + CAPACITY * FRAME_BYTES;

    fn layout() -> RingLayout {
        RingLayout::new(REGION_LEN, FORMAT, PERIOD, PERIODS).unwrap()
    }

    fn region() -> Vec<u8> {
        let mut region = vec![0u8; REGION_LEN];
        layout()
            .write_header(&mut region, 0x0102_0304_0506_0708)
            .unwrap();
        region
    }

    fn pcm(frames: usize, start: u32) -> Vec<u8> {
        let mut bytes = vec![0u8; frames * FRAME_BYTES];
        for (index, sample) in bytes.chunks_exact_mut(FRAME_BYTES).enumerate() {
            let value = start.wrapping_add(index as u32);
            sample.copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }

    fn assert_pcm(bytes: &[u8], start: u32) {
        for (index, sample) in bytes.chunks_exact(FRAME_BYTES).enumerate() {
            let value = start.wrapping_add(index as u32);
            assert_eq!(sample, value.to_le_bytes(), "frame {index}");
        }
    }

    #[test]
    fn header_layout_is_stable() {
        assert_eq!(size_of::<RingHeader>(), HEADER_LEN);
        assert_eq!(HEADER_LEN, 80);
        assert_eq!(offset_of!(RingHeader, read_index), 32);
        assert_eq!(offset_of!(RingHeader, doorbell), 64);
        assert_eq!(offset_of!(RingHeader, data_len), 76);
    }

    #[test]
    fn layout_computes_the_documented_geometry() {
        let layout = layout();
        assert_eq!(layout.region_len(), REGION_LEN);
        assert_eq!(layout.capacity_frames(), 4096);
        assert_eq!(layout.period_frames(), 1024);
        assert_eq!(layout.period_count(), 4);
        assert_eq!(layout.frame_bytes(), 4);
        assert_eq!(layout.data_offset(), 128);
        assert_eq!(layout.data_len(), 16_384);
        assert_eq!(layout.format(), FORMAT);
    }

    #[test]
    fn mono_layout_uses_two_bytes_per_frame() {
        let mono = Format {
            rate: 8_000,
            channels: 1,
            sample: SampleFormat::S16Le,
        };
        let region = 128 + 64 * 2;
        let layout = RingLayout::new(region, mono, 64, 1).unwrap();
        assert_eq!(layout.frame_bytes(), 2);
        assert_eq!(layout.data_len(), 128);
        assert_eq!(layout.period_count(), 1);
    }

    #[test]
    fn layout_rejects_reserved_formats() {
        for sample in [
            SampleFormat::S24Le,
            SampleFormat::S32Le,
            SampleFormat::F32Le,
        ] {
            let format = Format { sample, ..FORMAT };
            assert_eq!(
                RingLayout::new(REGION_LEN, format, PERIOD, PERIODS),
                Err(LayoutError::UnsupportedFormat),
                "{sample}"
            );
        }
    }

    #[test]
    fn layout_rejects_bad_rate_or_channels() {
        for rate in [0, 7_999, 192_001] {
            let format = Format { rate, ..FORMAT };
            assert_eq!(
                RingLayout::new(REGION_LEN, format, PERIOD, PERIODS),
                Err(LayoutError::BadFormat)
            );
        }
        for channels in [0, 9] {
            let format = Format { channels, ..FORMAT };
            assert_eq!(
                RingLayout::new(REGION_LEN, format, PERIOD, PERIODS),
                Err(LayoutError::BadFormat)
            );
        }
    }

    #[test]
    fn layout_rejects_zero_and_excessive_geometry() {
        assert_eq!(
            RingLayout::new(REGION_LEN, FORMAT, 0, PERIODS),
            Err(LayoutError::ZeroPeriod)
        );
        assert_eq!(
            RingLayout::new(REGION_LEN, FORMAT, PERIOD, 0),
            Err(LayoutError::ZeroPeriod)
        );
        assert_eq!(
            RingLayout::new(REGION_LEN, FORMAT, PERIOD, MAX_PERIODS + 1),
            Err(LayoutError::TooManyPeriods)
        );
        assert_eq!(
            RingLayout::new(REGION_LEN, FORMAT, MAX_FRAMES, 2),
            Err(LayoutError::TooManyFrames)
        );
    }

    #[test]
    fn layout_requires_the_exact_region_size() {
        assert_eq!(
            RingLayout::new(REGION_LEN - 4, FORMAT, PERIOD, PERIODS),
            Err(LayoutError::RegionTooSmall)
        );
        assert_eq!(
            RingLayout::new(REGION_LEN + 4, FORMAT, PERIOD, PERIODS),
            Err(LayoutError::NotAligned)
        );
    }

    #[test]
    fn frame_offsets_wrap_at_capacity() {
        let layout = layout();
        assert_eq!(layout.frame_offset(0), Some(128));
        assert_eq!(layout.frame_offset(1), Some(132));
        assert_eq!(layout.frame_offset(4095), Some(128 + 4095 * 4));
        assert_eq!(layout.frame_offset(4096), Some(128));
        assert_eq!(layout.frame_offset(u64::MAX), Some(128 + 4095 * 4));
    }

    #[test]
    fn header_golden_bytes() {
        let mut bytes = [0u8; HEADER_LEN];
        layout()
            .header(0x0102_0304_0506_0708)
            .write_to(&mut bytes)
            .unwrap();
        assert_eq!(
            &bytes[..],
            &[
                b'R', b'A', b'V', b'E', b'N', b'A', b'U', b'D', // magic
                0x01, 0x00, // version
                0x50, 0x00, // header length 80
                0x00, 0x10, 0x00, 0x00, // capacity 4096
                0x00, 0x04, 0x00, 0x00, // period 1024
                0x04, 0x00, // period count 4
                0x04, 0x00, // frame bytes 4
                0x80, 0xbb, 0x00, 0x00, // rate 48000
                0x02, // channels
                0x01, // S16LE
                0x00, // stopped
                0x00, // reserved
                0, 0, 0, 0, 0, 0, 0, 0, // read index
                0, 0, 0, 0, 0, 0, 0, 0, // write index
                0, 0, 0, 0, 0, 0, 0, 0, // start index
                0, 0, 0, 0, // underruns
                0, 0, 0, 0, // reserved
                0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01, // doorbell
                0x80, 0x00, 0x00, 0x00, // data offset 128
                0x00, 0x40, 0x00, 0x00, // data length 16384
            ]
        );
    }

    #[test]
    fn header_roundtrips_and_rebuilds_the_layout() {
        let mut region = region();
        let header = RingHeader::read_from(&region).unwrap();
        assert_eq!(header.magic, RING_MAGIC);
        assert_eq!(header.version, RING_VERSION);
        assert_eq!(header.format(), FORMAT);
        assert_eq!(header.state(), StreamState::Stopped);
        assert_eq!(RingLayout::from_header(&header, REGION_LEN), Ok(layout()));

        region[0] = b'X';
        assert_eq!(RingHeader::read_from(&region), Err(LayoutError::BadHeader));
    }

    #[test]
    fn header_rejects_bad_fields_and_indices() {
        let region = region();

        let mut header = RingHeader::read_from(&region).unwrap();
        header.sample_format = 9;
        let mut bytes = vec![0u8; REGION_LEN];
        header.write_to(&mut bytes).unwrap();
        assert_eq!(RingHeader::read_from(&bytes), Err(LayoutError::BadHeader));

        let mut header = RingHeader::read_from(&region).unwrap();
        header.capacity_frames += 1;
        header.write_to(&mut bytes).unwrap();
        assert_eq!(
            RingHeader::read_from(&bytes),
            Err(LayoutError::TooManyFrames)
        );

        let mut header = RingHeader::read_from(&region).unwrap();
        header.reserved = 1;
        header.write_to(&mut bytes).unwrap();
        assert_eq!(RingHeader::read_from(&bytes), Err(LayoutError::BadHeader));

        let mut header = RingHeader::read_from(&region).unwrap();
        header.read_index = 10;
        header.write_index = 5;
        header.write_to(&mut bytes).unwrap();
        assert_eq!(RingHeader::read_from(&bytes), Err(LayoutError::BadHeader));

        let mut header = RingHeader::read_from(&region).unwrap();
        header.start_index = 1;
        header.write_to(&mut bytes).unwrap();
        assert_eq!(RingHeader::read_from(&bytes), Err(LayoutError::BadHeader));
    }

    #[test]
    fn short_region_is_rejected_by_both_helpers() {
        let mut short = vec![0u8; REGION_LEN - 1];
        assert_eq!(
            RingProducer::new(&mut short, layout()).map(|_| ()),
            Err(LayoutError::RegionTooSmall)
        );
        assert_eq!(
            RingConsumer::new(&mut short, layout()).map(|_| ()),
            Err(LayoutError::RegionTooSmall)
        );
    }

    #[test]
    fn write_and_read_one_period() {
        let mut region = region();
        let data = pcm(PERIOD as usize, 0);
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.free_frames(), 4096);
            assert_eq!(producer.write(&data), Ok(PERIOD as usize));
            assert_eq!(producer.free_frames(), 4096 - PERIOD);
            assert_eq!(producer.available_frames(), PERIOD);
            assert_eq!(producer.periods_written(), 1);
        }
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.available_frames(), PERIOD);
            assert_eq!(consumer.read(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(consumer.available_frames(), 0);
            assert_eq!(consumer.periods_completed(), 1);
        }
        assert_pcm(&buffer, 0);
    }

    #[test]
    fn partial_frames_are_rejected() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.write(&[0; 3]), Err(RingError::PartialFrame));
        }
        let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
        assert_eq!(consumer.read(&mut [0; 3]), Err(RingError::PartialFrame));
        assert_eq!(
            consumer.read_period(&mut [0; 6]),
            Err(RingError::PartialFrame)
        );
        assert_eq!(
            consumer.read_period(&mut vec![0; PERIOD as usize * FRAME_BYTES / 2]),
            Err(RingError::BufferTooSmall)
        );
    }

    #[test]
    fn full_ring_reports_a_short_write() {
        let mut region = region();
        let four_periods = pcm(CAPACITY, 0);
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.write(&four_periods), Ok(CAPACITY));
            assert_eq!(producer.free_frames(), 0);
            assert_eq!(producer.write(&pcm(4, 0)), Ok(0));
        }
        let five_periods = pcm(CAPACITY + PERIOD as usize, 0);
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            // A fresh producer sees the same full ring.
            assert_eq!(producer.write(&five_periods), Ok(0));
        }
    }

    #[test]
    fn ring_wraps_through_capacity() {
        let mut region = region();
        // Five periods on a four-period ring: write_index and read_index wrap the slots.
        for round in 0..5u32 {
            let data = pcm(PERIOD as usize, round * PERIOD);
            {
                let mut producer = RingProducer::new(&mut region, layout()).unwrap();
                assert_eq!(producer.write(&data), Ok(PERIOD as usize));
            }
            let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
            {
                let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
                assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            }
            assert_pcm(&buffer, round * PERIOD);
        }
    }

    #[test]
    fn underrun_is_counted_and_data_is_kept() {
        let mut region = region();
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.read_period(&mut buffer), Err(RingError::Underrun));
            assert_eq!(consumer.underruns(), 1);
            assert_eq!(consumer.periods_completed(), 0);
        }
        // A partial period is not enough.
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(
                producer.write(&pcm(PERIOD as usize / 2, 0)),
                Ok(PERIOD as usize / 2)
            );
        }
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.read_period(&mut buffer), Err(RingError::Underrun));
            assert_eq!(consumer.underruns(), 2);
            assert_eq!(consumer.available_frames(), PERIOD / 2);
        }
        // Completing the period succeeds and does not touch the counter.
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(
                producer.write(&pcm(PERIOD as usize / 2, PERIOD / 2)),
                Ok(PERIOD as usize / 2)
            );
        }
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(consumer.underruns(), 2);
            assert_eq!(consumer.periods_completed(), 1);
        }
        assert_pcm(&buffer, 0);
    }

    #[test]
    fn period_accounting_tracks_reads_and_writes() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.write(&pcm(2 * PERIOD as usize, 0)), Ok(2048));
            assert_eq!(producer.periods_written(), 2);
            assert_eq!(producer.periods_played(), 0);
        }
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.periods_completed(), 0);
            assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(consumer.periods_completed(), 1);
            assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(consumer.periods_completed(), 2);
        }
    }

    #[test]
    fn position_starts_at_start_and_restarts_on_the_next_start() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.write(&pcm(2 * PERIOD as usize, 0)), Ok(2048));
            producer.start();
            assert_eq!(producer.state(), StreamState::Running);
            assert_eq!(producer.position_frames(), 0);
        }
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.position_frames(), 0);
            assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(consumer.position_frames(), u64::from(PERIOD));
        }
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.position_frames(), u64::from(PERIOD));
            producer.stop();
            assert_eq!(producer.state(), StreamState::Stopped);
        }
        {
            let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
            assert_eq!(consumer.read_period(&mut buffer), Ok(PERIOD as usize));
            assert_eq!(
                consumer.position_frames(),
                u64::from(2 * PERIOD),
                "position keeps counting after stop"
            );
        }
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            producer.start();
            assert_eq!(producer.position_frames(), 0, "new start interval");
        }
    }

    #[test]
    fn all_states_and_doorbell_are_visible() {
        let mut region = region();
        {
            let mut producer = RingProducer::new(&mut region, layout()).unwrap();
            assert_eq!(producer.doorbell(), 0x0102_0304_0506_0708);
            assert_eq!(producer.state(), StreamState::Stopped);
            producer.drain();
            assert_eq!(producer.state(), StreamState::Draining);
            producer.set_state(StreamState::Underrun);
            assert_eq!(producer.state(), StreamState::Underrun);
        }
        let consumer = RingConsumer::new(&mut region, layout()).unwrap();
        assert_eq!(consumer.doorbell(), 0x0102_0304_0506_0708);
        assert_eq!(consumer.state(), StreamState::Underrun);
    }

    #[test]
    fn header_state_codes_are_accepted() {
        let mut region = region();
        for state in [
            StreamState::Stopped,
            StreamState::Running,
            StreamState::Draining,
            StreamState::Underrun,
        ] {
            {
                let mut producer = RingProducer::new(&mut region, layout()).unwrap();
                producer.set_state(state);
            }
            let header = RingHeader::read_from(&region).unwrap();
            assert_eq!(header.state(), state);
        }
    }

    #[test]
    fn random_chunks_survive_a_wrapping_ring() {
        let mut region = region();
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut written = 0u32;
        let mut read = 0u32;
        let mut buffer = vec![0u8; PERIOD as usize * FRAME_BYTES];
        while read < 20 * PERIOD {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let frames = (state % u64::from(PERIOD / 8) + 1) as usize;
            let data = pcm(frames, written);
            let produced = {
                let mut producer = RingProducer::new(&mut region, layout()).unwrap();
                producer.write(&data).unwrap()
            };
            written += produced as u32;
            let consumed = {
                let mut consumer = RingConsumer::new(&mut region, layout()).unwrap();
                consumer
                    .read(&mut buffer[..produced * FRAME_BYTES])
                    .unwrap()
            };
            assert_eq!(consumed, produced);
            read += consumed as u32;
            assert_pcm(
                &buffer[..consumed * FRAME_BYTES],
                read.saturating_sub(consumed as u32),
            );
        }
        assert!(read >= 20 * PERIOD);
    }
}
