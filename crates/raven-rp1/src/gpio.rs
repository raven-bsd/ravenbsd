// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GPIO and pinctrl state.
//!
//! The RP1 GPIO block is also the pin controller: a pin has exactly one owner, an optional
//! alternate function declared by the device tree, a direction, a bias and a logical level. This
//! module models those states and the ownership invariants; it does not write registers and makes
//! no claim about the RP1 pin-mux encoding, which is UNVERIFIED.

use core::fmt;

use crate::name::Name;

/// Largest pin count the model accepts. The RP1 device tree declares 54 (`gpio-ranges`).
pub const MAX_PINS: usize = 64;
/// Largest number of pin rules copied from the pinctrl groups.
pub const MAX_RULES: usize = 16;

/// The bias of a pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bias {
    /// The device tree does not constrain the bias; leave the pin as it is.
    Unspecified,
    /// No pull resistor.
    Disable,
    /// Pull up.
    PullUp,
    /// Pull down.
    PullDown,
    /// Bus hold.
    BusHold,
}

/// The direction of a pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Input.
    Input,
    /// Output.
    Output,
}

/// One pin configuration declared by a device-tree pinctrl group.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinRule {
    /// Pin number (`gpioN` in the tree).
    pub pin: u8,
    /// Alternate function name (`uart0`, `i2c0`, ...).
    pub function: Name,
    /// Bias the group declares, or [`Bias::Unspecified`].
    pub bias: Bias,
}

/// The pinctrl rules copied from the GPIO node's groups.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pinctrl {
    rules: [Option<PinRule>; MAX_RULES],
    count: usize,
}

impl Pinctrl {
    /// An empty rule set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rules: [None; MAX_RULES],
            count: 0,
        }
    }

    /// Add one rule; returns `false` when the table is full.
    pub fn push(&mut self, rule: PinRule) -> bool {
        match self.rules.get_mut(self.count) {
            Some(slot) => {
                *slot = Some(rule);
                self.count = self.count.saturating_add(1);
                true
            }
            None => false,
        }
    }

    /// Number of rules.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.count
    }

    /// True when no rule is present.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Iterate the rules.
    pub fn iter(&self) -> impl Iterator<Item = &PinRule> {
        self.rules.iter().flatten()
    }

    /// True when the device tree declares `function` on `pin`.
    #[must_use]
    pub fn allows(&self, pin: u8, function: Name) -> bool {
        self.iter()
            .any(|rule| rule.pin == pin && rule.function == function)
    }

    /// The default bias the tree declares for `(pin, function)`.
    #[must_use]
    pub fn bias_for(&self, pin: u8, function: Name) -> Option<Bias> {
        self.iter()
            .find(|rule| rule.pin == pin && rule.function == function)
            .map(|rule| rule.bias)
    }
}

/// The state of one claimed pin.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinConfig {
    /// Alternate function, or `None` for plain GPIO.
    pub function: Option<Name>,
    /// Input or output.
    pub direction: Direction,
    /// Bias.
    pub bias: Bias,
    /// Logical level; only meaningful for an output.
    pub high: bool,
}

impl PinConfig {
    /// The value of a released pin.
    pub const UNCLAIMED: Self = Self {
        function: None,
        direction: Direction::Input,
        bias: Bias::Unspecified,
        high: false,
    };
}

/// Why a pin transition is invalid.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GpioError {
    /// The pin count is zero or above [`MAX_PINS`].
    PinCount(u8),
    /// The pin number is outside the declared count.
    PinOutOfRange(u8),
    /// Another owner holds the pin.
    AlreadyClaimed {
        /// Pin number.
        pin: u8,
    },
    /// The caller does not own the pin.
    NotOwner {
        /// Pin number.
        pin: u8,
    },
    /// The device tree does not declare this function on the pin.
    FunctionNotOnPin {
        /// Pin number.
        pin: u8,
        /// Requested function.
        function: Name,
    },
    /// The pin is not configured as an output.
    NotOutput {
        /// Pin number.
        pin: u8,
    },
}

impl fmt::Display for GpioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PinCount(pins) => write!(f, "pin count {pins} is outside 1..={MAX_PINS}"),
            Self::PinOutOfRange(pin) => write!(f, "pin {pin} is outside the declared range"),
            Self::AlreadyClaimed { pin } => write!(f, "pin {pin} already has an owner"),
            Self::NotOwner { pin } => write!(f, "the caller does not own pin {pin}"),
            Self::FunctionNotOnPin { pin, function } => {
                write!(f, "function {function} is not declared on pin {pin}")
            }
            Self::NotOutput { pin } => write!(f, "pin {pin} is not an output"),
        }
    }
}

impl core::error::Error for GpioError {}

/// The claimed pins and their configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinController {
    pins: u8,
    owners: [Option<Name>; MAX_PINS],
    configs: [PinConfig; MAX_PINS],
    rules: Pinctrl,
}

