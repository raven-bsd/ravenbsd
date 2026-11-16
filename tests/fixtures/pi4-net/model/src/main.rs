// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/init` for the `pi4-net-model` qtest fixture.
//!
//! The guest reads the normalized launch/readiness chain from the initfs and
//! checks both a coherent model and a duplicate-launcher model. It never reads
//! a board log or touches GENET hardware.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_initfs::Initfs;
    use raven_rt::entry;

    entry!(main);

    const GOOD: &str = "/etc/pi4-net-good.chain";
    const DUPLICATE: &str = "/etc/pi4-net-duplicate.chain";

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::println!("pi4-net-model: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), &'static str> {
        let image = raven_rt::map_initfs().map_err(|_| "map initfs")?;
        let initfs = Initfs::parse(image).map_err(|_| "parse initfs")?;
        let good = initfs.get(GOOD).ok_or("missing good chain")?;
        let duplicate = initfs.get(DUPLICATE).ok_or("missing duplicate chain")?;

        raven_rt::println!("pi4-net-model: start");
        if !chain_ok(good.data) {
            return Err("coherent chain rejected");
        }
        raven_rt::println!("pi4-net-model: chain pass");
        if chain_ok(duplicate.data) {
            return Err("duplicate launcher accepted");
        }
        raven_rt::println!("pi4-net-model: duplicate rejected");
        raven_rt::println!("pi4-net-model: pass");
        Ok(())
    }

    fn chain_ok(data: &[u8]) -> bool {
        let Ok(text) = core::str::from_utf8(data) else {
            return false;
        };
        let mut devd = false;
        let mut genet = false;
        let mut launchers = false;
        let mut netd = false;
        let mut dtb = false;
        let mut drivers = false;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return false;
            };
            match key {
                "devd_args" => devd = value == "dtb,launch",
                "genet_driver" => genet = value == "slots=platform",
                "genet_launchers" => launchers = value == "1",
                "netd_needs" => netd = value == "scheme:nic",
                "dtb_staged" => dtb = value == "yes",
                "drivers_staged" => drivers = value == "yes",
                _ => return false,
            }
        }
        devd && genet && launchers && netd && dtb && drivers
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
