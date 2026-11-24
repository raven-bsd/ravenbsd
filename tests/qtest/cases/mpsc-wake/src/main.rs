// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K155: a receiver waits, a sender sends; the send must return and the receiver must wake.
//! The condvar ping-pong is the control; 20 unbounded and 20 bounded mpsc rounds follow.
//!
//! The fixture runs as `/init` in a custom initfs with no vfsd/servers and therefore no stdout
//! fd. Markers go through the kernel DEBUG_WRITE syscall, not std::io.

use core::arch::asm;

use raven_abi::syscall::Syscall;
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

fn debug_write(bytes: &[u8]) {
    // SAFETY: DEBUG_WRITE reads `bytes` for its length; the kernel validates the user slice.
    unsafe {
        asm!(
            "syscall",
            inlateout("rax") Syscall::DEBUG_WRITE as usize => _,
            in("rdi") bytes.as_ptr(),
            in("rsi") bytes.len(),
            lateout("rcx") _,
            lateout("r11") _,
            options(nostack),
        );
    }
}

fn marker(text: &str) {
    let line = format!("mpsc-wake: {}\n", text);
    debug_write(line.as_bytes());
}

fn fail(message: &str) -> ! {
    marker(&format!("fail {}", message));
    std::process::exit(1);
}

fn condvar_control() {
    let pair = Arc::new((Mutex::new(0u32), Condvar::new()));
    let worker = {
        let pair = Arc::clone(&pair);
        thread::spawn(move || {
            let (lock, condvar) = &*pair;
            let mut ready = lock.lock().unwrap_or_else(|error| error.into_inner());
            while *ready == 0 {
                ready = condvar
                    .wait(ready)
                    .unwrap_or_else(|error| error.into_inner());
            }
            *ready
        })
    };
    thread::sleep(Duration::from_millis(10));
    {
        let (lock, condvar) = &*pair;
        let mut ready = lock.lock().unwrap_or_else(|error| error.into_inner());
        *ready = 1;
        condvar.notify_one();
    }
    match worker.join() {
        Ok(1) => {}
        _ => fail("condvar control"),
    }
    marker("condvar ok");
}

fn unbounded() {
    for round in 0..20u32 {
        let (sender, receiver) = mpsc::channel();
        let worker = thread::spawn(move || receiver.recv());
        thread::sleep(Duration::from_millis(10));
        if sender.send(round).is_err() {
            fail("unbounded send");
        }
        match worker.join() {
            Ok(Ok(value)) if value == round => {}
            _ => fail("unbounded recv"),
        }
        marker(&format!("unbounded round {} ok", round));
    }
    marker("unbounded 20 ok");
}

fn bounded() {
    for round in 0..20u32 {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker = thread::spawn(move || receiver.recv());
        thread::sleep(Duration::from_millis(10));
        if sender.send(round).is_err() {
            fail("bounded send");
        }
        match worker.join() {
            Ok(Ok(value)) if value == round => {}
            _ => fail("bounded recv"),
        }
        marker(&format!("bounded round {} ok", round));
    }
    marker("bounded 20 ok");
}

fn main() {
    marker("start");
    condvar_control();
    unbounded();
    bounded();
    marker("done");
}