impl PinController {
    /// Build a controller for `pins` pins and the device-tree rules.
    ///
    /// # Errors
    ///
    /// Returns [`GpioError::PinCount`] for zero pins or more than [`MAX_PINS`].
    pub const fn new(pins: u8, rules: Pinctrl) -> Result<Self, GpioError> {
        if pins == 0 || pins as usize > MAX_PINS {
            return Err(GpioError::PinCount(pins));
        }
        Ok(Self {
            pins,
            owners: [None; MAX_PINS],
            configs: [PinConfig::UNCLAIMED; MAX_PINS],
            rules,
        })
    }

    /// Number of pins.
    #[must_use]
    pub const fn pin_count(&self) -> u8 {
        self.pins
    }

    /// The pinctrl rules.
    #[must_use]
    pub const fn pinctrl(&self) -> Pinctrl {
        self.rules
    }

    /// The configuration of `pin`, or `None` when the pin does not exist.
    #[must_use]
    pub fn config(&self, pin: u8) -> Option<&PinConfig> {
        if pin >= self.pins {
            return None;
        }
        self.configs.get(usize::from(pin))
    }

    /// The owner of `pin`, when claimed.
    #[must_use]
    pub fn owner(&self, pin: u8) -> Option<Name> {
        if pin >= self.pins {
            return None;
        }
        self.owners.get(usize::from(pin)).copied().flatten()
    }

    /// Claim a pin for `owner`.
    ///
    /// # Errors
    ///
    /// Returns [`GpioError::PinOutOfRange`] for an unknown pin and
    /// [`GpioError::AlreadyClaimed`] when the pin already has an owner.
    pub fn claim(&mut self, pin: u8, owner: Name) -> Result<(), GpioError> {
        if pin >= self.pins {
            return Err(GpioError::PinOutOfRange(pin));
        }
        let slot = usize::from(pin);
        if self.owners.get(slot).copied().flatten().is_some() {
            return Err(GpioError::AlreadyClaimed { pin });
        }
        if let Some(owner_slot) = self.owners.get_mut(slot) {
            *owner_slot = Some(owner);
        }
        if let Some(config) = self.configs.get_mut(slot) {
            *config = PinConfig::UNCLAIMED;
        }
        Ok(())
    }

    /// Release a pin owned by `owner`.
    ///
    /// # Errors
    ///
    /// Returns [`GpioError::PinOutOfRange`] for an unknown pin and [`GpioError::NotOwner`] when
    /// `owner` does not hold it.
    pub fn release(&mut self, pin: u8, owner: Name) -> Result<(), GpioError> {
        if pin >= self.pins {
            return Err(GpioError::PinOutOfRange(pin));
        }
        let slot = usize::from(pin);
        if self.owners.get(slot).copied().flatten() != Some(owner) {
            return Err(GpioError::NotOwner { pin });
        }
        if let Some(owner_slot) = self.owners.get_mut(slot) {
            *owner_slot = None;
        }
        if let Some(config) = self.configs.get_mut(slot) {
            *config = PinConfig::UNCLAIMED;
        }
        Ok(())
    }

    /// Configure a claimed pin.
    ///
    /// `function` is `None` for plain GPIO; an alternate function must be one the device tree
    /// declares on that pin.
    ///
    /// # Errors
    ///
    /// Returns [`GpioError::PinOutOfRange`], [`GpioError::NotOwner`] or
    /// [`GpioError::FunctionNotOnPin`].
    pub fn configure(
        &mut self,
        pin: u8,
        owner: Name,
        function: Option<Name>,
        direction: Direction,
        bias: Bias,
    ) -> Result<(), GpioError> {
        if pin >= self.pins {
            return Err(GpioError::PinOutOfRange(pin));
        }
        let slot = usize::from(pin);
        if self.owners.get(slot).copied().flatten() != Some(owner) {
            return Err(GpioError::NotOwner { pin });
        }
        if let Some(function) = function
            && !self.rules.allows(pin, function)
        {
            return Err(GpioError::FunctionNotOnPin { pin, function });
        }
        if let Some(config) = self.configs.get_mut(slot) {
            *config = PinConfig {
                function,
                direction,
                bias,
                high: false,
            };
        }
        Ok(())
    }

    /// Drive an output pin.
    ///
    /// # Errors
    ///
    /// Returns [`GpioError::PinOutOfRange`], [`GpioError::NotOwner`] or
    /// [`GpioError::NotOutput`].
    pub fn set_high(&mut self, pin: u8, owner: Name, high: bool) -> Result<(), GpioError> {
        if pin >= self.pins {
            return Err(GpioError::PinOutOfRange(pin));
        }
        let slot = usize::from(pin);
        if self.owners.get(slot).copied().flatten() != Some(owner) {
            return Err(GpioError::NotOwner { pin });
        }
        let Some(current) = self.configs.get(slot) else {
            return Err(GpioError::PinOutOfRange(pin));
        };
        if current.direction != Direction::Output {
            return Err(GpioError::NotOutput { pin });
        }
        let mut config = *current;
        config.high = high;
        if let Some(slot) = self.configs.get_mut(slot) {
            *slot = config;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "gpio_ut.rs"]
mod tests;
