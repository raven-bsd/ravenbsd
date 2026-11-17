// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Q4b trace-demo controller: create the shared sink, run a traced server call, then an untraced
//! one, and run the `trace` command to print the recorded tree.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::syscall::TRACE_SET;
use raven_abi::types::{Handle, Rights};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_trace_demo::{
    PAGE, handle_close, memobj_create, nanosleep, proc_wait, stage, start, transfer,
};

entry!(main);

/// Open flags: read only.
const O_READ: u32 = 1;
/// The non-zero trace id the controller sets for the traced requests.
const TRACE_ID: u64 = 0xDEAD_BEEF_CAFE_F00D;
/// Open retry budget for the server registration.
const OPEN_RETRIES: usize = 200_000;

fn fail(message: &str) -> i32 {
    println!("demo: failed {message}");
    1
}

/// `TRACE_SET`: set the calling thread's current trace id (zero clears it).
fn trace_set(id: u64) -> Result<(), Errno> {
    // SAFETY: TRACE_SET takes one scalar argument.
    Errno::from_ret(unsafe { raven_rt::sys::syscall1(TRACE_SET, id as usize) }).map(|_| ())
}

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    loop {
        match raven_rt::sys::open(path, O_READ) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) | Err(Errno::EIO) if attempt < OPEN_RETRIES => {
                core::hint::spin_loop();
                attempt += 1;
            }
            Err(error) => return Err(error),
        }
    }
}

fn one_call() -> Result<(), &'static str> {
    let file = open_retry("trace-demo:probe").map_err(|_| "open")?;
    let mut byte = [0u8; 1];
    match raven_rt::sys::read(file, &mut byte) {
        Ok(1) if byte[0] == b'R' => {}
        _ => return Err("read"),
    }
    let _ = raven_rt::sys::close(file);
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match raven_rt::map_initfs() {
        Ok(image) => image,
        Err(_) => return fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail("initfs parse"),
    };

    // Create and initialise the shared sink through the runtime API.
    let sink = match memobj_create(raven_rt::trace::SINK_PAGES * PAGE) {
        Ok(sink) => sink,
        Err(_) => return fail("sink memobj"),
    };
    let root = raven_rt::trace::Sink::attach(sink, 0);
    if !root.enabled() {
        return fail("sink attach");
    }

    // Stage and start the fixture server with the sink transferred.
    let staged = match stage(&initfs, "/server") {
        Ok(staged) => staged,
        Err(_) => return fail("server stage"),
    };
    if transfer(
        staged.process,
        sink,
        Rights::MAP.union(Rights::READ).union(Rights::WRITE),
    )
    .is_err()
    {
        return fail("server transfer");
    }
    if start(&staged).is_err() {
        return fail("server start");
    }
    nanosleep(50_000_000);

    // A traced request, then an untraced one.
    if trace_set(TRACE_ID).is_err() {
        return fail("trace_set");
    }
    if let Err(message) = one_call() {
        return fail(message);
    }
    if trace_set(0).is_err() {
        return fail("trace_clear");
    }
    if let Err(message) = one_call() {
        return fail(message);
    }

    // Stage and run the trace command with the sink transferred.
    let tracer = match stage(&initfs, "/trace") {
        Ok(staged) => staged,
        Err(_) => return fail("trace stage"),
    };
    if transfer(
        tracer.process,
        sink,
        Rights::MAP.union(Rights::READ).union(Rights::WRITE),
    )
    .is_err()
    {
        return fail("trace transfer");
    }
    if start(&tracer).is_err() {
        return fail("trace start");
    }
    match proc_wait(tracer.process) {
        Ok(0) => {}
        _ => return fail("trace wait"),
    }
    handle_close(tracer.process);

    let _ = raven_rt::proc_terminate(staged.process, 0);
    match proc_wait(staged.process) {
        Ok(0) => {}
        _ => return fail("server wait"),
    }
    handle_close(staged.process);
    handle_close(sink);

    println!("demo: ok");
    0
}
