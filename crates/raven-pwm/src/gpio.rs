// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! GPIO40/41 pinmux arithmetic for PWM1.
//!
//! Both pins use ALT0 and pull-off in the Pi 4 `audio_pins` group. The register writes are only
//! data here; the board seam claims the pins through `platd` before applying them.

use crate::registers::{GPIO_ALT0, gpio_fsel_shift, gpio_pull_shift};

/// The two GPIO pins used by the 3.5 mm jack.
pub const PWM_PINS: [u8; 2] = [40, 41];

/// The computed GPIO register values for the PWM1 pins.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PinPlan {
    /// `GPFSEL4` value.
    pub gpfsel4: u32,
    /// `GPPUPPDN2` value with both fields pull-off.
    pub gppuppdn2: u32,
}

impl PinPlan {
    /// Apply ALT0 to GPIO40/41 and pull-off to both fields.
    #[must_use]
    pub const fn new(gpfsel4: u32, gppuppdn2: u32) -> Self {
        let mut fsel = gpfsel4;
        let mut pull = gppuppdn2;
        let mut index = 0usize;
        while index < PWM_PINS.len() {
            let pin = PWM_PINS[index];
            let fsel_shift = match gpio_fsel_shift(pin) {
                Some(value) => value,
                None => {
                    return Self {
                        gpfsel4: fsel,
                        gppuppdn2: pull,
                    };
                }
            };
            let pull_shift = match gpio_pull_shift(pin) {
                Some(value) => value,
                None => {
                    return Self {
                        gpfsel4: fsel,
                        gppuppdn2: pull,
                    };
                }
            };
            fsel &= !(0x7 << fsel_shift);
            fsel |= GPIO_ALT0 << fsel_shift;
            pull &= !(0x3 << pull_shift);
            index += 1;
        }
        Self {
            gpfsel4: fsel,
            gppuppdn2: pull,
        }
    }

    /// The `GPFSEL4` value after the PWM1 pins are ALT0.
    #[must_use]
    pub const fn gpfsel4(self) -> u32 {
        self.gpfsel4
    }

    /// The `GPPUPPDN2` value after the PWM1 pins are pull-off.
    #[must_use]
    pub const fn gppuppdn2(self) -> u32 {
        self.gppuppdn2
    }
}

impl Default for PinPlan {
    fn default() -> Self {
        Self::new(0, 0)
    }
}

/// Apply the PWM1 pin configuration to the two GPIO registers.
#[must_use]
pub const fn configure(current_gpfsel4: u32, current_gppuppdn2: u32) -> PinPlan {
    PinPlan::new(current_gpfsel4, current_gppuppdn2)
}

/// True when the `GPFSEL4` fields for GPIO40/41 are ALT0.
#[must_use]
pub const fn gpfsel4_is_alt0(value: u32) -> bool {
    let mut index = 0usize;
    while index < PWM_PINS.len() {
        let shift = match gpio_fsel_shift(PWM_PINS[index]) {
            Some(value) => value,
            None => return false,
        };
        if (value >> shift) & 0x7 != GPIO_ALT0 {
            return false;
        }
        index += 1;
    }
    true
}

/// True when the `GPPUPPDN2` fields for GPIO40/41 are pull-off.
#[must_use]
pub const fn gppuppdn2_is_pull_off(value: u32) -> bool {
    let mut index = 0usize;
    while index < PWM_PINS.len() {
        let shift = match gpio_pull_shift(PWM_PINS[index]) {
            Some(value) => value,
            None => return false,
        };
        if (value >> shift) & 0x3 != 0 {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
#[path = "gpio_ut.rs"]
mod tests;
