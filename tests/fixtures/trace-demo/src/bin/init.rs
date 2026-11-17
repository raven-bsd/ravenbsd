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
    PAGE, Staged, handle_close, memobj_create, nanosleep, proc_wait, stage, start, transfer,
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
    for attempt in 0..OPEN_RETRIES {
        match raven_rt::sys::open(path, O_READ) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) | Err(Errno::EIO) if attempt + 1 < OPEN_RETRIES => {
                nanosleep(25_000_000);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ENOENT)
}

fn one_call(path: &str, expected: u8) -> Result<(), &'static str> {
    let file = open_retry(path).map_err(|_| "open")?;
    let mut byte = [0u8; 1];
    match raven_rt::sys::read(file, &mut byte) {
        Ok(1) if byte[0] == expected => {}
        _ => return Err("read"),
    }
    let _ = raven_rt::sys::close(file);
    Ok(())
}

fn start_with_sink(initfs: &Initfs<'_>, path: &str, sink: Handle) -> Result<Staged, &'static str> {
    let staged = stage(initfs, path).map_err(|_| "real server stage")?;
    transfer(
        staged.process,
        sink,
        Rights::MAP.union(Rights::READ).union(Rights::WRITE),
    )
    .map_err(|_| "real server transfer")?;
    start(&staged).map_err(|_| "real server start")?;
    Ok(staged)
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

    // Start the real block stack: initfsd serves /etc/blockd.img, blockd the GPT disk, and
    // fsd-ufs mounts block:0 and serves ufs:.
    let initfsd = match stage(&initfs, "/initfsd") {
        Ok(staged) => staged,
        Err(_) => return fail("initfsd stage"),
    };
    if start(&initfsd).is_err() {
        return fail("initfsd start");
    }
    nanosleep(100_000_000);
    // Wait for initfsd to serve the disk image before blockd opens it.
    match open_retry("initfs:/etc/blockd.img") {
        Ok(handle) => {
            let _ = raven_rt::sys::close(handle);
        }
        Err(_) => return fail("initfsd ready"),
    }
    let blockd = match start_with_sink(&initfs, "/blockd", sink) {
        Ok(staged) => staged,
        Err(message) => return fail(message),
    };
    nanosleep(300_000_000);
    // Wait for blockd to publish block:0 before fsd-ufs mounts it.
    match open_retry("block:0") {
        Ok(handle) => {
            let _ = raven_rt::sys::close(handle);
        }
        Err(_) => return fail("blockd ready"),
    }
    let fsd = match start_with_sink(&initfs, "/fsd-ufs", sink) {
        Ok(staged) => staged,
        Err(message) => return fail(message),
    };
    nanosleep(300_000_000);

    // Fixture-server proof: one traced and one untraced request.
    if trace_set(TRACE_ID).is_err() {
        return fail("trace_set");
    }
    if let Err(message) = one_call("trace-demo:probe", b'R') {
        return fail(message);
    }
    if trace_set(0).is_err() {
        return fail("trace_clear");
    }
    if let Err(message) = one_call("trace-demo:probe", b'R') {
        return fail(message);
    }

    // Real read: client -> fsd-ufs -> blockd, then the same path untraced.
    if trace_set(TRACE_ID).is_err() {
        return fail("trace_set");
    }
    if let Err(message) = one_call("ufs:/ten.txt", b'0') {
        return fail(message);
    }
    if trace_set(0).is_err() {
        return fail("trace_clear");
    }
    if let Err(message) = one_call("ufs:/ten.txt", b'0') {
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

    for server in [&fsd, &blockd, &initfsd] {
        let _ = raven_rt::proc_terminate(server.process, 0);
        let _ = proc_wait(server.process);
        handle_close(server.process);
    }
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
