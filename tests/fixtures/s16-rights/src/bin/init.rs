// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S16 controller: same-process and cross-process revocation, prefix visibility/removal, exact
//! names, and a 10,000-handle churn that must return the lineage counter to baseline.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, map, namespace, prot};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_s16_rights::{
    PAGE, close, dup, handle_close, map_initfs_image, map_memory, memobj_create,
    namespace as set_namespace, nanosleep, open, proc_terminate, proc_wait, revoke, spawn, stage,
    start, transfer, unmap,
};

entry!(main);

/// Churn cycles from the ticket.
const CYCLES: usize = 10_000;
/// How long the controller waits before removing the prefix (the child sleeps longer).
const PREFIX_HOLD_NS: u64 = 250_000_000;
/// Open retry budget for the server registration.
const OPEN_RETRIES: usize = 2000;

fn fail(message: &str) -> i32 {
    println!("s16: failed {message}");
    1
}

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < OPEN_RETRIES {
        match open(path, raven_s16_rights::O_READ) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn map_error(handle: Handle) -> Result<(), Errno> {
    map_memory(
        Handle::SELF_PROC,
        handle,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    )
    .map(|_| ())
}

fn revoke_same_process() -> Result<(), &'static str> {
    let object = memobj_create(PAGE).map_err(|_| "same memobj")?;
    let mapped = map_memory(
        Handle::SELF_PROC,
        object,
        0,
        0,
        PAGE,
        map::pack(prot::READ, 0),
    )
    .map_err(|_| "same map")?;
    unmap(Handle::SELF_PROC, mapped, PAGE).map_err(|_| "same unmap")?;
    let child = dup(object, Rights::MAP.union(Rights::DUP)).map_err(|_| "same dup")?;
    revoke(object, Rights::MAP).map_err(|_| "same revoke")?;
    if map_error(object) != Err(Errno::EPERM) {
        return Err("parent MAP not EPERM");
    }
    if map_error(child) != Err(Errno::EPERM) {
        return Err("child MAP not EPERM");
    }
    let leaf = dup(child, Rights::DUP.union(Rights::READ)).map_err(|_| "leaf dup")?;
    if map_error(leaf) != Err(Errno::EACCES) {
        return Err("leaf MAP not EACCES");
    }
    revoke(object, Rights::MAP).map_err(|_| "second revoke")?;
    handle_close(leaf);
    handle_close(child);
    handle_close(object);
    if revoke(object, Rights::MAP) != Err(Errno::EBADF) {
        return Err("stale revoke not EBADF");
    }
    println!("s16: revoke same ok");
    Ok(())
}

fn revoke_cross_process(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let object = memobj_create(PAGE).map_err(|_| "cross memobj")?;
    let staged = stage(initfs, "/revoke").map_err(|_| "cross stage")?;
    let granted = transfer(
        staged.process,
        object,
        Rights::READ.union(Rights::MAP).union(Rights::DUP),
    )
    .map_err(|_| "cross transfer")?;
    if granted.index() != 3 {
        println!("s16: cross grant index {}", granted.index());
        return Err("cross grant index");
    }
    revoke(object, Rights::MAP).map_err(|_| "cross revoke")?;
    start(&staged).map_err(|_| "cross start")?;
    let code = proc_wait(staged.process).map_err(|_| "cross wait")?;
    handle_close(staged.process);
    handle_close(object);
    if code != 0 {
        println!("s16: cross exit {code}");
        return Err("cross client exit");
    }
    println!("s16: revoke cross ok");
    Ok(())
}

fn prefix_case(initfs: &Initfs<'_>) -> Result<Handle, &'static str> {
    // Stage both children before the server registers, so their snapshots contain no exact
    // scheme names; only the granted prefixes can reach the servers.
    let remove_child = stage(initfs, "/prefix").map_err(|_| "prefix stage")?;
    set_namespace(remove_child.process, namespace::ADD, b"s16a").map_err(|_| "prefix add")?;
    let clear_child = stage(initfs, "/clear").map_err(|_| "clear stage")?;
    set_namespace(clear_child.process, namespace::ADD, b"s16a").map_err(|_| "clear add s16a")?;
    set_namespace(clear_child.process, namespace::ADD, b"other").map_err(|_| "clear add other")?;

    let server = spawn(initfs, "/server").map_err(|_| "server spawn")?;
    let probe = open_retry("s16a:probe").map_err(|_| "server probe")?;
    let _ = close(probe);

    start(&remove_child).map_err(|_| "prefix start")?;
    nanosleep(PREFIX_HOLD_NS);
    set_namespace(remove_child.process, namespace::REMOVE, b"s16a").map_err(|_| "prefix remove")?;
    let code = proc_wait(remove_child.process).map_err(|_| "prefix wait")?;
    handle_close(remove_child.process);
    if code != 0 {
        println!("s16: prefix exit {code}");
        return Err("prefix client exit");
    }
    println!("s16: prefix case ok");

    start(&clear_child).map_err(|_| "clear start")?;
    nanosleep(PREFIX_HOLD_NS);
    set_namespace(clear_child.process, namespace::CLEAR, b"").map_err(|_| "prefix clear")?;
    let code = proc_wait(clear_child.process).map_err(|_| "clear wait")?;
    handle_close(clear_child.process);
    if code != 0 {
        println!("s16: clear exit {code}");
        return Err("clear client exit");
    }
    println!("s16: clear case ok");
    Ok(server)
}

fn exact_case(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let child = spawn(initfs, "/exact").map_err(|_| "exact spawn")?;
    let code = proc_wait(child).map_err(|_| "exact wait")?;
    handle_close(child);
    if code != 0 {
        println!("s16: exact exit {code}");
        return Err("exact client exit");
    }
    println!("s16: exact case ok");
    Ok(())
}

fn churn() -> Result<(), &'static str> {
    let mut cycle = 0usize;
    while cycle < CYCLES {
        let object = memobj_create(PAGE).map_err(|_| "churn memobj")?;
        let child = dup(object, Rights::MAP.union(Rights::DUP)).map_err(|_| "churn dup")?;
        revoke(object, Rights::MAP).map_err(|_| "churn revoke")?;
        revoke(object, Rights::MAP).map_err(|_| "churn second revoke")?;
        handle_close(child);
        handle_close(object);
        cycle = cycle.saturating_add(1);
    }
    println!("s16: churn done={CYCLES}");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => return fail("initfs map"),
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => return fail("initfs parse"),
    };

    if let Err(message) = revoke_same_process() {
        return fail(message);
    }
    if let Err(message) = revoke_cross_process(&initfs) {
        return fail(message);
    }
    let server = match prefix_case(&initfs) {
        Ok(server) => server,
        Err(message) => return fail(message),
    };
    if let Err(message) = exact_case(&initfs) {
        return fail(message);
    }
    if let Err(message) = churn() {
        return fail(message);
    }

    if proc_terminate(server, 0).is_err() {
        return fail("server terminate");
    }
    match proc_wait(server) {
        Ok(0) => {}
        _ => return fail("server wait"),
    }
    handle_close(server);

    println!("s16: rights ok");
    println!("s16: namespace ok");
    println!("s16: lifecycle ok");
    0
}
