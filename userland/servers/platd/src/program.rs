// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `platd` startup: select the fake or hardware backend and serve `plat:`.

use alloc::boxed::Box;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_plat::{FakePlatform, Platform};
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Server;

use crate::handler::PlatHandler;
use crate::hardware::PiPlatform;

entry!(main);

const MAILBOX_LEN: usize = 0x40;
const GPIO_LEN: usize = 0xb4;
const RNG_LEN: usize = 0x28;
const DMA_LEN: usize = 0x100;

fn main(args: raven_rt::Args) -> i32 {
    let fake = args
        .iter()
        .any(|argument| argument == "fake" || argument.starts_with("fake="));
    let backend: Box<dyn Platform> = if fake {
        Box::new(FakePlatform::new())
    } else {
        match hardware_backend(&args) {
            Ok(platform) => Box::new(platform),
            Err(error) => {
                println!(
                    "platd: hardware backend unavailable: {}",
                    error.description()
                );
                return 1;
            }
        }
    };
    let mut server = match Server::register("plat") {
        Ok(server) => server,
        Err(error) => {
            println!("platd: {}", error.description());
            return 1;
        }
    };
    println!(
        "platd: ready backend={}",
        if fake { "fake" } else { "hardware" }
    );
    let mut handler = PlatHandler::new(backend);
    server.run(&mut handler);
}

fn hardware_backend(args: &raven_rt::Args) -> Result<PiPlatform, Errno> {
    let (mailbox, _) = window(args, "mailbox").ok_or(Errno::EINVAL)?;
    let (gpio, _) = window(args, "gpio").ok_or(Errno::EINVAL)?;
    let (rng, _) = window(args, "rng").ok_or(Errno::EINVAL)?;
    let (dma, dma_len) = window(args, "dma").ok_or(Errno::EINVAL)?;
    let mailbox = map_mmio(mailbox, MAILBOX_LEN)?;
    let gpio = map_mmio(gpio, GPIO_LEN)?;
    let rng = map_mmio(rng, RNG_LEN)?;
    let buffer = map_dma(dma, dma_len.max(DMA_LEN))?;
    PiPlatform::new(mailbox, buffer, gpio, rng)
}

fn window(args: &raven_rt::Args, key: &str) -> Option<(u32, usize)> {
    args.iter().find_map(|argument| {
        let value = argument.strip_prefix(key)?.strip_prefix('=')?;
        let (handle, length) = value.split_once(':')?;
        let handle = handle.parse().ok()?;
        let length = length.parse().ok()?;
        Some((handle, length))
    })
}

fn map_mmio(handle: u32, length: usize) -> Result<raven_driver::MmioRegion, Errno> {
    let window = raven_rt::map_mmio_mut(Handle(handle), length)?;
    Ok(raven_driver::MmioRegion::new(window))
}

fn map_dma(handle: u32, length: usize) -> Result<raven_driver::DmaRegion, Errno> {
    let info = raven_rt::dma_info(Handle(handle))?;
    let window = raven_rt::map_mmio_mut(Handle(handle), length)?;
    Ok(raven_driver::DmaRegion::new(window, info.bus_addr))
}
