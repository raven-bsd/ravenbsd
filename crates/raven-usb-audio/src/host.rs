// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The UAC1 device model over the xHCI rings.
//!
//! The host side of the streaming and control paths. `queue` builds a Normal TRB for the next
//! isochronous packet on the R4 [`TransferRing`](raven_rp1_xhci::TransferRing); `queue_pcm`
//! consumes the same packet from a [`PcmSource`] (the audiod ring is one implementation) so the
//! driver can copy those bytes to the DMA buffer. `control` queues the xHCI Setup/Data/Status TRB
//! sequence on a second transfer ring and applies the UAC1 request to the feature unit; the
//! device's answer is returned as `Response`, and `control_complete` reclaims the transfer.
//!
//! This is a model: it never programs a controller, and the DMA addresses are caller data.

use core::fmt;

use raven_rp1_xhci::{ControlDirection, ControlTransfer, EventRing, RingError, TransferRing, Trb};

use crate::control::{FeatureUnitState, Response, VolumeLimits};
use crate::pcm::PcmSource;
use crate::stream::{HIGH_SPEED_MICROFRAMES, IsoStream, Packet, PcmFormat, Schedule, StreamError};
use crate::uac1::Uac1Config;
use crate::usb::{Direction, SetupPacket};

/// Largest packet payload the model copies.
pub const MAX_PACKET_BYTES: usize = 1024;

/// Why the audio host cannot advance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostError {
    /// The stream has not been opened.
    NotOpen,
    /// The alternate setting is not the streaming one.
    BadAlternate(u8),
    /// The sample rate is not in the format's list.
    UnsupportedRate(u32),
    /// The stream schedule rejected the format or a packet.
    Stream(StreamError),
    /// The xHCI transfer ring is full.
    RingFull,
    /// The endpoint has no device context index.
    BadEndpoint,
    /// The ring construction failed.
    Ring(RingError),
    /// The PCM source's frame size differs from the stream format.
    SourceFormat,
    /// The PCM source has fewer bytes than the next packet needs.
    SourceUnderrun,
    /// A control transfer is already in flight.
    ControlBusy,
    /// No control transfer is in flight.
    NoControl,
    /// The control transfer parameters are inconsistent.
    BadControl,
}

impl fmt::Display for HostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotOpen => f.write_str("stream is not open"),
            Self::BadAlternate(alternate) => {
                write!(f, "alternate setting {alternate} is not streaming")
            }
            Self::UnsupportedRate(rate) => write!(f, "sample rate {rate} is not supported"),
            Self::Stream(error) => write!(f, "stream: {error}"),
            Self::RingFull => f.write_str("transfer ring is full"),
            Self::BadEndpoint => f.write_str("endpoint has no device context index"),
            Self::Ring(error) => write!(f, "ring: {error}"),
            Self::SourceFormat => f.write_str("PCM source frame size differs from the stream"),
            Self::SourceUnderrun => f.write_str("PCM source underrun"),
            Self::ControlBusy => f.write_str("a control transfer is already in flight"),
            Self::NoControl => f.write_str("no control transfer is in flight"),
            Self::BadControl => f.write_str("inconsistent control transfer"),
        }
    }
}

impl core::error::Error for HostError {}

impl From<StreamError> for HostError {
    fn from(error: StreamError) -> Self {
        Self::Stream(error)
    }
}

impl From<RingError> for HostError {
    fn from(error: RingError) -> Self {
        Self::Ring(error)
    }
}

/// One completed isochronous transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Completion {
    /// Transfer-ring slot.
    pub index: u16,
    /// The completed TRB.
    pub trb: Trb,
    /// The device's transfer event, when it was queued.
    pub event: Option<Trb>,
}

/// One completed control transfer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ControlCompletion {
    /// The Status Stage ring slot.
    pub index: u16,
    /// The Status Stage TRB.
    pub trb: Trb,
    /// The device's transfer event, when it was queued.
    pub event: Option<Trb>,
}

