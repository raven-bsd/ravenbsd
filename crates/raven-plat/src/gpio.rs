// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GPIO/pinmux arbitration.
//!
//! The arbiter is pure bookkeeping: one owner per pin, a configuration per pin and the
//! `claim`/`release` transitions. The `platd` backend applies a configuration to the BCM2711
//! registers after the arbiter accepts it, and the kernel's CLOSE notification releases the owner
//! when a client dies.

use raven_abi::errno::Errno;

/// Number of BCM2711 GPIO pins.
pub const GPIO_PINS: usize = 58;

/// BCM2711 pin function.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Function {
    /// GPIO input.
    Input,
    /// GPIO output.
    Output,
    /// Alternate function 0.
    Alt0,
    /// Alternate function 1.
    Alt1,
    /// Alternate function 2.
    Alt2,
    /// Alternate function 3.
    Alt3,
    /// Alternate function 4.
    Alt4,
    /// Alternate function 5.
    Alt5,
}

impl Function {
    /// The three-bit `GPFSEL` encoding.
    pub const fn bits(self) -> u32 {
        match self {
            Self::Input => 0,
            Self::Output => 1,
            Self::Alt0 => 4,
            Self::Alt1 => 5,
            Self::Alt2 => 6,
            Self::Alt3 => 7,
            Self::Alt4 => 3,
            Self::Alt5 => 2,
        }
    }

    /// Decode a three-bit `GPFSEL` value.
    pub const fn from_bits(value: u32) -> Result<Self, Errno> {
        match value & 7 {
            0 => Ok(Self::Input),
            1 => Ok(Self::Output),
            2 => Ok(Self::Alt5),
            3 => Ok(Self::Alt4),
            4 => Ok(Self::Alt0),
            5 => Ok(Self::Alt1),
            6 => Ok(Self::Alt2),
            7 => Ok(Self::Alt3),
            _ => Err(Errno::EINVAL),
        }
    }
}

/// Pin pull configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pull {
    /// No pull.
    None,
    /// Pull up.
    Up,
    /// Pull down.
    Down,
}

impl Pull {
    /// The two-bit `GPPUD` encoding.
    pub const fn bits(self) -> u32 {
        match self {
            Self::None => 0,
            Self::Up => 1,
            Self::Down => 2,
        }
    }

    /// Decode a two-bit `GPPUD` value.
    pub const fn from_bits(value: u32) -> Result<Self, Errno> {
        match value & 3 {
            0 => Ok(Self::None),
            1 => Ok(Self::Up),
            2 => Ok(Self::Down),
            _ => Err(Errno::EINVAL),
        }
    }
}

/// One pin's requested configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinConfig {
    /// Pin function.
    pub function: Function,
    /// Pull setting.
    pub pull: Pull,
    /// Initial output level (`true` high) when the function is output.
    pub level: bool,
}

impl Default for PinConfig {
    fn default() -> Self {
        Self {
            function: Function::Input,
            pull: Pull::None,
            level: false,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PinState {
    owner: Option<u64>,
    config: PinConfig,
}

/// The per-server GPIO arbiter.
pub struct GpioArbiter {
    pins: [PinState; GPIO_PINS],
}

impl GpioArbiter {
    /// An unclaimed arbiter.
    pub const fn new() -> Self {
        Self {
            pins: [PinState {
                owner: None,
                config: PinConfig {
                    function: Function::Input,
                    pull: Pull::None,
                    level: false,
                },
            }; GPIO_PINS],
        }
    }

    /// Claim `pin` for `owner`. A pin with a live owner is `EBUSY`.
    pub fn claim(&mut self, pin: u8, owner: u64) -> Result<(), Errno> {
        let state = self.state_mut(pin)?;
        if state.owner.is_some() {
            return Err(Errno::EBUSY);
        }
        state.owner = Some(owner);
        state.config = PinConfig::default();
        Ok(())
    }

    /// Release `pin`; only the owner may release it.
    pub fn release(&mut self, pin: u8, owner: u64) -> Result<(), Errno> {
        let state = self.state_mut(pin)?;
        match state.owner {
            Some(current) if current == owner => {
                state.owner = None;
                state.config = PinConfig::default();
                Ok(())
            }
            Some(_) => Err(Errno::EPERM),
            None => Err(Errno::EINVAL),
        }
    }

    /// Configure a claimed pin.
    pub fn configure(&mut self, pin: u8, owner: u64, config: PinConfig) -> Result<(), Errno> {
        let state = self.state_mut(pin)?;
        match state.owner {
            Some(current) if current == owner => {
                state.config = config;
                Ok(())
            }
            Some(_) => Err(Errno::EPERM),
            None => Err(Errno::EINVAL),
        }
    }

    /// Current configuration, if claimed.
    pub fn config(&self, pin: u8) -> Option<PinConfig> {
        self.pins.get(usize::from(pin)).map(|state| state.config)
    }

    /// True when the pin has an owner.
    pub fn is_claimed(&self, pin: u8) -> bool {
        self.pins
            .get(usize::from(pin))
            .is_some_and(|state| state.owner.is_some())
    }

    fn state_mut(&mut self, pin: u8) -> Result<&mut PinState, Errno> {
        self.pins.get_mut(usize::from(pin)).ok_or(Errno::EINVAL)
    }
}

impl Default for GpioArbiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "gpio_ut.rs"]
mod tests;
