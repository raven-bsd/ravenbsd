// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P9c std spawn probe: the same `std::process::Command` path brush uses, without the shell.
//!
//! The libc-test service starts with an empty descriptor table, so the probe reports through the
//! exit code only: 0 when `/bin/libc-child` exits 7, 42 on a spawn/wait error, 43 on a bad status.

use std::process::Command;

fn main() {
    let status = Command::new("/bin/libc-child")
        .env("RL", "1")
        .env("RL_QUIET", "1")
        .env("RL_EXIT", "7")
        .status();
    let code = match status {
        Ok(status) => status.code(),
        Err(_) => None,
    };
    std::process::exit(if code == Some(7) { 0 } else { 42 });
}
