// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hardware backend over the MMIO/DMA capabilities granted to `platd`.
//!
//! Nothing here hard-codes a physical address: the program maps `MmioRegion` and `DmaBuffer`
//! handles passed by the supervisor and constructs this backend from those mappings.

use core::hint::spin_loop;

use raven_abi::errno::Errno;
use raven_driver::{DmaRegion, MmioRegion};
use raven_plat::Platform;
use raven_plat::gpio::{PinConfig, Pull};
use raven_plat::mailbox::{
    BoardInfo, MAX_PROPERTY_BYTES, TAG_GET_BOARD_MODEL, TAG_GET_BOARD_REVISION,
    TAG_GET_BOARDSERIAL, TAG_GET_CLOCKRATE, TAG_GET_MACADDRESS, TAG_GET_POWERSTATE,
    decode_response, encode_request,
};
use raven_plat::rng::{Registers, Rng, Rng200};

const MBOX_READ: usize = 0x00;
const MBOX_STATUS: usize = 0x18;
const MBOX_WRITE: usize = 0x20;
const MBOX_FULL: u32 = 1 << 31;
const MBOX_EMPTY: u32 = 1 << 30;
const MBOX_CHANNEL: u32 = 8;
const MBOX_RETRIES: usize = 1_000_000;

const GPFSEL_BASE: usize = 0x00;
const GPSET0: usize = 0x1c;
const GPCLR0: usize = 0x28;
const GPLEV0: usize = 0x34;
const GPPUD: usize = 0x94;
const GPPUDCLK0: usize = 0x98;
const GPIO_RETRIES: usize = 150;

/// VideoCore property-channel client.
pub struct HardwareMailbox {
    registers: MmioRegion,
    buffer: DmaRegion,
}

impl HardwareMailbox {
    /// Wrap the mailbox registers and the DMA buffer used for property messages.
    pub fn new(registers: MmioRegion, buffer: DmaRegion) -> Self {
        Self { registers, buffer }
    }

    fn query(&mut self, tag: u32, request: &[u32], response: &mut [u32]) -> Result<usize, Errno> {
        let mut bytes = [0u8; MAX_PROPERTY_BYTES];
        let size = encode_request(&mut bytes, tag, request)?;
        self.buffer
            .write_bytes(0, bytes.get(..size).ok_or(Errno::E2BIG)?)
            .map_err(|_| Errno::EIO)?;
        self.buffer.publish();
        let word = (self.buffer.bus_address() & !0xf) | u64::from(MBOX_CHANNEL);
        let word = u32::try_from(word).map_err(|_| Errno::EINVAL)?;

        self.wait_status(MBOX_FULL, false)?;
        self.registers
            .write_u32(MBOX_WRITE, word)
            .map_err(|_| Errno::EIO)?;
        self.wait_status(MBOX_EMPTY, false)?;
        let reply = self.registers.read_u32(MBOX_READ).map_err(|_| Errno::EIO)?;
        if reply & 0xf != MBOX_CHANNEL {
            return Err(Errno::EPROTO);
        }
        self.buffer.acquire();
        self.buffer
            .read_bytes(0, bytes.get_mut(..size).ok_or(Errno::E2BIG)?)
            .map_err(|_| Errno::EIO)?;
        decode_response(bytes.get(..size).ok_or(Errno::E2BIG)?, tag, response)
    }

    fn wait_status(&mut self, mask: u32, set: bool) -> Result<(), Errno> {
        let mut attempt = 0usize;
        while attempt < MBOX_RETRIES {
            let status = self
                .registers
                .read_u32(MBOX_STATUS)
                .map_err(|_| Errno::EIO)?;
            if (status & mask != 0) == set {
                return Ok(());
            }
            spin_loop();
            attempt += 1;
        }
        Err(Errno::ETIMEDOUT)
    }
}

/// BCM2711 GPIO register backend.
pub struct HardwareGpio {
    registers: MmioRegion,
}

impl HardwareGpio {
    /// Wrap the GPIO register window.
    pub fn new(registers: MmioRegion) -> Self {
        Self { registers }
    }

    fn apply(&mut self, pin: u8, config: PinConfig) -> Result<(), Errno> {
        self.set_function(pin, config.function.bits())?;
        self.set_pull(pin, config.pull)?;
        if config.function == raven_plat::gpio::Function::Output {
            self.set_level(pin, config.level)?;
        }
        Ok(())
    }

    fn set_function(&mut self, pin: u8, function: u32) -> Result<(), Errno> {
        let pin = usize::from(pin);
        let offset = GPFSEL_BASE
            .checked_add((pin / 10).saturating_mul(4))
            .ok_or(Errno::EINVAL)?;
        let shift = (pin % 10).saturating_mul(3);
        let mask = 7u32.checked_shl(shift as u32).ok_or(Errno::EINVAL)?;
        let current = self.registers.read_u32(offset).map_err(|_| Errno::EIO)?;
        let value = (current & !mask) | ((function & 7) << shift);
        self.registers
            .write_u32(offset, value)
            .map_err(|_| Errno::EIO)
    }

    fn set_pull(&mut self, pin: u8, pull: Pull) -> Result<(), Errno> {
        self.registers
            .write_u32(GPPUD, pull.bits())
            .map_err(|_| Errno::EIO)?;
        delay();
        let (clock, bit) = clock_register(pin)?;
        self.registers
            .write_u32(clock, bit)
            .map_err(|_| Errno::EIO)?;
        delay();
        self.registers.write_u32(GPPUD, 0).map_err(|_| Errno::EIO)?;
        self.registers.write_u32(clock, 0).map_err(|_| Errno::EIO)
    }

