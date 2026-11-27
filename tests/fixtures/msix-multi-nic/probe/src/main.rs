// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S199 fixture probe: wait for each sequential `nic:` service and quit it.
//!
//! `devd test` launches each matched virtio-net function one at a time. The probe waits for the
//! first function to register `nic:`, asks it to quit, waits for the name to disappear, then does
//! the same for the second function. This proves both functions reached the persistent `nic:`
//! service instead of the second one failing to attach.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
extern crate alloc;

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};

    use raven_rt::entry;
    use raven_scheme::Client;

    entry!(main);

    const OPEN_READ: u32 = 1;
    const OPEN_WRITE: u32 = 2;
    const CYCLES: u32 = 2;
    const ROUNDS: usize = 600;
    const NAP_NS: u64 = 100_000_000;

    fn main(_args: raven_rt::Args) -> i32 {
        match run() {
            Ok(()) => 0,
            Err(message) => {
                raven_rt::eprintln!("msix-probe: {message}");
                1
            }
        }
    }

    fn run() -> Result<(), String> {
        let client = Client::new();
        for cycle in 1..=CYCLES {
            wait_for_nic(&client)?;
            let handle = client
                .open("nic:quit", OPEN_WRITE)
                .map_err(|error| format!("open nic:quit: {}", error.description()))?;
            client
                .write(handle, 0, b"1")
                .map_err(|error| format!("write nic:quit: {}", error.description()))?;
            let _ = client.close(handle);
            wait_for_absent(&client)?;
            raven_rt::println!("msix-probe: cycle {cycle} quit");
        }
        Ok(())
    }

    fn wait_for_nic(client: &Client) -> Result<(), String> {
        for _ in 0..ROUNDS {
            if let Ok(handle) = client.open("nic:info", OPEN_READ) {
                let _ = client.close(handle);
                return Ok(());
            }
            let _ = raven_rt::nanosleep(NAP_NS);
        }
        Err("nic: did not appear".to_string())
    }

    fn wait_for_absent(client: &Client) -> Result<(), String> {
        for _ in 0..ROUNDS {
            if client.open("nic:info", OPEN_READ).is_err() {
                return Ok(());
            }
            let _ = raven_rt::nanosleep(NAP_NS);
        }
        Err("nic: did not disappear after quit".to_string())
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
