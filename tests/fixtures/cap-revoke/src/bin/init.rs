// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S73 controller: transfer a reduced-rights capability to a child, let it use it, revoke the
//! lineage (child sees EPERM), and prove the object is released only after the last live handle
//! with no stale-handle reuse.

#![no_std]
#![no_main]

use raven_abi::types::Rights;
use raven_cap_revoke::{
    PAGE, handle_close, memobj_create, nanosleep, proc_fork, proc_wait, revoke, stage, start,
    thread_create, transfer,
};
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;

entry!(main);

/// How long the controller waits for the client to finish its first use before acting.
const HOLD_NS: u64 = 300_000_000;

fn fail(message: &str) -> i32 {
    println!("cap: failed {message}");
    1
}

/// The first handle the controller installs in a child lands at index 3 (0..2 are the well-known
/// SELF_PROC/INITFS/namespace handles; the root-device slot is first-process-only).
const FIRST_GRANT: u32 = 3;

fn revoke_case(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let object = memobj_create(PAGE).map_err(|_| "revoke memobj")?;
    let staged = stage(initfs, "/client1").map_err(|_| "revoke stage")?;
    // Handle 3: MAP|READ|DUP, the revoked lineage. Handle 4: READ|DUP, never had MAP.
    let granted = transfer(
        staged.process,
        object,
        Rights::MAP.union(Rights::READ).union(Rights::DUP),
    )
    .map_err(|_| "revoke transfer")?;
    let never = transfer(staged.process, object, Rights::READ.union(Rights::DUP))
        .map_err(|_| "revoke transfer never")?;
    if granted.index() != FIRST_GRANT || never.index() != FIRST_GRANT.saturating_add(1) {
        println!(
            "cap: revoke grant index {} {}",
            granted.index(),
            never.index()
        );
        return Err("revoke grant index");
    }
    // The fork shares the lineage nodes and the two handles; both processes must see the revoke.
    let forked = proc_fork(staged.process).map_err(|_| "revoke fork")?;
    let thread =
        thread_create(forked, staged.entry, staged.stack, 0).map_err(|_| "revoke clone")?;
    handle_close(thread);
    start(&staged).map_err(|_| "revoke start")?;
    nanosleep(HOLD_NS);
    // Revoke MAP on the source lineage; every descendant in both processes loses it (EPERM).
    revoke(object, Rights::MAP).map_err(|_| "revoke")?;
    let code = proc_wait(staged.process).map_err(|_| "revoke wait")?;
    let fork_code = proc_wait(forked).map_err(|_| "revoke fork wait")?;
    handle_close(forked);
    handle_close(staged.process);
    handle_close(object);
    if code != 0 || fork_code != 0 {
        println!("cap: revoke client exit {code} fork exit {fork_code}");
        return Err("revoke client exit");
    }
    println!("cap: revoke case ok");
    Ok(())
}

fn lifetime_case(initfs: &Initfs<'_>) -> Result<(), &'static str> {
    let object = memobj_create(PAGE).map_err(|_| "lifetime memobj")?;
    let staged = stage(initfs, "/client2").map_err(|_| "lifetime stage")?;
    let granted = transfer(staged.process, object, Rights::MAP).map_err(|_| "lifetime transfer")?;
    if granted.index() != FIRST_GRANT {
        println!("cap: lifetime grant index {}", granted.index());
        return Err("lifetime grant index");
    }
    start(&staged).map_err(|_| "lifetime start")?;
    nanosleep(HOLD_NS);
    // Release the source handle while the child still holds its derived handle: the object must
    // stay alive (refcount) and the child's next map must still succeed.
    handle_close(object);
    let code = proc_wait(staged.process).map_err(|_| "lifetime wait")?;
    handle_close(staged.process);
    if code != 0 {
        println!("cap: lifetime client exit {code}");
        return Err("lifetime client exit");
    }
    println!("cap: lifetime case ok");
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

    if let Err(message) = revoke_case(&initfs) {
        return fail(message);
    }
    if let Err(message) = lifetime_case(&initfs) {
        return fail(message);
    }
    println!("cap: ok");
    0
}
