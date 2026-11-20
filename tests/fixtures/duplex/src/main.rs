// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K208 duplex endpoint fixture: pair creation, both directions, EOF, EPIPE, shutdown,
//! datagram boundaries, dup/transfer/close and EVQ readiness, all through raw syscalls.

#![no_std]
#![no_main]
#![allow(unsafe_code)]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_rt::entry;
use raven_rt::event::{
    EventQueue, EvqEvent, HUP, READABLE, WRITABLE,
};
use raven_rt::pipe::pipe;
use raven_rt::println;
use raven_rt::sys::{
    close, handle_close, handle_dup, handle_transfer, read, syscall6, write,
};

entry!(main);

/// Raw `duplex_pair(out: *mut [u32; 2], flags: u32) -> 0` (K208; not in the ABI crate).
const DUPLEX_PAIR: usize = 0x110;
/// Raw `duplex_shutdown(handle: u32, how: u32) -> 0` (K208; not in the ABI crate).
const DUPLEX_SHUTDOWN: usize = 0x111;
/// Datagram mode flag.
const DUPLEX_DATAGRAM: u32 = 1;
/// `O_NONBLOCK` from the native descriptor flags.
const O_NONBLOCK: u32 = 0o4000;
/// POSIX shutdown selectors.
const SHUT_RD: usize = 0;
const SHUT_WR: usize = 1;
const SHUT_RDWR: usize = 2;

fn pair(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut handles = [0u32; 2];
    // SAFETY: `handles` is a live two-word array and the kernel writes exactly eight bytes.
    let result = unsafe {
        syscall6(
            DUPLEX_PAIR,
            handles.as_mut_ptr() as usize,
            flags as usize,
            0,
            0,
            0,
            0,
        )
    };
    Errno::from_ret(result)?;
    Ok((Handle(handles[0]), Handle(handles[1])))
}

fn shutdown(handle: Handle, how: usize) -> Result<(), Errno> {
    // SAFETY: all arguments are scalar handles and selectors; the kernel validates them.
    let result = unsafe { syscall6(DUPLEX_SHUTDOWN, handle.0 as usize, how, 0, 0, 0, 0) };
    Errno::from_ret(result).map(|_| ())
}

fn both_directions() -> Result<bool, Errno> {
    let mut buffer = [0u8; 16];
    let (left, right) = pair(0)?;
    let left_to_right = write(left, b"hello") == Ok(5)
        && read(right, &mut buffer[..5]) == Ok(5)
        && &buffer[..5] == b"hello";
    let right_to_left = write(right, b"world") == Ok(5)
        && read(left, &mut buffer[..5]) == Ok(5)
        && &buffer[..5] == b"world";
    handle_close(left)?;
    handle_close(right)?;
    Ok(left_to_right && right_to_left)
}

fn flags() -> bool {
    pair(0x8000_0000)
        .map(|_| false)
        .unwrap_or(true)
}

fn close_eof_epipe() -> Result<bool, Errno> {
    let mut buffer = [0u8; 4];
    let (left, right) = pair(0)?;
    write(left, b"last")?;
    handle_close(left)?;
    let tail = read(right, &mut buffer) == Ok(4) && &buffer == b"last";
    let eof = read(right, &mut buffer) == Ok(0);
    handle_close(right)?;

    let (left, right) = pair(0)?;
    handle_close(right)?;
    let epipe = write(left, b"x") == Err(Errno::EPIPE);
    handle_close(left)?;
    Ok(tail && eof && epipe)
}

fn shutdown_checks() -> Result<bool, Errno> {
    let mut buffer = [0u8; 4];

    let (left, right) = pair(0)?;
    shutdown(left, SHUT_WR)?;
    let write_eof = read(right, &mut buffer) == Ok(0);
    let write_epipe = write(left, b"x") == Err(Errno::EPIPE);
    handle_close(left)?;
    handle_close(right)?;

    let (left, right) = pair(0)?;
    shutdown(left, SHUT_RD)?;
    let read_eof = read(left, &mut buffer) == Ok(0);
    let peer_epipe = write(right, b"x") == Err(Errno::EPIPE);
    handle_close(left)?;
    handle_close(right)?;

    let (left, right) = pair(0)?;
    shutdown(left, SHUT_RDWR)?;
    let both_eof = read(left, &mut buffer) == Ok(0);
    let both_epipe = write(left, b"x") == Err(Errno::EPIPE);
    handle_close(left)?;
    handle_close(right)?;

    Ok(write_eof && write_epipe && read_eof && peer_epipe && both_eof && both_epipe)
}