/// The streaming device model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioHost {
    config: Uac1Config,
    stream: Option<IsoStream>,
    ring: TransferRing,
    events: EventRing,
    control_ring: TransferRing,
    controls: FeatureUnitState,
    control_busy: bool,
    control_count: u8,
    pcm: [u8; MAX_PACKET_BYTES],
    pcm_len: u16,
    slot: u8,
    dci: u8,
    pending: u16,
}

impl AudioHost {
    /// Build a host for `config` with the given stream/event ring sizes and the model's default
    /// volume range.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Ring`] when a ring size is invalid.
    pub fn new(
        config: Uac1Config,
        ring_entries: u16,
        event_entries: u16,
    ) -> Result<Self, HostError> {
        Self::with_controls(
            config,
            ring_entries,
            event_entries,
            ring_entries,
            VolumeLimits::MODEL_DEFAULT,
        )
    }

    /// Build a host with an explicit control-ring size and volume range.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Ring`] when a ring size is invalid.
    pub fn with_controls(
        config: Uac1Config,
        ring_entries: u16,
        event_entries: u16,
        control_entries: u16,
        limits: VolumeLimits,
    ) -> Result<Self, HostError> {
        let dci = config.streaming.data.address.dci();
        let controls = FeatureUnitState::new(config.feature.id, config.control.number, limits);
        Ok(Self {
            config,
            stream: None,
            ring: TransferRing::new(ring_entries).map_err(HostError::Ring)?,
            events: EventRing::new(event_entries).map_err(HostError::Ring)?,
            control_ring: TransferRing::new(control_entries).map_err(HostError::Ring)?,
            controls,
            control_busy: false,
            control_count: 0,
            pcm: [0; MAX_PACKET_BYTES],
            pcm_len: 0,
            slot: 1,
            dci,
            pending: 0,
        })
    }

    /// The descriptor set.
    #[must_use]
    pub const fn config(&self) -> Uac1Config {
        self.config
    }

    /// The feature-unit state.
    #[must_use]
    pub const fn controls(&self) -> FeatureUnitState {
        self.controls
    }

    /// Replace the volume range.
    pub fn set_volume_limits(&mut self, limits: VolumeLimits) {
        self.controls.limits = limits;
        self.controls.volume = limits.clamp(self.controls.volume);
    }

    /// The PCM format of the open stream.
    #[must_use]
    pub fn format(&self) -> Option<PcmFormat> {
        self.stream.map(|stream| stream.schedule().format())
    }

    /// The device context index of the data endpoint.
    #[must_use]
    pub const fn dci(&self) -> u8 {
        self.dci
    }

    /// Transfers queued but not completed.
    #[must_use]
    pub const fn pending(&self) -> u16 {
        self.pending
    }

    /// True while a control transfer is in flight.
    #[must_use]
    pub const fn control_busy(&self) -> bool {
        self.control_busy
    }

    /// The last PCM packet copied by `queue_pcm`.
    #[must_use]
    pub fn last_packet(&self) -> &[u8] {
        self.pcm
            .get(..usize::from(self.pcm_len))
            .unwrap_or_default()
    }

    /// Open the operational alternate setting at `rate_hz`.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::BadAlternate`], [`HostError::UnsupportedRate`] or the schedule error.
    pub fn open(&mut self, alternate: u8, rate_hz: u32) -> Result<(), HostError> {
        if alternate != self.config.streaming.interface.alternate {
            return Err(HostError::BadAlternate(alternate));
        }
        if !self.config.streaming.format.supports_rate(rate_hz) {
            return Err(HostError::UnsupportedRate(rate_hz));
        }
        let format = PcmFormat {
            channels: self.config.streaming.format.channels,
            subframe_bytes: self.config.streaming.format.subframe_bytes,
            bit_resolution: self.config.streaming.format.bit_resolution,
            rate_hz,
        };
        let schedule = Schedule::new(
            format,
            HIGH_SPEED_MICROFRAMES,
            self.config.streaming.data.max_packet_size(),
        )?;
        let mut stream = IsoStream::new(schedule);
        stream.start();
        self.stream = Some(stream);
        self.pending = 0;
        Ok(())
    }

