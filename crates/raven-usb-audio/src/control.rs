// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! UAC1 feature-unit control requests.
//!
//! The class requests are the USB Audio Class 1.0 definitions: `SET_CUR`/`GET_CUR` and the
//! range requests on a feature unit's mute and volume controls. Volume is a signed 16-bit value in
//! 1/256 dB units; the valid range is device data supplied by the caller. A request that the
//! model cannot serve is `Unsupported`; a value outside a control's range is `Stalled`, the USB
//! way of refusing it.

use crate::usb::{Direction, SetupPacket};

/// `SET_CUR`.
pub const SET_CUR: u8 = 0x01;
/// `GET_CUR`.
pub const GET_CUR: u8 = 0x81;
/// `SET_MIN`.
pub const SET_MIN: u8 = 0x02;
/// `GET_MIN`.
pub const GET_MIN: u8 = 0x82;
/// `SET_MAX`.
pub const SET_MAX: u8 = 0x03;
/// `GET_MAX`.
pub const GET_MAX: u8 = 0x83;
/// `SET_RES`.
pub const SET_RES: u8 = 0x04;
/// `GET_RES`.
pub const GET_RES: u8 = 0x84;

/// Feature-unit control selector: mute.
pub const FEATURE_MUTE: u8 = 0x01;
/// Feature-unit control selector: volume.
pub const FEATURE_VOLUME: u8 = 0x02;

/// The volume range, in 1/256 dB units.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeLimits {
    /// Minimum volume.
    pub min: i16,
    /// Maximum volume.
    pub max: i16,
    /// Resolution.
    pub res: i16,
}

impl VolumeLimits {
    /// The model's default range: -64 dB to 0 dB in 0.125 dB steps.
    ///
    /// This is a model default, not a device claim; the real range comes from the feature unit's
    /// GET_MIN/GET_MAX/GET_RES requests.
    pub const MODEL_DEFAULT: Self = Self {
        min: -0x4000,
        max: 0,
        res: 0x20,
    };

    /// Reject an inverted or zero range.
    #[must_use]
    pub const fn valid(self) -> bool {
        self.min < self.max && self.res > 0
    }

    /// Clamp `value` into the range.
    #[must_use]
    pub const fn clamp(self, value: i16) -> i16 {
        if value < self.min {
            self.min
        } else if value > self.max {
            self.max
        } else {
            value
        }
    }
}

/// The result of a control request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    /// The write was accepted; the status stage is zero-length.
    Ack,
    /// The read returns `len` bytes of `bytes`.
    Data {
        /// Data buffer.
        bytes: [u8; 2],
        /// Valid byte count (1 or 2).
        len: u8,
    },
    /// The request is not a supported class request.
    Unsupported,
    /// The request is supported but the value/state is refused.
    Stalled,
}

/// The feature unit's state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeatureUnitState {
    /// The feature unit's ID (the setup `wValue` high byte).
    pub entity: u8,
    /// The AudioControl interface number (the setup `wIndex`).
    pub interface: u8,
    /// Mute state.
    pub mute: bool,
    /// Volume in 1/256 dB.
    pub volume: i16,
    /// Volume range.
    pub limits: VolumeLimits,
}

impl FeatureUnitState {
    /// A unit with `entity`/`interface`, unmuted and at 0 dB (clamped to the range).
    #[must_use]
    pub const fn new(entity: u8, interface: u8, limits: VolumeLimits) -> Self {
        Self {
            entity,
            interface,
            mute: false,
            volume: limits.clamp(0),
            limits,
        }
    }

    /// Handle one class request.
    #[must_use]
    pub fn handle(&mut self, setup: SetupPacket, data: &[u8]) -> Response {
        if !setup.is_class_interface() || setup.index != u16::from(self.interface) {
            return Response::Unsupported;
        }
        let entity = (setup.value >> 8) as u8;
        let selector = setup.value as u8;
        if entity != self.entity {
            return Response::Unsupported;
        }
        let get = matches!(setup.request, GET_CUR | GET_MIN | GET_MAX | GET_RES);
        let set = matches!(setup.request, SET_CUR | SET_MIN | SET_MAX | SET_RES);
        if get && setup.direction() != Direction::In {
            return Response::Unsupported;
        }
        if set && setup.direction() != Direction::Out {
            return Response::Unsupported;
        }
        match selector {
            FEATURE_MUTE => self.mute_request(setup, data),
            FEATURE_VOLUME => self.volume_request(setup, data),
            _ => Response::Unsupported,
        }
    }

    fn mute_request(&mut self, setup: SetupPacket, data: &[u8]) -> Response {
        match setup.request {
            SET_CUR => {
                let Some(value) = data.first().copied() else {
                    return Response::Stalled;
                };
                if value > 1 {
                    return Response::Stalled;
                }
                self.mute = value == 1;
                Response::Ack
            }
            GET_CUR => Response::Data {
                bytes: [u8::from(self.mute), 0],
                len: 1,
            },
            _ => Response::Unsupported,
        }
    }

    fn volume_request(&mut self, setup: SetupPacket, data: &[u8]) -> Response {
        let limits = self.limits;
        match setup.request {
            SET_CUR => {
                let Some(bytes) = data.get(..2) else {
                    return Response::Stalled;
                };
                let value = i16::from_le_bytes([bytes[0], bytes[1]]);
                if value < limits.min || value > limits.max {
                    return Response::Stalled;
                }
                self.volume = value;
                Response::Ack
            }
            GET_CUR => Self::volume_data(self.volume),
            GET_MIN => Self::volume_data(limits.min),
            GET_MAX => Self::volume_data(limits.max),
            GET_RES => Self::volume_data(limits.res),
            _ => Response::Unsupported,
        }
    }

    fn volume_data(value: i16) -> Response {
        let bytes = value.to_le_bytes();
        Response::Data {
            bytes: [bytes[0], bytes[1]],
            len: 2,
        }
    }
}

#[cfg(test)]
#[path = "control_ut.rs"]
mod tests;
