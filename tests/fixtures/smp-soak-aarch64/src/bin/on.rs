// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `smp_user=on` soak entry: eight user threads spread over the four QEMU `virt` CPUs.

#![no_std]
#![no_main]

use raven_rt::entry;

entry!(main);

/// Soak duration in seconds (the ticket's bounded run).
const DURATION_S: u64 = 60;

fn main(_args: raven_rt::Args) -> i32 {
    raven_smp_soak_aarch64::run(DURATION_S, true)
}
