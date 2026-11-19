// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K147 client: null CALL, 4 KiB PREAD, 64 KiB PREAD and 1 MiB read through one scheme.

#![no_std]
#![no_main]
#![allow(unsafe_code)] // The benchmark buffer is a private anonymous mapping.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_rt::entry;
use raven_rt::println;
use raven_s11_echo::{O_RDWR, call, close, map_anonymous, nanosleep, open, read, write};
use raven_scheme_call_cost::{now_ns, pread, report};

entry!(main);

const SAMPLES: usize = 128;
const BIG_SAMPLES: usize = 100;
const TRANSFER: usize = 1024 * 1024;
const FOUR_K: usize = 4096;
const SIXTY_FOUR_K: usize = 64 * 1024;
const RETRIES: usize = 2000;
// Loaded-host baseline (load 25.9): null 28.7 us, 4K 83.6 us, 64K 122 us, 1M 1.87 ms p50.
// The thresholds are ~6-9x the measured fixed cost so the check catches the 2 ms/4K storm.
const NULL_MAX_NS: u64 = 250_000;
const FOUR_K_MAX_NS: u64 = 500_000;
const SIXTY_FOUR_K_MAX_NS: u64 = 1_000_000;
const ONE_MIB_MAX_NS: u64 = 10_000_000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn fail(message: &str) -> i32 {
    println!("scheme-call-cost: fail {}", message);
    1
}

fn main(_args: raven_rt::Args) -> i32 {
    let handle = match open_retry("bench:test") {
        Ok(handle) => handle,
        Err(_) => return fail("client open"),
    };
    let pointer = match map_anonymous(TRANSFER) {
        Ok(pointer) => pointer,
        Err(_) => return fail("client map"),
    };
    // SAFETY: `pointer` is a private anonymous mapping of `TRANSFER` bytes in this process.
    let bytes = unsafe { core::slice::from_raw_parts_mut(pointer, TRANSFER) };
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = (index as u8) ^ 0x5a;
    }
    if write(handle, bytes).ok() != Some(TRANSFER) {
        return fail("client setup write");
    }

    for _ in 0..8 {
        if call(handle, b"", None).is_err() {
            return fail("client warmup call");
        }
        if pread(handle, &mut bytes[..FOUR_K], 0).ok() != Some(FOUR_K) {
            return fail("client warmup pread4k");
        }
        if pread(handle, &mut bytes[..SIXTY_FOUR_K], 0).ok() != Some(SIXTY_FOUR_K) {
            return fail("client warmup pread64k");
        }
        if read(handle, bytes).ok() != Some(TRANSFER) {
            return fail("client warmup read1m");
        }
    }

    let mut null = [0u64; SAMPLES];
    for slot in null.iter_mut() {
        let start = now_ns();
        if call(handle, b"", None).is_err() {
            return fail("client null call");
        }
        *slot = now_ns().saturating_sub(start);
    }
    if report("null", &mut null) > NULL_MAX_NS {
        return fail("null threshold");
    }

    let mut four_k = [0u64; SAMPLES];
    for slot in four_k.iter_mut() {
        let start = now_ns();
        if pread(handle, &mut bytes[..FOUR_K], 0).ok() != Some(FOUR_K) {
            return fail("client pread4k");
        }
        *slot = now_ns().saturating_sub(start);
    }
    if report("pread4k", &mut four_k) > FOUR_K_MAX_NS {
        return fail("pread4k threshold");
    }

    let mut sixty_four_k = [0u64; SAMPLES];
    for slot in sixty_four_k.iter_mut() {
        let start = now_ns();
        if pread(handle, &mut bytes[..SIXTY_FOUR_K], 0).ok() != Some(SIXTY_FOUR_K) {
            return fail("client pread64k");
        }
        *slot = now_ns().saturating_sub(start);
    }
    if report("pread64k", &mut sixty_four_k) > SIXTY_FOUR_K_MAX_NS {
        return fail("pread64k threshold");
    }

    let mut one_mib = [0u64; BIG_SAMPLES];
    for slot in one_mib.iter_mut() {
        let start = now_ns();
        if read(handle, bytes).ok() != Some(TRANSFER) {
            return fail("client read1m");
        }
        *slot = now_ns().saturating_sub(start);
    }
    if report("read1m", &mut one_mib) > ONE_MIB_MAX_NS {
        return fail("read1m threshold");
    }

    if bytes[0] != 0x5a || bytes[TRANSFER - 1] != ((TRANSFER - 1) as u8 ^ 0x5a) {
        return fail("client data mismatch");
    }
    let _ = close(handle);
    println!("scheme-call-cost: done");
    0
}
