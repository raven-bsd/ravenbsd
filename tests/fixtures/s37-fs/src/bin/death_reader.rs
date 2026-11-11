// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S37 death reader: a bare ELF entry that receives a transferred SchemeFile handle in `rdi`.
//! It reads the unlinked data, closes the last reference and exits.

#![no_std]
#![no_main]

use raven_abi::types::Handle;
use raven_s37_fs::{close, exit, print_line, read};

/// Bare thread entry; no `raven-rt` startup stack is needed.
#[unsafe(no_mangle)]
pub extern "C" fn _start(handle: usize) -> ! {
    let handle = Handle(u32::try_from(handle).unwrap_or(u32::MAX));
    let mut buffer = [0u8; 32];
    match read(handle, &mut buffer) {
        Ok(count) if count == 13 && &buffer[..13] == b"unlinked-data" => {
            print_line("s37: death reader ok");
        }
        Ok(_) => print_line("s37: death reader bad data"),
        Err(_) => print_line("s37: death reader read failed"),
    }
    let _ = close(handle);
    print_line("s37: death reader closed");
    exit(0)
}