    /// Close the stream.
    pub fn close(&mut self) {
        self.stream = None;
        self.pending = 0;
    }

    /// The next packet the stream would send, when open.
    #[must_use]
    pub fn next_packet(&self) -> Option<Packet> {
        let stream = self.stream?;
        stream.schedule().packet(stream.next_index()).ok()
    }

    /// The feedback value of the open stream.
    #[must_use]
    pub fn feedback(&self) -> Option<u32> {
        self.stream?
            .schedule()
            .feedback()
            .ok()
            .map(|feedback| feedback.raw())
    }

    /// Queue the next isochronous packet for `buffer`.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::NotOpen`], [`HostError::Stream`], [`HostError::BadEndpoint`] or
    /// [`HostError::RingFull`].
    pub fn queue(&mut self, buffer: u64) -> Result<u16, HostError> {
        let packet_bytes = {
            let stream = self.stream.as_ref().ok_or(HostError::NotOpen)?;
            stream.schedule().packet(stream.next_index())?.bytes
        };
        let length = u16::try_from(packet_bytes).map_err(|_| {
            HostError::Stream(StreamError::PacketTooLarge {
                bytes: packet_bytes,
                max: self.config.streaming.data.max_packet_size(),
            })
        })?;
        let trb = Trb::normal(buffer, length, true)
            .with_endpoint(self.dci)
            .ok_or(HostError::BadEndpoint)?;
        let index = self.ring.push(trb).map_err(|error| match error {
            RingError::Full => HostError::RingFull,
            other => HostError::Ring(other),
        })?;
        if let Some(stream) = self.stream.as_mut() {
            stream.next_packet()?;
        }
        self.pending = self.pending.saturating_add(1);
        Ok(index)
    }

    /// Queue the next packet, consuming its bytes from `source`.
    ///
    /// The bytes are copied into the host's scratch packet (`last_packet`); the driver copies the
    /// same bytes into the DMA buffer at `buffer` before ringing the doorbell. A short source
    /// leaves the stream and the ring untouched and returns [`HostError::SourceUnderrun`].
    ///
    /// # Errors
    ///
    /// Returns [`HostError::NotOpen`], [`HostError::SourceFormat`],
    /// [`HostError::SourceUnderrun`], [`HostError::Stream`] or [`HostError::RingFull`].
    pub fn queue_pcm(&mut self, buffer: u64, source: &mut dyn PcmSource) -> Result<u16, HostError> {
        let (needed, frame_bytes) = {
            let stream = self.stream.as_ref().ok_or(HostError::NotOpen)?;
            let schedule = stream.schedule();
            let packet = schedule.packet(stream.next_index())?;
            (
                usize::try_from(packet.bytes).unwrap_or(usize::MAX),
                schedule.format().frame_bytes(),
            )
        };
        if u16::try_from(frame_bytes).ok() != Some(source.frame_bytes()) {
            return Err(HostError::SourceFormat);
        }
        if needed == 0 || needed > MAX_PACKET_BYTES {
            return Err(HostError::Stream(StreamError::PacketTooLarge {
                bytes: u32::try_from(needed).unwrap_or(u32::MAX),
                max: self.config.streaming.data.max_packet_size(),
            }));
        }
        let available = usize::try_from(source.available_frames())
            .unwrap_or(usize::MAX)
            .saturating_mul(usize::from(source.frame_bytes()));
        if available < needed {
            return Err(HostError::SourceUnderrun);
        }
        let buffer_slot = self
            .pcm
            .get_mut(..needed)
            .ok_or(HostError::SourceUnderrun)?;
        if source.read(buffer_slot) != needed {
            return Err(HostError::SourceUnderrun);
        }
        self.pcm_len = u16::try_from(needed).unwrap_or(0);
        self.queue(buffer)
    }

