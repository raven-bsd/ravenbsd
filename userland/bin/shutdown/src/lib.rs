// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared implementation of `shutdown`, `reboot` and `poweroff`.
//!
//! The commands ask `svcd` to stop every service except the live getty session, flush file
//! systems, then call the root-only `PROC_REBOOT` syscall. The kernel owns the final platform
//! power/reset action; this crate never fakes it.

#![cfg_attr(target_os = "none", no_std)]
#![allow(unsafe_code)]

extern crate alloc;

/// One requested power action.
#[cfg(target_os = "none")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Normal reboot (`RB_AUTOBOOT`).
    Reboot,
    /// Halt without power (`RB_HALT`).
    Halt,
    /// Power off (`RB_POWER_OFF`).
    PowerOff,
}

#[cfg(target_os = "none")]
mod program {
    use alloc::format;
    use alloc::string::{String, ToString};
    use alloc::vec::Vec;

    use raven_abi::errno::Errno;
    use raven_proto_fs::DirEntryIter;
    use raven_proto_svc::{Op, State, Status};
    use raven_scheme::Client;

    use crate::Kind;

    const PROC_REBOOT: usize = 0x46;
    const RB_AUTOBOOT: usize = 0;
    const RB_HALT: usize = 0x0000_0008;
    const RB_POWER_OFF: usize = 0x0000_4000;
    const WAIT_STEPS: usize = 200;
    const WAIT_NS: u64 = 10_000_000;

    /// Parse and run `shutdown [-r|-h|-p] now`.
    pub fn run_shutdown(argv: &[&str]) -> i32 {
        let arguments = argv.get(1..).unwrap_or(&[]);
        let kind = match arguments {
            ["-r", "now"] => Kind::Reboot,
            ["-h", "now"] => Kind::Halt,
            ["-p", "now"] => Kind::PowerOff,
            _ => {
                raven_rt::eprintln!("usage: shutdown -r|-h|-p now");
                return 2;
            }
        };
        run_kind(kind)
    }

    /// Run the `reboot` command.
    pub fn run_reboot() -> i32 {
        run_kind(Kind::Reboot)
    }

    /// Run the `poweroff` command.
    pub fn run_poweroff() -> i32 {
        run_kind(Kind::PowerOff)
    }

    fn run_kind(kind: Kind) -> i32 {
        let credentials = match raven_rt::proc::credentials() {
            Ok(credentials) => credentials,
            Err(error) => {
                raven_rt::eprintln!("shutdown: credentials: {}", error.description());
                return 1;
            }
        };
        if credentials.euid != 0 {
            raven_rt::eprintln!("shutdown: operation not permitted");
            return 1;
        }

        raven_rt::println!("shutdown: stopping services");
        match stop_services() {
            Ok(count) => raven_rt::println!("shutdown: {count} services stopped"),
            Err(error) => raven_rt::eprintln!("shutdown: service stop: {error}"),
        }
        if let Err(error) = raven_rt::fs::sync() {
            raven_rt::eprintln!("shutdown: sync: {}", error.description());
        }

        let (name, how) = match kind {
            Kind::Reboot => ("reboot", RB_AUTOBOOT),
            Kind::Halt => ("halt", RB_HALT),
            Kind::PowerOff => ("poweroff", RB_POWER_OFF),
        };
        raven_rt::println!("shutdown: {name}");
        // SAFETY: PROC_REBOOT takes one scalar flag word. The kernel checks euid, validates the
        // mask and never returns on success; an error is reported below.
        let result = unsafe { raven_rt::sys::syscall1(PROC_REBOOT, how) };
        match Errno::from_ret(result) {
            Ok(_) => {
                raven_rt::eprintln!("shutdown: kernel returned without a power action");
                1
            }
            Err(error) => {
                raven_rt::eprintln!("shutdown: {}", error.description());
                1
            }
        }
    }

    fn stop_services() -> Result<usize, String> {
        let client = Client::new();
        let handle = match client.open("svc:", 1) {
            Ok(handle) => handle,
            Err(error) if error == Errno::ENOENT || error == Errno::EAFNOSUPPORT => return Ok(0),
            Err(error) => return Err(error.description().to_string()),
        };
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = client
                .read(handle, 0, &mut buffer)
                .map_err(|error| error.description().to_string())?;
            if read == 0 {
                break;
            }
            let chunk = buffer.get(..read).ok_or_else(|| "short read".to_string())?;
            data.extend_from_slice(chunk);
        }
        let _ = client.close(handle);

        let mut names = Vec::new();
        for entry in DirEntryIter::new(&data) {
            let entry = entry.map_err(|error| format!("directory: {error}"))?;
            let name = core::str::from_utf8(entry.name)
                .map_err(|_| "service name is not UTF-8".to_string())?;
            names.push(name.to_string());
        }

        let mut stopped = 0usize;
        for name in &names {
            // The live getty owns the shell session running this command. The kernel reset/power
            // path terminates it after the other services have stopped.
            if name == "getty" {
                continue;
            }
            if stop_service(&client, name).is_ok() {
                stopped = stopped.saturating_add(1);
            }
        }
        Ok(stopped)
    }

    fn stop_service(client: &Client, name: &str) -> Result<(), String> {
        let path = alloc::format!("svc:/{name}");
        let handle = client
            .open(&path, 1)
            .map_err(|error| error.description().to_string())?;
        let mut response = [0u8; 1];
        let result = client.call(handle, &[Op::Stop.as_u8()], &mut response);
        let _ = client.close(handle);
        result.map_err(|error| error.description().to_string())?;

        let mut index = 0usize;
        while index < WAIT_STEPS {
            let _ = raven_rt::nanosleep(WAIT_NS);
            if service_state(client, name).is_some_and(|state| state == State::Stopped) {
                return Ok(());
            }
            index = index.saturating_add(1);
        }
        Err(alloc::format!("{name}: timed out waiting for stop"))
    }

    fn service_state(client: &Client, name: &str) -> Option<State> {
        let path = alloc::format!("svc:/{name}");
        let handle = client.open(&path, 1).ok()?;
        let mut buffer = [0u8; 128];
        let read = client.read(handle, 0, &mut buffer).ok()?;
        let _ = client.close(handle);
        let line = buffer.get(..read)?;
        Status::decode(line).ok().map(|status| status.state)
    }
}

#[cfg(target_os = "none")]
pub use program::{run_poweroff, run_reboot, run_shutdown};

#[cfg(not(target_os = "none"))]
pub use host::{run_poweroff, run_reboot, run_shutdown};

#[cfg(not(target_os = "none"))]
mod host {
    /// Host stub for CLI tests.
    pub fn run_shutdown(_argv: &[&str]) -> i32 {
        1
    }

    /// Host stub for CLI tests.
    pub fn run_reboot() -> i32 {
        1
    }

    /// Host stub for CLI tests.
    pub fn run_poweroff() -> i32 {
        1
    }
}