fn datagrams() -> Result<bool, Errno> {
    let mut buffer = [0u8; 8];
    let (left, right) = pair(DUPLEX_DATAGRAM | O_NONBLOCK)?;
    let first = write(left, b"one") == Ok(3);
    let second = write(left, b"two") == Ok(3);
    let one = read(right, &mut buffer) == Ok(3) && &buffer[..3] == b"one";
    let two = read(right, &mut buffer) == Ok(3) && &buffer[..3] == b"two";
    let empty = read(right, &mut buffer) == Err(Errno::EAGAIN);
    handle_close(left)?;
    handle_close(right)?;
    Ok(first && second && one && two && empty)
}

fn dup_transfer() -> Result<bool, Errno> {
    let mut buffer = [0u8; 8];
    let (left, right) = pair(0)?;
    let duplicate = handle_dup(left, Rights::ALL)?;
    handle_close(left)?;
    let duplicate_ok = write(duplicate, b"dup") == Ok(3)
        && read(right, &mut buffer) == Ok(3)
        && &buffer[..3] == b"dup";
    handle_close(duplicate)?;
    let duplicate_eof = read(right, &mut buffer) == Ok(0);
    handle_close(right)?;

    let (left, right) = pair(0)?;
    let transferred = handle_transfer(Handle::SELF_PROC, left, Rights::ALL)?;
    handle_close(left)?;
    let transfer_ok = write(transferred, b"xfr") == Ok(3)
        && read(right, &mut buffer) == Ok(3)
        && &buffer[..3] == b"xfr";
    handle_close(transferred)?;
    let transfer_eof = read(right, &mut buffer) == Ok(0);
    handle_close(right)?;

    Ok(duplicate_ok && duplicate_eof && transfer_ok && transfer_eof)
}

fn readiness() -> Result<bool, Errno> {
    let (left, right) = pair(0)?;
    let queue = EventQueue::create()?;
    let mut events = [EvqEvent::default(); 1];

    queue.add(left, WRITABLE, 0x11)?;
    let writable = queue.wait(&mut events, 1_000_000_000) == Ok(1)
        && events[0].events & WRITABLE != 0;

    queue.modify(left, READABLE, 0x12)?;
    write(right, b"e")?;
    let readable = queue.wait(&mut events, 1_000_000_000) == Ok(1)
        && events[0].events & READABLE != 0;

    queue.modify(left, HUP, 0x13)?;
    handle_close(right)?;
    let hup = queue.wait(&mut events, 1_000_000_000) == Ok(1) && events[0].events & HUP != 0;

    queue.delete(left)?;
    handle_close(left)?;
    handle_close(queue.handle())?;
    Ok(writable && readable && hup)
}

fn close_paths() -> Result<bool, Errno> {
    let mut buffer = [0u8; 4];

    let (left, right) = pair(0)?;
    close(left)?;
    let eof = read(right, &mut buffer) == Ok(0);
    let epipe = write(right, b"x") == Err(Errno::EPIPE);
    close(right)?;
    let left_stale = handle_close(left) == Err(Errno::EBADF);
    let right_stale = handle_close(right) == Err(Errno::EBADF);
    let closed_io = write(left, b"x") == Err(Errno::EBADF);

    let (read_end, write_end) = pipe()?;
    close(read_end)?;
    let pipe_epipe = write(write_end, b"x") == Err(Errno::EPIPE);
    close(write_end)?;
    let pipe_stale = handle_close(write_end) == Err(Errno::EBADF);

    Ok(eof && epipe && left_stale && right_stale && closed_io && pipe_epipe && pipe_stale)
}

fn run() -> Result<(), Errno> {
    let both = both_directions()?;
    let bad_flags = flags();
    let eof_epipe = close_eof_epipe()?;
    let shutdown_ok = shutdown_checks()?;
    let datagram_ok = datagrams()?;
    let handles_ok = dup_transfer()?;
    let evq_ok = readiness()?;
    let close_ok = close_paths()?;
    println!(
        "duplex: both={} flags={}",
        u8::from(both),
        u8::from(bad_flags)
    );
    println!(
        "duplex: eof_epipe={} shutdown={}",
        u8::from(eof_epipe),
        u8::from(shutdown_ok)
    );
    println!(
        "duplex: datagram={} handles={} evq={}",
        u8::from(datagram_ok),
        u8::from(handles_ok),
        u8::from(evq_ok)
    );
    println!("duplex: close={}", u8::from(close_ok));
    if !(both
        && bad_flags
        && eof_epipe
        && shutdown_ok
        && datagram_ok
        && handles_ok
        && evq_ok
        && close_ok)
    {
        println!("duplex: FAIL checks");
        return Err(Errno::EINVAL);
    }
    println!("duplex: ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("duplex: error {error:?}");
            1
        }
    }
}