    /// The device completed the oldest transfer; queue a Transfer Event for it.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::NotOpen`] or the event-ring error when the ring is full.
    pub fn device_complete(&mut self, completion_code: u8) -> Result<(), HostError> {
        if self.stream.is_none() {
            return Err(HostError::NotOpen);
        }
        let pointer = 0x1000u64.saturating_add(u64::from(self.pending) * 0x10);
        let event = Trb::transfer_event(self.slot, self.dci, completion_code, pointer)
            .ok_or(HostError::BadEndpoint)?;
        self.events.push_event(event).map_err(|error| match error {
            RingError::Full => HostError::RingFull,
            other => HostError::Ring(other),
        })?;
        Ok(())
    }

    /// Reclaim the oldest completed transfer and its event.
    #[must_use]
    pub fn complete(&mut self) -> Option<Completion> {
        let index = self.ring.complete()?;
        let trb = *self.ring.slot(index)?;
        let event = self.events.pop().map(|(_, event)| event);
        self.pending = self.pending.saturating_sub(1);
        Some(Completion { index, trb, event })
    }

    /// Queue the xHCI Setup/Data/Status sequence for a UAC1 request and apply it to the feature
    /// unit.
    ///
    /// `data` is the host-to-device payload for a `SET_*`; it is ignored for a `GET_*`. The
    /// returned [`Response`] is the device model's answer; the actual completion arrives through
    /// [`AudioHost::control_complete`].
    ///
    /// # Errors
    ///
    /// Returns [`HostError::ControlBusy`], [`HostError::BadControl`] or [`HostError::RingFull`].
    pub fn control(
        &mut self,
        setup: SetupPacket,
        data: &[u8],
        buffer: u64,
    ) -> Result<Response, HostError> {
        if self.control_busy {
            return Err(HostError::ControlBusy);
        }
        let direction = if setup.length == 0 {
            ControlDirection::NoData
        } else if setup.direction() == Direction::In {
            ControlDirection::In
        } else {
            ControlDirection::Out
        };
        let transfer = ControlTransfer::new(setup.to_bytes(), buffer, setup.length, direction)
            .ok_or(HostError::BadControl)?;
        let (trbs, count) = transfer.trbs();
        if self.control_ring.free_slots() < u16::from(count) {
            return Err(HostError::RingFull);
        }
        for trb in trbs.iter().take(usize::from(count)) {
            self.control_ring.push(*trb).map_err(|error| match error {
                RingError::Full => HostError::RingFull,
                other => HostError::Ring(other),
            })?;
        }
        self.control_busy = true;
        self.control_count = count;
        Ok(self.controls.handle(setup, data))
    }

    /// The device completed the control transfer; queue its Transfer Event (endpoint 0).
    ///
    /// # Errors
    ///
    /// Returns [`HostError::NoControl`] or the event-ring error.
    pub fn control_device_complete(&mut self, completion_code: u8) -> Result<(), HostError> {
        if !self.control_busy {
            return Err(HostError::NoControl);
        }
        let event = Trb::transfer_event(self.slot, 1, completion_code, 0x3000)
            .ok_or(HostError::BadEndpoint)?;
        self.events.push_event(event).map_err(|error| match error {
            RingError::Full => HostError::RingFull,
            other => HostError::Ring(other),
        })?;
        Ok(())
    }

    /// Reclaim the completed control transfer and its event.
    #[must_use]
    pub fn control_complete(&mut self) -> Option<ControlCompletion> {
        if !self.control_busy {
            return None;
        }
        let count = self.control_count;
        let mut status = None;
        for _ in 0..count {
            status = Some(self.control_ring.complete()?);
        }
        let index = status?;
        let trb = *self.control_ring.slot(index)?;
        let event = self.events.pop().map(|(_, event)| event);
        self.control_busy = false;
        Some(ControlCompletion { index, trb, event })
    }

    /// The event ring, for the device side of a test or a model.
    pub fn event_ring(&mut self) -> &mut EventRing {
        &mut self.events
    }
}

#[cfg(test)]
#[path = "host_ut.rs"]
mod tests;
