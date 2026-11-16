// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S58 `dtrace-user` fixture.
//!
//! Drives the userland consumer end to end without libc:
//!
//! 1. `DTRACE_PROBE_LIST` through `Consumer` lists the S57 `syscall::entry` probe.
//! 2. Enable, generate records, `DTRACE_READ`, format one record and verify its probe/syscall.
//! 3. A no-consumer burst wraps a ring; the per-CPU stats and the drained records agree.
//! 4. The USDT marker helper reports the `raven`/`usdt-test` site and a `nop` opcode.

#![no_std]
#![no_main]

use core::fmt;

use raven_abi::syscall::THREAD_SELF;
use raven_dtrace_user::abi::provider;
use raven_dtrace_user::sys::SyscallKernel;
use raven_dtrace_user::{
    Consumer, CpuStats, ProbeInfo, Record, USDT_PROBES, format_record, site_opcode,
};
use raven_rt::entry;
use raven_rt::println;
use raven_rt::sys::syscall0;

entry!(main);

const PROBE: u32 = 1;
const THREAD_SELF_NR: u32 = THREAD_SELF as u32;
const RAPID_RECORDS: usize = 5_000;
const RING_CAPACITY: u64 = 4_096;
const MIN_DROPPED: u64 = 900;
const READ_BATCH: usize = 32;

struct Writer;

impl fmt::Write for Writer {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        raven_rt::print!("{text}");
        Ok(())
    }
}

fn fail(message: &str) -> i32 {
    println!("dtrace-user: fail {message}");
    1
}

fn self_syscall() {
    // SAFETY: THREAD_SELF takes no arguments and returns the calling thread id.
    let _ = unsafe { syscall0(THREAD_SELF) };
}

fn main(_args: raven_rt::Args) -> i32 {
    let consumer = Consumer::new(SyscallKernel);

    // 1. Registry.
    let mut probes = [ProbeInfo::default(); 4];
    let count = match consumer.list_probes(&mut probes) {
        Ok(count) => count,
        Err(error) => return fail(error.description()),
    };
    if count != 1 || probes[0].id != PROBE || provider(&probes[0]) != "syscall" {
        return fail("probe-list");
    }
    println!("dtrace-user: probe id=1 provider=syscall");

    // 2. One enabled probe, records read and formatted through the consumer.
    if consumer.clear().is_err() || consumer.enable(PROBE).is_err() {
        return fail("enable");
    }
    for _ in 0..10 {
        self_syscall();
    }
    let mut records = [Record::default(); READ_BATCH];
    let count = match consumer.read(&mut records) {
        Ok(count) => count,
        Err(error) => return fail(error.description()),
    };
    if count < 10 {
        return fail("record count");
    }
    if records[0].probe != PROBE as u16 || records[0].syscall != THREAD_SELF_NR {
        return fail("record contents");
    }
    if format_record(&records[0], &mut Writer).is_err() {
        return fail("format");
    }
    println!();
    if consumer.disable(PROBE).is_err() || consumer.clear().is_err() {
        return fail("reset");
    }

    // 3. Ring wrap: no consumer during the burst, then drain the retained records.
    if consumer.enable(PROBE).is_err() {
        return fail("wrap enable");
    }
    for _ in 0..RAPID_RECORDS {
        self_syscall();
    }
    if consumer.disable(PROBE).is_err() {
        return fail("wrap disable");
    }
    let mut stats = [CpuStats::default(); 8];
    let cpus = match consumer.stats(&mut stats) {
        Ok(cpus) => cpus,
        Err(error) => return fail(error.description()),
    };
    let mut produced = 0u64;
    let mut dropped = 0u64;
    for entry in stats.iter().take(cpus.min(stats.len())) {
        produced = produced.saturating_add(entry.produced);
        dropped = dropped.saturating_add(entry.dropped);
    }
    if produced != RING_CAPACITY || dropped < MIN_DROPPED {
        return fail("wrap stats");
    }
    let mut retained = 0usize;
    loop {
        let count = match consumer.read(&mut records) {
            Ok(count) => count,
            Err(error) => return fail(error.description()),
        };
        if count == 0 {
            break;
        }
        retained = retained.saturating_add(count);
    }
    if retained as u64 != RING_CAPACITY {
        return fail("wrap retained");
    }
    println!(
        "dtrace-user: wrap produced={} retained={} dropped={}",
        produced, retained, dropped
    );
    if consumer.clear().is_err() {
        return fail("wrap clear");
    }

    // 4. USDT marker and descriptor.
    if USDT_PROBES.len() != 1 {
        return fail("usdt count");
    }
    let probe = &USDT_PROBES[0];
    if probe.provider != "raven" || probe.name != "usdt-test" || site_opcode() != 0x90 {
        return fail("usdt marker");
    }
    println!(
        "dtrace-user: usdt provider={} name={} opcode=0x90",
        probe.provider, probe.name
    );

    println!("dtrace-user: ok");
    0
}
