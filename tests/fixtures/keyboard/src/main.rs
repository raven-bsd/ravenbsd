// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S59 `keyboard` fixture.
//!
//! Waits for the bytes injected through the QEMU monitor (`sendkey`) to arrive through the
//! i8042 IRQ1 path and the shared console queue. The serial receiver uses the same queue, so a
//! successful run proves the keyboard feed reaches an ordinary console reader.

#![no_std]
#![no_main]

use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::debug_read;

entry!(main);

/// One read timeout; the helper injects the keys about eight seconds after the QMP connection.
const READ_TIMEOUT_NS: u64 = 2_000_000_000;
/// Read attempts before the fixture gives up (each blocks for the timeout).
const ATTEMPTS: u32 = 20;

fn main(_args: raven_rt::Args) -> i32 {
    println!("keyboard: ready");
    let mut buffer = [0u8; 32];
    let mut got_h = false;
    let mut got_i = false;
    let mut got_enter = false;
    let mut attempt = 0u32;
    while attempt < ATTEMPTS {
        attempt = attempt.saturating_add(1);
        let count = match debug_read(&mut buffer, READ_TIMEOUT_NS) {
            Ok(count) => count,
            Err(error) => {
                println!("keyboard: fail read {}", error.0);
                return 1;
            }
        };
        if count == 0 {
            continue;
        }
        for byte in buffer.iter().take(count).copied() {
            match byte {
                b'h' => {
                    got_h = true;
                    println!("keyboard: got h");
                }
                b'i' => {
                    got_i = true;
                    println!("keyboard: got i");
                }
                b'\n' | b'\r' => {
                    got_enter = true;
                    println!("keyboard: got enter");
                }
                other => {
                    println!("keyboard: byte 0x{:02x}", other);
                }
            }
        }
        if got_h && got_i && got_enter {
            println!("keyboard: ok");
            return 0;
        }
    }
    println!("keyboard: fail sequence");
    1
}
