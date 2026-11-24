// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! S183 discriminator: a static std program with no brush. It sets a `thread_local!` with a
//! destructor, spawns a thread that reads it, joins, and calls `exit(0)`. The second mode reads
//! the same `thread_local!` from its destructor, which is the exact std `AccessError` shape.
//! If the normal mode panics here, the defect is in std/libc TLS teardown; if it is clean, the
//! shell owns the finding and this program remains the regression discriminator.

use std::cell::RefCell;

unsafe extern "C" {
    fn write(fd: i32, buffer: *const u8, length: usize) -> isize;
    fn pthread_exit(value: *mut core::ffi::c_void) -> !;
}

struct ThreadValue;

impl Drop for ThreadValue {
    fn drop(&mut self) {
        println!("TLS183: drop thread");
    }
}

struct MainValue;

impl Drop for MainValue {
    fn drop(&mut self) {
        println!("TLS183: drop main");
    }
}

struct SelfRead;

impl Drop for SelfRead {
    fn drop(&mut self) {
        println!("TLS183: selfread drop");
        // The same-key access from its own destructor is the std AccessError contract case.
        SELF_READ.with(|_| ());
        println!("TLS183: destructor-read returned");
    }
}

thread_local! {
    static THREAD_VALUE: RefCell<Option<ThreadValue>> = RefCell::new(Some(ThreadValue));
    static MAIN_VALUE: RefCell<Option<MainValue>> = RefCell::new(Some(MainValue));
    static SELF_READ: RefCell<Option<SelfRead>> = RefCell::new(Some(SelfRead));
}

fn destructor_read_child() -> i32 {
    println!("TLS183: destructor-read start");
    SELF_READ.with(|_| ());
    // Returning from main runs std's TLS destructor list; SelfRead's Drop reads its own key.
    0
}

fn discriminator() -> i32 {
    println!("TLS183: start");
    MAIN_VALUE.with(|_| ());
    let worker = std::thread::spawn(|| {
        THREAD_VALUE.with(|_| println!("TLS183: thread read"));
    });
    if worker.join().is_err() {
        println!("TLS183: join failed");
        return 1;
    }
    println!("TLS183: join ok");
    println!("TLS183: clean");
    std::process::exit(0);
}

#[cfg(not(tls183_destructor_read))]
fn main() -> std::process::ExitCode {
    std::process::ExitCode::from(discriminator() as u8)
}

#[cfg(tls183_destructor_read)]
fn main() -> std::process::ExitCode {
    // The AccessError panic happens while the TLS list is being destroyed; the service stderr is
    // not on the console, so print the payload on stdout with a raw write from the panic hook.
    std::panic::set_hook(Box::new(|info| {
        let mut message = String::from("TLS183: PANIC ");
        if let Some(text) = info.payload().downcast_ref::<&str>() {
            message.push_str(text);
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            message.push_str(text);
        } else {
            message.push_str("non-string panic");
        }
        message.push('\n');
        // SAFETY: fd 1 is the service console; the buffer is live for the call.
        unsafe {
            write(1, message.as_ptr(), message.len());
        }
    }));
    let _ = destructor_read_child();
    // The contract case: pthread_exit from the main thread runs the key destructors while the
    // TLS is addressable. The same-key read in SelfRead::drop is the std AccessError.
    // SAFETY: pthread_exit is the libc thread-exit entry; a null value is the thread result.
    unsafe { pthread_exit(core::ptr::null_mut()) }
}
