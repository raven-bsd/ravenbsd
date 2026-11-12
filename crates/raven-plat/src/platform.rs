// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Backend boundary between `platd` and the hardware/fake platform.

use raven_abi::errno::Errno;

use alloc::boxed::Box;

use crate::gpio::{GPIO_PINS, PinConfig};
use crate::mailbox::{BoardInfo, FakeMailbox, Mailbox, TAG_GET_CLOCKRATE, TAG_GET_POWERSTATE};
use crate::rng::{FakeRng, Rng};

/// The operations `platd` exposes through the `plat:` scheme.
pub trait Platform {
    /// Board identity.
    fn board_info(&mut self) -> Result<BoardInfo, Errno>;
    /// Firmware clock rate for `id`.
    fn clock_rate(&mut self, id: u32) -> Result<u32, Errno>;
    /// Firmware power-domain state for `id`.
    fn power_state(&mut self, id: u32) -> Result<u32, Errno>;
    /// Fill `out` with bounded entropy.
    fn rng_fill(&mut self, out: &mut [u8]) -> Result<usize, Errno>;
    /// Apply a pin configuration after the arbiter accepts it.
    fn gpio_apply(&mut self, pin: u8, config: PinConfig) -> Result<(), Errno>;
    /// Read the current pin level.
    fn gpio_level(&mut self, pin: u8) -> Result<bool, Errno>;
}

/// The fake platform used by the host tests and the QEMU qtest.
pub struct FakePlatform {
    mailbox: FakeMailbox,
    rng: FakeRng,
}

impl FakePlatform {
    /// A fake platform with the default board, clocks, power domains and seed.
    pub const fn new() -> Self {
        Self {
            mailbox: FakeMailbox::new(),
            rng: FakeRng::new(),
        }
    }
}

impl Default for FakePlatform {
    fn default() -> Self {
        Self::new()
    }
}

impl Platform for FakePlatform {
    fn board_info(&mut self) -> Result<BoardInfo, Errno> {
        Ok(self.mailbox.board)
    }

    fn clock_rate(&mut self, id: u32) -> Result<u32, Errno> {
        let mut values = [0u32; 2];
        let count = self.mailbox.query(TAG_GET_CLOCKRATE, &[id], &mut values)?;
        if count < 2 {
            return Err(Errno::EPROTO);
        }
        Ok(values[1])
    }

    fn power_state(&mut self, id: u32) -> Result<u32, Errno> {
        let mut values = [0u32; 2];
        let count = self.mailbox.query(TAG_GET_POWERSTATE, &[id], &mut values)?;
        if count < 2 {
            return Err(Errno::EPROTO);
        }
        Ok(values[1])
    }

    fn rng_fill(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        self.rng.fill(out)
    }

    fn gpio_apply(&mut self, pin: u8, _config: PinConfig) -> Result<(), Errno> {
        if usize::from(pin) >= GPIO_PINS {
            return Err(Errno::EINVAL);
        }
        Ok(())
    }

    fn gpio_level(&mut self, pin: u8) -> Result<bool, Errno> {
        if usize::from(pin) >= GPIO_PINS {
            return Err(Errno::EINVAL);
        }
        Ok(pin.is_multiple_of(2))
    }
}

impl<T: Platform + ?Sized> Platform for Box<T> {
    fn board_info(&mut self) -> Result<BoardInfo, Errno> {
        (**self).board_info()
    }

    fn clock_rate(&mut self, id: u32) -> Result<u32, Errno> {
        (**self).clock_rate(id)
    }

    fn power_state(&mut self, id: u32) -> Result<u32, Errno> {
        (**self).power_state(id)
    }

    fn rng_fill(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        (**self).rng_fill(out)
    }

    fn gpio_apply(&mut self, pin: u8, config: PinConfig) -> Result<(), Errno> {
        (**self).gpio_apply(pin, config)
    }

    fn gpio_level(&mut self, pin: u8) -> Result<bool, Errno> {
        (**self).gpio_level(pin)
    }
}

// The fake platform is the only backend needed on the host; the real one lives in platd.
#[cfg(test)]
#[path = "platform_ut.rs"]
mod tests;
