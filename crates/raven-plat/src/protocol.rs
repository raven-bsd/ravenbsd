// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `plat:` path parsing and the small GPIO call encoding.

use raven_abi::errno::Errno;

/// GPIO call: set the function; argument is a [`crate::gpio::Function`] value.
pub const GPIO_SET_FUNCTION: u8 = 0;
/// GPIO call: set the pull; argument is a [`crate::gpio::Pull`] value.
pub const GPIO_SET_PULL: u8 = 1;
/// GPIO call: read the current level (argument ignored).
pub const GPIO_LEVEL: u8 = 2;

/// One resource a client can open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resource {
    /// Board model (`GET_BOARD_MODEL`).
    Model,
    /// Board revision (`GET_BOARD_REVISION`).
    Revision,
    /// Board serial (`GET_BOARDSERIAL`).
    Serial,
    /// Ethernet MAC (`GET_MACADDRESS`).
    Mac,
    /// Firmware clock rate for this id (`GET_CLOCKRATE`).
    Clock(u32),
    /// Firmware power-domain state for this id (`GET_POWERSTATE`).
    Power(u32),
    /// Claim this GPIO pin and use the handle as the ownership lease.
    Gpio(u8),
    /// Bounded RNG200 entropy.
    Rng,
}

/// Parse a `plat:` suffix (`model`, `clock/12`, `gpio/17`, ...).
pub fn parse_resource(path: &str) -> Result<Resource, Errno> {
    let path = path.strip_prefix('/').unwrap_or(path);
    match path {
        "" | "model" => Ok(Resource::Model),
        "revision" => Ok(Resource::Revision),
        "serial" => Ok(Resource::Serial),
        "mac" => Ok(Resource::Mac),
        "rng" => Ok(Resource::Rng),
        _ => {
            if let Some(rest) = path.strip_prefix("clock/") {
                let id = parse_id(rest, 64)?;
                Ok(Resource::Clock(id))
            } else if let Some(rest) = path.strip_prefix("power/") {
                let id = parse_id(rest, 16)?;
                Ok(Resource::Power(id))
            } else if let Some(rest) = path.strip_prefix("gpio/") {
                let pin = parse_id(rest, u32::from(crate::gpio::GPIO_PINS as u8))?;
                Ok(Resource::Gpio(pin as u8))
            } else {
                Err(Errno::ENOENT)
            }
        }
    }
}

fn parse_id(text: &str, limit: u32) -> Result<u32, Errno> {
    if text.is_empty() || text.len() > 2 {
        return Err(Errno::EINVAL);
    }
    let value = text.parse::<u32>().map_err(|_| Errno::EINVAL)?;
    if value >= limit {
        return Err(Errno::EINVAL);
    }
    Ok(value)
}

/// Encode a GPIO call: `[command, argument]`.
pub fn encode_gpio_call(command: u8, argument: u8) -> [u8; 2] {
    [command, argument]
}

#[cfg(test)]
#[path = "protocol_ut.rs"]
mod tests;
