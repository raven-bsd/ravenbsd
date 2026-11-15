// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The UAC1 device model over the xHCI rings.
//!
//! The host side of the streaming path: the configuration is decoded, the operational alternate
//! setting is opened at a supported rate, and each `queue` builds a Normal TRB for the next
//! isochronous packet on the R4 [`TransferRing`](raven_rp1_xhci::TransferRing). Completion
//! events are read from the [`EventRing`](raven_rp1_xhci::EventRing). This is a model: it never
//! programs a controller, and the DMA addresses are caller data.

use core::fmt;

use raven_rp1_xhci::{EventRing, RingError, TransferRing, Trb};

use crate::stream::{HIGH_SPEED_MICROFRAMES, IsoStream, Packet, PcmFormat, Schedule, StreamError};
use crate::uac1::Uac1Config;

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

/// The streaming device model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioHost {
    config: Uac1Config,
    stream: Option<IsoStream>,
    ring: TransferRing,
    events: EventRing,
    slot: u8,
    dci: u8,
    pending: u16,
}

impl AudioHost {
    /// Build a host for `config` with the given ring sizes.
    ///
    /// # Errors
    ///
    /// Returns [`HostError::Ring`] when a ring size is invalid and rejects an invalid descriptor
    /// set through the caller's earlier parse.
    pub fn new(
        config: Uac1Config,
        ring_entries: u16,
        event_entries: u16,
    ) -> Result<Self, HostError> {
        let dci = config.streaming.data.address.dci();
        Ok(Self {
            config,
            stream: None,
            ring: TransferRing::new(ring_entries).map_err(HostError::Ring)?,
            events: EventRing::new(event_entries).map_err(HostError::Ring)?,
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

    /// The event ring, for the device side of a test or a model.
    pub fn event_ring(&mut self) -> &mut EventRing {
        &mut self.events
    }
}

#[cfg(test)]
#[path = "host_ut.rs"]
mod tests;
