// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `plat:` scheme handler.

use alloc::collections::BTreeMap;

use raven_abi::errno::Errno;
use raven_plat::gpio::{Function, PinConfig, Pull};
use raven_plat::protocol::{
    GPIO_LEVEL, GPIO_SET_FUNCTION, GPIO_SET_PULL, Resource, parse_resource,
};
use raven_plat::{GpioArbiter, MAX_RNG_BYTES, Platform};
use raven_proto_fs::{S_IFCHR, Stat};
use raven_rt::println;
use raven_scheme::{Ctx, Handler, HandlerResult, Opened};

struct OpenFile {
    resource: Resource,
}

/// One `platd` handler over a backend.
pub struct PlatHandler<P: Platform> {
    backend: P,
    arbiter: GpioArbiter,
    files: BTreeMap<u64, OpenFile>,
    next_file: u64,
}

impl<P: Platform> PlatHandler<P> {
    /// Create a handler for `backend`.
    pub fn new(backend: P) -> Self {
        Self {
            backend,
            arbiter: GpioArbiter::new(),
            files: BTreeMap::new(),
            next_file: 1,
        }
    }

    fn configure(
        &mut self,
        file: u64,
        pin: u8,
        change: impl FnOnce(&mut PinConfig),
    ) -> Result<(), Errno> {
        let Some(mut config) = self.arbiter.config(pin) else {
            return Err(Errno::EINVAL);
        };
        change(&mut config);
        self.arbiter.configure(pin, file, config)?;
        self.backend.gpio_apply(pin, config)
    }
}

impl<P: Platform> Handler for PlatHandler<P> {
    fn open(&mut self, path: &str, _flags: u32, _mode: u32, _ctx: &Ctx) -> HandlerResult<Opened> {
        let resource = parse_resource(path)?;
        let file = self.next_file;
        self.next_file = self.next_file.saturating_add(1);
        if let Resource::Gpio(pin) = resource {
            match self.arbiter.claim(pin, file) {
                Ok(()) => println!("platd: claim pin={pin}"),
                Err(Errno::EBUSY) => {
                    println!("platd: conflict pin={pin}");
                    return Err(Errno::EBUSY.into());
                }
                Err(error) => return Err(error.into()),
            }
            if let Err(error) = self.backend.gpio_apply(pin, PinConfig::default()) {
                let _ = self.arbiter.release(pin, file);
                return Err(error.into());
            }
        }
        self.files.insert(file, OpenFile { resource });
        Ok(Opened::File(file))
    }

    fn read(&mut self, file: u64, _offset: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
        let resource = self
            .files
            .get(&file)
            .map(|open| open.resource)
            .ok_or(Errno::EBADF)?;
        match resource {
            Resource::Model => copy_u32(self.backend.board_info()?.model, buffer),
            Resource::Revision => copy_u32(self.backend.board_info()?.revision, buffer),
            Resource::Serial => copy_u64(self.backend.board_info()?.serial, buffer),
            Resource::Mac => copy_bytes(&self.backend.board_info()?.mac, buffer),
            Resource::Clock(id) => {
                let rate = self.backend.clock_rate(id)?;
                copy_u32(rate, buffer)
            }
            Resource::Power(id) => {
                let state = self.backend.power_state(id)?;
                copy_u32(state, buffer)
            }
            Resource::Rng => {
                let length = buffer.len().min(MAX_RNG_BYTES);
                let destination = buffer.get_mut(..length).ok_or(Errno::E2BIG)?;
                Ok(self.backend.rng_fill(destination)?)
            }
            Resource::Gpio(pin) => {
                let level = self.backend.gpio_level(pin)?;
                let slot = buffer.first_mut().ok_or(Errno::E2BIG)?;
                *slot = u8::from(level);
                Ok(1)
            }
        }
    }

    fn call(&mut self, file: u64, request: &[u8], response: &mut [u8]) -> HandlerResult<usize> {
        let resource = self
            .files
            .get(&file)
            .map(|open| open.resource)
            .ok_or(Errno::EBADF)?;
        let Resource::Gpio(pin) = resource else {
            return Err(Errno::ENOSYS.into());
        };
        let command = request.first().copied().ok_or(Errno::EINVAL)?;
        let argument = request.get(1).copied().unwrap_or(0);
        match command {
            GPIO_SET_FUNCTION => {
                let function = Function::from_bits(u32::from(argument))?;
                self.configure(file, pin, |config| config.function = function)?;
                Ok(0)
            }
            GPIO_SET_PULL => {
                let pull = Pull::from_bits(u32::from(argument))?;
                self.configure(file, pin, |config| config.pull = pull)?;
                Ok(0)
            }
            GPIO_LEVEL => {
                let level = self.backend.gpio_level(pin)?;
                let slot = response.first_mut().ok_or(Errno::E2BIG)?;
                *slot = u8::from(level);
                Ok(1)
            }
            _ => Err(Errno::EINVAL.into()),
        }
    }

    fn close(&mut self, file: u64) -> HandlerResult<()> {
        if let Some(open) = self.files.remove(&file)
            && let Resource::Gpio(pin) = open.resource
        {
            let _ = self.arbiter.release(pin, file);
            println!("platd: release pin={pin}");
        }
        Ok(())
    }

    fn fstat(&mut self, _file: u64) -> HandlerResult<Stat> {
        Ok(Stat {
            mode: S_IFCHR | 0o666,
            nlink: 1,
            ..Stat::default()
        })
    }
}

fn copy_u32(value: u32, buffer: &mut [u8]) -> HandlerResult<usize> {
    let destination = buffer.get_mut(..4).ok_or(Errno::E2BIG)?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(4)
}

fn copy_u64(value: u64, buffer: &mut [u8]) -> HandlerResult<usize> {
    let destination = buffer.get_mut(..8).ok_or(Errno::E2BIG)?;
    destination.copy_from_slice(&value.to_le_bytes());
    Ok(8)
}

fn copy_bytes(value: &[u8], buffer: &mut [u8]) -> HandlerResult<usize> {
    let destination = buffer.get_mut(..value.len()).ok_or(Errno::E2BIG)?;
    destination.copy_from_slice(value);
    Ok(value.len())
}
