// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The thin `raven` host binary. All command logic lives in `raven-cli`.
//!
//! K74 is a host-only prototype. The target entry point is a compile-time stub that reports the
//! missing target wiring and exits 1; K78 replaces it together with the profile/initfs wiring.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(not(target_os = "none"))]
fn main() {
    use std::io::IsTerminal;

    let mut stdout = std::io::stdout();
    let mut stderr = std::io::stderr();
    let mut io = raven_cli::Io {
        stdout: &mut stdout,
        stderr: &mut stderr,
        is_tty: std::io::stdout().is_terminal(),
        no_color: std::env::var_os("NO_COLOR").is_some(),
    };
    let exit = raven_cli::run(std::env::args_os().skip(1), &mut io);
    std::process::exit(exit.code());
}

#[cfg(target_os = "none")]
mod target {
    use raven_rt::entry;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        raven_rt::eprintln!("raven: target wiring arrives in K78");
        1
    }
}
