// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `platd` startup: select the fake or hardware backend and serve `plat:`.

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_plat::{FakePlatform, Platform};
use raven_platd::resources;
use raven_rt::entry;
use raven_rt::println;
use raven_scheme::Server;

use crate::handler::PlatHandler;
use crate::hardware::PiPlatform;

entry!(main);

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
    let args: Vec<&str> = args.iter().collect();
    let resources = resources::parse(&args).map_err(|error| {
        println!("platd: resource plan: {error}");
        Errno::EINVAL
    })?;
    let mailbox = map_mmio(resources.mailbox.handle, resources.mailbox.len)?;
    let gpio = map_mmio(resources.gpio.handle, resources.gpio.len)?;
    let rng = map_mmio(resources.rng.handle, resources.rng.len)?;
    let buffer = map_dma(resources.dma.handle, resources.dma.len)?;
    PiPlatform::new(mailbox, buffer, gpio, rng)
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
