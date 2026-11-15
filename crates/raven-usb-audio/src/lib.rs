// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable USB Audio Class 1.0 model.
//!
//! The Pi 5 sound path is a USB DAC on the RP1 xHCI controller. This crate carries the parts a
//! host test can prove without a board:
//!
//! * [`usb`] - the standard USB device/configuration/interface/endpoint descriptors and the
//!   control setup packet;
//! * [`uac1`] - the UAC1 AudioControl and AudioStreaming descriptors, the terminal/feature-unit
//!   entity graph and the format type I record;
//! * [`control`] - the UAC1 class requests for mute and volume on a feature unit;
//! * [`stream`] - PCM format, the isochronous packet cadence, the asynchronous feedback value and
//!   the packet schedule;
//! * [`host`] - the device model over the R4 xHCI transfer/event rings.
//!
//! **No hardware claim.** The descriptor and class-request shapes are the public USB 2.0 and USB
//! Audio Class 1.0 definitions; the device values, the real feedback behaviour and every board
//! result are UNVERIFIED until a Pi 5 log exists. The `audiod` integration is a follow-up; this
//! crate does not edit or import the server.
//!
//! The crate is `#![forbid(unsafe_code)]` and `no_std` outside tests.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

pub mod control;
pub mod host;
pub mod stream;
pub mod uac1;
pub mod usb;

#[cfg(test)]
mod test_config;

pub use control::{
    FEATURE_MUTE, FEATURE_VOLUME, FeatureUnitState, GET_CUR, GET_MAX, GET_MIN, GET_RES, Response,
    SET_CUR, SET_MAX, SET_MIN, SET_RES, VolumeLimits,
};
pub use host::{AudioHost, Completion, HostError};
pub use stream::{
    FULL_SPEED_FRAMES, Feedback, HIGH_SPEED_MICROFRAMES, IsoStream, MAX_CHANNELS, MAX_RATE,
    MIN_RATE, Packet, PcmFormat, Schedule, StreamError,
};
pub use uac1::{
    AcHeader, AsInterface, FeatureUnit, FormatTypeI, InputTerminal, OutputTerminal,
    StreamingInterface, Uac1Config, Uac1Error,
};
pub use usb::{
    ConfigDescriptor, DescriptorError, DeviceDescriptor, Direction, EndpointAddress,
    EndpointDescriptor, InterfaceDescriptor, SetupPacket, SyncType, TransferType, UsageType,
};

#[cfg(test)]
#[path = "lib_ut.rs"]
mod tests;
