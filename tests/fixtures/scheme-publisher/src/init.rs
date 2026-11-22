// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K532 controller: reserved-name authority, trusted setuid and a hostile `04755` server.

#![no_std]
#![no_main]

extern crate alloc;

use raven_abi::errno::Errno;
use raven_proto_fs::OpenFlags;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnError, SpawnOptions};
use raven_scheme::Client;
use raven_scheme_publisher::{
    CHILD, FAKE_SERVER, GID, INITFSD, PROBE, SETUID_CHILD, TRUSTED_PUBLISHER, UID, child_ids,
    close, create_empty, exec_cred, initfs, open_retry, scheme_register, spawn_file, spawn_initfs,
    wait,
};

entry!(main);

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => {
            println!("scheme-publisher: ok");
            0
        }
        Err(step) => {
            println!("scheme-publisher: fail {step}");
            1
        }
    }
}

fn run() -> Result<(), &'static str> {
    let initfs = initfs().map_err(|_| "initfs")?;
    let _initfsd = spawn_initfs(&initfs, INITFSD, &[]).map_err(|_| "initfsd spawn")?;
    let ready = open_retry("initfs:", 3000).map_err(|_| "initfsd ready")?;
    let _ = Client::new().close(ready);
    println!("scheme-publisher: initfs ready");

    raven_rt::proc::setresgid(GID).map_err(|_| "setgid")?;
    raven_rt::proc::setresuid(UID).map_err(|_| "setuid")?;
    let ids = raven_rt::proc::credentials().map_err(|_| "credentials")?;
    if ids.euid != UID || ids.egid != GID {
        return Err("caller ids");
    }
    println!(
        "scheme-publisher: caller euid={} egid={}",
        ids.euid, ids.egid
    );

    let mut failed = false;

    // Protected names before any trusted publisher exists.
    let tty = scheme_register("tty")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    let net = scheme_register("net")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    println!("reserved-boot: tty={tty} net={net}");
    if tty != "EPERM" || net != "EPERM" {
        failed = true;
    }

    // The exact file/vfs names before boot registration, in a process that exits so a master
    // registration cannot hide the trusted-publisher reclaim row that follows.
    let probe = spawn_initfs(&initfs, PROBE, &[]).map_err(|_| "probe spawn")?;
    if wait(probe).map_err(|_| "probe wait")? != 0 {
        return Err("probe exit");
    }

    // A trusted root publisher records file/vfs, then exits.
    let first = spawn_file(TRUSTED_PUBLISHER, &[TRUSTED_PUBLISHER, "first"], false)
        .map_err(|_| "trusted first spawn")?;
    if wait(first) != Ok(0) {
        return Err("trusted first wait");
    }
    println!("trusted-first: wait=0");

    // Death/restart race: an unprivileged process must not reclaim either name.
    let file = scheme_register("file")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    let vfs = scheme_register("vfs")
        .map(|()| "registered")
        .unwrap_or("EPERM");
    println!("reserved-restart: file={file} vfs={vfs}");
    if file != "EPERM" || vfs != "EPERM" {
        failed = true;
    }

    // The trusted root service may reclaim both names.
    let second = spawn_file(TRUSTED_PUBLISHER, &[TRUSTED_PUBLISHER, "second"], false)
        .map_err(|_| "trusted second spawn")?;
    let second_code = wait(second).map_err(|_| "trusted second wait")?;
    if second_code == 0 {
        println!("trusted-second: wait=0");
    } else {
        println!("trusted-second: wait={second_code}");
        failed = true;
    }

    // The trusted initfs setuid image still elevates; EXEC_NOSUID suppresses it.
    run_trusted(SETUID_CHILD, "setuid", false, 0)?;
    run_trusted(SETUID_CHILD, "nosuid", true, UID)?;

    // Hostile unprivileged server, started after the caller dropped the credentials.
    let _fake = spawn_initfs(&initfs, FAKE_SERVER, &[]).map_err(|_| "fake spawn")?;
    let ready = open_retry("evil:child", 3000).map_err(|_| "fake ready")?;
    let _ = Client::new().close(ready);
    println!("fake-server: reachable");

    // Direct kernel path: an empty child keeps the caller ids and gets EPERM.
    let child = create_empty().map_err(|_| "fake child create")?;
    let file = Client::new()
        .open("evil:child", u32::from(OpenFlags::READ.to_wire()))
        .map_err(|_| "fake open")?;
    match exec_cred(child, file, false) {
        Err(Errno::EPERM) => {
            let (euid, egid) = child_ids(child).map_err(|_| "fake child ids")?;
            println!("fake-exec: EPERM child-euid={euid} child-egid={egid}");
            if euid != UID || egid != GID {
                failed = true;
            }
        }
        Ok(()) => {
            let (euid, egid) = child_ids(child).map_err(|_| "fake child ids")?;
            println!("fake-exec: euid0 child-euid={euid} child-egid={egid}");
            failed = true;
        }
        Err(_) => return Err("fake exec error"),
    }
    let _ = Client::new().close(file);
    close(child);

    // Full spawn path: the same hostile image must fail closed with EPERM.
    let file = Client::new()
        .open("evil:child", u32::from(OpenFlags::READ.to_wire()))
        .map_err(|_| "fake open spawn")?;
    let args = [CHILD, "fake", "0", "1000"];
    match raven_rt::spawn::spawn_file(
        file,
        CHILD,
        &SpawnOptions {
            args: &args,
            env: &[],
        },
        false,
    ) {
        Err(SpawnError::Sys(Errno::EPERM)) => println!("fake-spawn: EPERM"),
        Ok(child) => {
            let code = wait(child).map_err(|_| "fake spawn wait")?;
            println!("fake-spawn: euid0 wait={code}");
            failed = true;
        }
        Err(_) => return Err("fake spawn error"),
    }
    let _ = Client::new().close(file);

    if failed { Err("rows") } else { Ok(()) }
}

fn run_trusted(
    path: &str,
    label: &str,
    nosuid: bool,
    expected_euid: u32,
) -> Result<(), &'static str> {
    let expected = alloc::format!("{expected_euid}");
    let args = [path, label, expected.as_str(), "1000"];
    let child = spawn_file(path, &args, nosuid).map_err(|_| "trusted child spawn")?;
    let code = wait(child).map_err(|_| "trusted child wait")?;
    if code != 0 {
        return Err("trusted child exit");
    }
    println!("trusted-run: {label}=0");
    Ok(())
}
