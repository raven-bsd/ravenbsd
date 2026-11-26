// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S51b `/init` fixture: read the kernel log ring, force a wrap through the `klog-test=1`
//! DEBUG_WRITE injection, observe `LOST`/`MORE`, read a fresh record and prove the `ADMIN`
//! denial path. Every printed line is deterministic for the qtest harness.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::KLOG_READ;
use raven_abi::types::Handle;
use raven_abi::types::klog::{KlogInfo, LOST, MORE};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys;

entry!(main);

const BUF_LEN: usize = 4096;
const WRITE_BODY: usize = 1024;
const WRITE_COUNT: usize = 72;

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("kernel-log: ok");
            0
        }
        Err(error) => {
            println!("kernel-log: fail {error:?}");
            1
        }
    }
}

fn run() -> Result<(), Errno> {
    let mut buffer = [0u8; BUF_LEN];
    let mut info = KlogInfo::default();

    // 1. Drain from cursor 0 and find a retained boot banner record.
    let mut cursor = 0u64;
    let mut boot = false;
    let mut guard = 0usize;
    while guard < 64 {
        let copied = klog_read(cursor, &mut buffer, &mut info)?;
        if contains(&buffer[..copied], b"RavenBSD kernel 0.1") {
            boot = true;
        }
        cursor = info.next_cursor;
        guard = guard.saturating_add(1);
        if copied == 0 {
            break;
        }
    }
    if !boot {
        return Err(Errno::ENOENT);
    }
    println!("kernel-log: boot=1");

    // 2. Write more than the 64 KiB ring capacity. `klog-test=1` routes each DEBUG_WRITE through
    // the ring as an info record, so the oldest boot records are overwritten whole.
    let mut payload = [b'w'; WRITE_BODY];
    for index in 0..WRITE_COUNT {
        for (offset, byte) in payload.iter_mut().enumerate() {
            *byte = b'a' + ((index.saturating_add(offset) % 26) as u8);
        }
        if let Some(last) = payload.last_mut() {
            *last = b'\n';
        }
        let written = sys::debug_write(&payload)?;
        if written != WRITE_BODY {
            return Err(Errno::EIO);
        }
    }

    // 3. The old cursor is now older than the first retained record; a small buffer must also
    // report MORE without consuming the first record.
    let mut small = [0u8; 16];
    let copied = klog_read(0, &mut small, &mut info)?;
    let lost = info.flags & LOST != 0;
    let more = info.flags & MORE != 0;
    if copied != 0 || !lost || !more || info.first_cursor == 0 || info.dropped == 0 {
        return Err(Errno::EIO);
    }
    println!("kernel-log: wrap dropped={} lost=1 more=1", info.dropped);

    // 4. Drain every retained record and finish exactly at the write cursor.
    let mut cursor = info.first_cursor;
    loop {
        let copied = klog_read(cursor, &mut buffer, &mut info)?;
        cursor = info.next_cursor;
        if copied == 0 {
            break;
        }
    }
    if cursor != info.last_cursor || info.flags != 0 {
        return Err(Errno::EIO);
    }
    println!("kernel-log: drain next=last=1");

    // 5. A fresh record written after the drain is returned from the caught-up cursor.
    let fresh_cursor = info.last_cursor;
    println!("kernel-log: fresh");
    let copied = klog_read(fresh_cursor, &mut buffer, &mut info)?;
    if copied == 0 || !contains(&buffer[..copied], b"kernel-log: fresh") {
        return Err(Errno::EIO);
    }
    println!("kernel-log: fresh=1");

    // 6. An oversized body is not stored and increments `dropped` without moving the cursor.
    let _ = klog_read(info.last_cursor, &mut buffer, &mut info)?;
    let oversized_cursor = info.last_cursor;
    let dropped_before = info.dropped;
    let mut oversized = [b'z'; WRITE_BODY * 2];
    if let Some(last) = oversized.last_mut() {
        *last = b'\n';
    }
    let written = sys::debug_write(&oversized)?;
    if written != oversized.len() {
        return Err(Errno::EIO);
    }
    let copied = klog_read(oversized_cursor, &mut small, &mut info)?;
    if copied != 0
        || info.dropped != dropped_before.saturating_add(1)
        || info.last_cursor != oversized_cursor
    {
        return Err(Errno::EIO);
    }
    println!("kernel-log: oversize=1");

    // 7. Without ADMIN on the root device capability the read is denied.
    sys::handle_close(Handle::ROOT_DEVICE)?;
    match klog_read(info.last_cursor, &mut buffer, &mut info) {
        Err(Errno::EACCES) => {}
        _ => return Err(Errno::EIO),
    }
    println!("kernel-log: eacces=1");
    Ok(())
}

fn klog_read(cursor: u64, buffer: &mut [u8], info: &mut KlogInfo) -> Result<usize, Errno> {
    // SAFETY: KLOG_READ takes scalar cursor/length and two pointers to live local buffers; the
    // kernel validates both user ranges and copies only complete records.
    let result = unsafe {
        sys::syscall6(
            KLOG_READ,
            cursor as usize,
            buffer.as_mut_ptr() as usize,
            buffer.len(),
            info as *mut KlogInfo as usize,
            0,
            0,
        )
    };
    Errno::from_ret(result)
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