    fn set_level(&mut self, pin: u8, high: bool) -> Result<(), Errno> {
        let (register, bit) = output_register(pin, high)?;
        self.registers
            .write_u32(register, bit)
            .map_err(|_| Errno::EIO)
    }

    fn level(&mut self, pin: u8) -> Result<bool, Errno> {
        let (register, bit) = level_register(pin)?;
        let value = self.registers.read_u32(register).map_err(|_| Errno::EIO)?;
        Ok(value & bit != 0)
    }
}

fn clock_register(pin: u8) -> Result<(usize, u32), Errno> {
    let pin = u32::from(pin);
    if pin < 32 {
        Ok((GPPUDCLK0, 1 << pin))
    } else if pin < 58 {
        Ok((GPPUDCLK0 + 4, 1 << (pin - 32)))
    } else {
        Err(Errno::EINVAL)
    }
}

fn level_register(pin: u8) -> Result<(usize, u32), Errno> {
    let pin = u32::from(pin);
    let base = if pin < 32 {
        GPLEV0
    } else if pin < 58 {
        GPLEV0 + 4
    } else {
        return Err(Errno::EINVAL);
    };
    let bit = if pin < 32 { pin } else { pin - 32 };
    Ok((base, 1 << bit))
}

fn output_register(pin: u8, high: bool) -> Result<(usize, u32), Errno> {
    let pin = u32::from(pin);
    let base = match (pin < 32, high) {
        (true, true) => GPSET0,
        (true, false) => GPCLR0,
        (false, true) if pin < 58 => GPSET0 + 4,
        (false, false) if pin < 58 => GPCLR0 + 4,
        _ => return Err(Errno::EINVAL),
    };
    let bit = if pin < 32 { pin } else { pin - 32 };
    Ok((base, 1 << bit))
}

fn delay() {
    let mut count = 0usize;
    while count < GPIO_RETRIES {
        spin_loop();
        count += 1;
    }
}

/// `Registers` adapter for the RNG200 window.
pub struct MmioRegisters {
    region: MmioRegion,
}

impl Registers for MmioRegisters {
    fn read32(&mut self, offset: usize) -> Result<u32, Errno> {
        self.region.read_u32(offset).map_err(|_| Errno::EIO)
    }

    fn write32(&mut self, offset: usize, value: u32) -> Result<(), Errno> {
        self.region.write_u32(offset, value).map_err(|_| Errno::EIO)
    }
}

/// The Pi 4 platform backend.
pub struct PiPlatform {
    mailbox: HardwareMailbox,
    gpio: HardwareGpio,
    rng: Rng200<MmioRegisters>,
}

impl PiPlatform {
    /// Build the backend from mapped mailbox, GPIO and RNG windows plus a mailbox DMA buffer.
    pub fn new(
        mailbox: MmioRegion,
        mailbox_buffer: DmaRegion,
        gpio: MmioRegion,
        rng: MmioRegion,
    ) -> Result<Self, Errno> {
        let mut rng = Rng200::new(MmioRegisters { region: rng });
        rng.init()?;
        Ok(Self {
            mailbox: HardwareMailbox::new(mailbox, mailbox_buffer),
            gpio: HardwareGpio::new(gpio),
            rng,
        })
    }
}

impl Platform for PiPlatform {
    fn board_info(&mut self) -> Result<BoardInfo, Errno> {
        let model = self.query_one(TAG_GET_BOARD_MODEL)?;
        let revision = self.query_one(TAG_GET_BOARD_REVISION)?;
        let mut values = [0u32; 2];
        let serial = if self.mailbox.query(TAG_GET_BOARDSERIAL, &[], &mut values)? == 2 {
            u64::from(values[0]) | (u64::from(values[1]) << 32)
        } else {
            return Err(Errno::EPROTO);
        };
        let mac = if self.mailbox.query(TAG_GET_MACADDRESS, &[], &mut values)? == 2 {
            let mut mac = [0u8; 6];
            let first = values[0].to_le_bytes();
            let second = values[1].to_le_bytes();
            mac[..4].copy_from_slice(&first);
            mac[4..].copy_from_slice(&second[..2]);
            mac
        } else {
            return Err(Errno::EPROTO);
        };
        Ok(BoardInfo {
            model,
            revision,
            serial,
            mac,
        })
    }

    fn clock_rate(&mut self, id: u32) -> Result<u32, Errno> {
        let mut values = [0u32; 2];
        if self.mailbox.query(TAG_GET_CLOCKRATE, &[id], &mut values)? != 2 {
            return Err(Errno::EPROTO);
        }
        Ok(values[1])
    }

    fn power_state(&mut self, id: u32) -> Result<u32, Errno> {
        let mut values = [0u32; 2];
        if self.mailbox.query(TAG_GET_POWERSTATE, &[id], &mut values)? != 2 {
            return Err(Errno::EPROTO);
        }
        Ok(values[1])
    }

    fn rng_fill(&mut self, out: &mut [u8]) -> Result<usize, Errno> {
        self.rng.fill(out)
    }

    fn gpio_apply(&mut self, pin: u8, config: PinConfig) -> Result<(), Errno> {
        self.gpio.apply(pin, config)
    }

    fn gpio_level(&mut self, pin: u8) -> Result<bool, Errno> {
        self.gpio.level(pin)
    }
}

impl PiPlatform {
    fn query_one(&mut self, tag: u32) -> Result<u32, Errno> {
        let mut values = [0u32; 1];
        if self.mailbox.query(tag, &[], &mut values)? == 1 {
            Ok(values[0])
        } else {
            Err(Errno::EPROTO)
        }
    }
}
