// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S33 controller: drives the lifecycle scenarios against the fixture server.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::Handle;
use raven_initfs::Initfs;
use raven_rt::entry;
use raven_rt::println;
use raven_rt::spawn::{SpawnOptions, spawn};
use raven_s33_ipc::{O_RDWR, call, close, map_initfs_image, open, proc_wait, write};

entry!(main);

const SERVER: &str = "/server";
const LOOP: &str = "/s33-loop";
const EXIT: &str = "/s33-exit";
const BLOCK: &str = "/s33-block";
const RETRIES: usize = 2000;

fn open_retry(path: &str) -> Result<Handle, Errno> {
    let mut attempt = 0usize;
    while attempt < RETRIES {
        match open(path, O_RDWR) {
            Ok(handle) => return Ok(handle),
            Err(Errno::ENOENT) => {
                let _ = raven_s33_ipc::nanosleep(1_000_000);
                attempt = attempt.saturating_add(1);
            }
            Err(error) => return Err(error),
        }
    }
    Err(Errno::ETIMEDOUT)
}

fn spawn_child(initfs: &Initfs<'_>, path: &str) -> Result<Handle, Errno> {
    let options = SpawnOptions {
        args: &[],
        env: &[],
    };
    spawn(initfs, path, &options).map_err(|_| Errno::EIO)
}

fn wait_ok(child: Handle) -> bool {
    matches!(proc_wait(child), Ok(0))
}

fn main(_args: raven_rt::Args) -> i32 {
    let image = match map_initfs_image() {
        Ok(image) => image,
        Err(_) => {
            println!("s33: initfs map failed");
            return 1;
        }
    };
    let initfs = match Initfs::parse(image) {
        Ok(initfs) => initfs,
        Err(_) => {
            println!("s33: initfs parse failed");
            return 1;
        }
    };
    let server = match spawn_child(&initfs, SERVER) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: server spawn failed");
            return 1;
        }
    };
    let control = match open_retry("s33:normal") {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: server did not register");
            return 1;
        }
    };

    // K2: a server-supplied invalid RECV destination must complete the dequeued request with an
    // error, and the server must keep serving afterwards.
    let bad_recv = match open("s33:bad-recv", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: bad-recv open failed");
            return 1;
        }
    };
    if call(bad_recv, b"arm", None).is_err() {
        println!("s33: arm failed");
        return 1;
    }
    let first = write(bad_recv, b"payload");
    if !matches!(first, Err(Errno::EPROTO)) {
        match first {
            Ok(length) => println!("s33: invalid recv accepted {length} bytes"),
            Err(error) => println!("s33: invalid recv errno {}", error.0),
        }
        return 1;
    }
    let second = write(bad_recv, b"again!!");
    if second.ok() != Some(7) {
        match second {
            Ok(length) => println!("s33: request after invalid recv returned {length}"),
            Err(error) => println!("s33: request after invalid recv errno {}", error.0),
        }
        return 1;
    }
    let _ = close(bad_recv);
    println!("s33: bad recv ok");

    // K4: oversized successful replies are EPROTO; equal and zero lengths stay successful.
    let bad_write = match open("s33:bad-write", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: bad-write open failed");
            return 1;
        }
    };
    if !matches!(write(bad_write, b"12345678"), Err(Errno::EPROTO)) {
        println!("s33: oversized write reply was accepted");
        return 1;
    }
    let zero_write = match open("s33:zero-write", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: zero-write open failed");
            return 1;
        }
    };
    if write(zero_write, b"12345678").ok() != Some(0) {
        println!("s33: zero write reply failed");
        return 1;
    }
    if write(control, b"12345678").ok() != Some(8) {
        println!("s33: exact write reply failed");
        return 1;
    }

    let bad_call = match open("s33:bad-call", O_RDWR) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: bad-call open failed");
            return 1;
        }
    };
    let mut response = [0u8; 16];
    if !matches!(
        call(bad_call, b"bad", Some(&mut response)),
        Err(Errno::EPROTO)
    ) {
        println!("s33: oversized call reply was accepted");
        return 1;
    }
    if call(control, b"exact", Some(&mut response)).ok() != Some(response.len()) {
        println!("s33: exact call reply failed");
        return 1;
    }
    if call(control, b"zero", Some(&mut response)).ok() != Some(0) {
        println!("s33: zero call reply failed");
        return 1;
    }
    let _ = close(bad_write);
    let _ = close(zero_write);
    let _ = close(bad_call);
    println!("s33: reply bounds ok");

    // K3: 10,000 OPEN/CLOSE and DUP cycles must not accumulate completed one-way requests.
    let looper = match spawn_child(&initfs, LOOP) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: loop spawn failed");
            return 1;
        }
    };
    if !wait_ok(looper) {
        println!("s33: loop child failed");
        return 1;
    }

    // K3/cancellation: the child queues a one-way CLOSE and exits while the server is paused; the
    // kernel must cancel the request and the server must keep serving.
    let exiter = match spawn_child(&initfs, EXIT) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: exit spawn failed");
            return 1;
        }
    };
    if !wait_ok(exiter) {
        println!("s33: exit child failed");
        return 1;
    }
    if call(control, b"alive", None).is_err() {
        println!("s33: request after cancellation failed");
        return 1;
    }
    println!("s33: cancel ok");

    // K2/server death: a client blocked on a server that received without replying wakes with EIO
    // when the server exits.
    let blocker = match spawn_child(&initfs, BLOCK) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s33: block spawn failed");
            return 1;
        }
    };
    if !wait_ok(blocker) {
        println!("s33: block child failed");
        return 1;
    }
    let _ = proc_wait(server);
    let _ = close(control);
    println!("s33: lifecycle ok");
    0
}
