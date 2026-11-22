// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K529 controller: prove root CLOCK_SET and DTrace control succeed, then spawn a uid-1000
//! child that must be denied both, and prove the denied child left no DTrace record.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_capability::{
    CpuStats, DTRACE_CLEAR, DTRACE_DISABLE, DTRACE_ENABLE, DTRACE_READ, DTRACE_STATS, Record,
    SYSCALL_PROBE, clock_get, clock_set, dtrace,
};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{RtSys, SpawnOptions};

entry!(main);

const ROOT_SECONDS: i64 = 1_700_000_000;
const CHILD_BYTES: usize = 4;

fn fail(message: &str) -> i32 {
    println!("capability: fail {message}");
    1
}

fn clear_dtrace() -> Result<(), Errno> {
    match dtrace(DTRACE_CLEAR, 0, 0, 0) {
        Ok(_) => Ok(()),
        Err(Errno::EBUSY) => {
            dtrace(DTRACE_DISABLE, SYSCALL_PROBE as usize, 0, 0)?;
            dtrace(DTRACE_CLEAR, 0, 0, 0).map(|_| ())
        }
        Err(error) => Err(error),
    }
}

fn root_clock() -> Result<(), &'static str> {
    clock_set(ROOT_SECONDS, 0).map_err(|_| "root clock rejected")?;
    let value = clock_get().map_err(|_| "root clock read")?;
    // The wall clock advances between set and get; the whole seconds must match the target.
    if value.tv_sec != ROOT_SECONDS {
        println!(
            "capability: root clock value {}.{}",
            value.tv_sec, value.tv_nsec
        );
        return Err("root clock readback");
    }
    println!("capability: root clock ok");
    Ok(())
}

fn spawn_child(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let options = SpawnOptions {
        args: &[] as &[&str],
        env: &[] as &[&str],
    };
    let child = raven_rt::spawn::spawn(initfs, "/child", &options).map_err(|_| "spawn child")?;
    let mut code = 1;
    RtSys
        .proc_wait(child, &mut code)
        .map_err(|_| "wait child")?;
    if code != 0 {
        println!("capability: child exit {code}");
        return Err("child exit");
    }
    Ok(())
}

fn no_record_leak() -> Result<(), &'static str> {
    let mut records = [Record::default(); CHILD_BYTES];
    let read = dtrace(
        DTRACE_READ,
        records.as_mut_ptr() as usize,
        records.len(),
        0,
    )
    .map_err(|_| "leak read rejected")?;
    let mut stats = [CpuStats::default(); 8];
    let cpus = dtrace(
        DTRACE_STATS,
        stats.as_mut_ptr() as usize,
        stats.len(),
        0,
    )
    .map_err(|_| "leak stats rejected")?;
    let produced: u64 = stats
        .iter()
        .take(cpus)
        .map(|entry| entry.produced)
        .sum();
    if read != 0 || produced != 0 {
        println!("capability: leak read={read} produced={produced}");
        return Err("record leak");
    }
    println!("capability: no record leak ok");
    Ok(())
}

fn root_dtrace() -> Result<(), &'static str> {
    let credentials = raven_rt::proc::credentials().map_err(|_| "root credentials")?;
    if credentials.euid != 0 {
        println!("capability: root euid {}", credentials.euid);
        return Err("root euid");
    }
    if let Err(error) = dtrace(
        DTRACE_ENABLE,
        SYSCALL_PROBE as usize,
        0,
        raven_abi::types::Handle::NONE.0 as usize,
    ) {
        println!("capability: fail root enable {}", error.0);
        return Err("root enable rejected");
    }
    // A non-control syscall produces one record for the enabled syscall probe.
    let _ = clock_get();
    let mut records = [Record::default(); 8];
    let read = match dtrace(
        DTRACE_READ,
        records.as_mut_ptr() as usize,
        records.len(),
        0,
    ) {
        Ok(read) => read,
        Err(error) => {
            println!("capability: fail root read {}", error.0);
            return Err("root read rejected");
        }
    };
    if let Err(error) = dtrace(DTRACE_DISABLE, SYSCALL_PROBE as usize, 0, 0) {
        println!("capability: fail root disable {}", error.0);
        return Err("root disable rejected");
    }
    if let Err(error) = dtrace(DTRACE_CLEAR, 0, 0, 0) {
        println!("capability: fail root clear {}", error.0);
        return Err("root clear rejected");
    }
    if read == 0 {
        return Err("root read empty");
    }
    println!("capability: root dtrace ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    if let Err(message) = root_clock() {
        return fail(message);
    }
    if let Err(error) = clear_dtrace() {
        println!("capability: fail clear {}", error.0);
        return 1;
    }
    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => return fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail("initfs parse"),
    };
    if let Err(message) = spawn_child(&initfs) {
        return fail(message);
    }
    if let Err(message) = no_record_leak() {
        return fail(message);
    }
    if let Err(message) = root_dtrace() {
        return fail(message);
    }
    println!("capability: ok");
    0
}
