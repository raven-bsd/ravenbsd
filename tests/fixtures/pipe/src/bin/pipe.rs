// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P2j `pipe` fixture: data, non-blocking, EOF, EPIPE/SIGPIPE, ring wrap and bad-direction
//! checks in one process.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_rt::entry;
use raven_rt::pipe::{O_NONBLOCK, pipe, pipe2};
use raven_rt::println;
use raven_rt::sys::{handle_close, read, write};

use raven_pipe_tests::{SIGPIPE, WNOHANG, sigrecv};

entry!(main);

const CAPACITY: usize = 64 * 1024;

fn pattern(index: usize) -> u8 {
    (index & 0xFF) as u8
}

fn verify(bytes: &[u8]) -> bool {
    bytes
        .iter()
        .enumerate()
        .all(|(index, byte)| *byte == pattern(index))
}

fn run() -> Result<(), Errno> {
    let mut buffer = [0u8; CAPACITY];

    let (read_end, write_end) = pipe()?;
    let wrote = write(write_end, b"hello") == Ok(5);
    let echoed = read(read_end, &mut buffer[..5]) == Ok(5) && &buffer[..5] == b"hello";
    let basic = wrote && echoed;
    handle_close(read_end)?;
    handle_close(write_end)?;

    let (nonblock_read, nonblock_write) = pipe2(O_NONBLOCK)?;
    let empty = read(nonblock_read, &mut buffer) == Err(Errno::EAGAIN);
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = pattern(index);
    }
    let filled = write(nonblock_write, &buffer) == Ok(CAPACITY);
    let full = write(nonblock_write, b"x") == Err(Errno::EAGAIN);
    let drained = read(nonblock_read, &mut buffer) == Ok(CAPACITY) && verify(&buffer);
    let still_empty = read(nonblock_read, &mut buffer) == Err(Errno::EAGAIN);
    let nonblock = empty && filled && full && drained && still_empty;
    handle_close(nonblock_read)?;
    handle_close(nonblock_write)?;

    let (eof_read, eof_write) = pipe()?;
    let tail = write(eof_write, b"last") == Ok(4);
    handle_close(eof_write)?;
    let tail_read = read(eof_read, &mut buffer[..4]) == Ok(4) && &buffer[..4] == b"last";
    let eof = read(eof_read, &mut buffer) == Ok(0);
    handle_close(eof_read)?;

    let (broken_read, broken_write) = pipe()?;
    handle_close(broken_read)?;
    let epipe = write(broken_write, b"x") == Err(Errno::EPIPE);
    let mut signal = [0u64; 3];
    let sigpipe = sigrecv(&mut signal, WNOHANG) == Ok(SIGPIPE);
    handle_close(broken_write)?;

    let (bad_read, bad_write) = pipe()?;
    let read_on_write = read(bad_write, &mut buffer) == Err(Errno::EBADF);
    let write_on_read = write(bad_read, b"x") == Err(Errno::EBADF);
    let bad_flags = pipe2(0x8000_0000) == Err(Errno::EINVAL);
    handle_close(bad_read)?;
    handle_close(bad_write)?;

    let (wrap_read, wrap_write) = pipe()?;
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = pattern(index);
    }
    let first_write = write(wrap_write, &buffer[..40_000]) == Ok(40_000);
    let first_read =
        read(wrap_read, &mut buffer[..40_000]) == Ok(40_000) && verify(&buffer[..40_000]);
    for (index, byte) in buffer.iter_mut().enumerate() {
        *byte = pattern(index);
    }
    let second_write = write(wrap_write, &buffer[..40_000]) == Ok(40_000);
    let second_read =
        read(wrap_read, &mut buffer[..40_000]) == Ok(40_000) && verify(&buffer[..40_000]);
    let wrap = first_write && first_read && second_write && second_read;
    handle_close(wrap_read)?;
    handle_close(wrap_write)?;

    let bad_fd = read_on_write && write_on_read;
    println!(
        "pipe: basic={} nonblock={}",
        u8::from(basic),
        u8::from(nonblock)
    );
    println!(
        "pipe: eof={} epipe={} sigpipe={}",
        u8::from(tail && tail_read && eof),
        u8::from(epipe),
        u8::from(sigpipe)
    );
    println!(
        "pipe: wrap={} badfd={} flags={}",
        u8::from(wrap),
        u8::from(bad_fd),
        u8::from(bad_flags)
    );

    if !(basic
        && nonblock
        && tail
        && tail_read
        && eof
        && epipe
        && sigpipe
        && bad_fd
        && bad_flags
        && wrap)
    {
        println!("pipe: FAIL checks");
        return Err(Errno::EINVAL);
    }
    println!("pipe: ok");
    Ok(())
}

fn main(_args: raven_rt::Args) -> i32 {
    match run() {
        Ok(()) => 0,
        Err(error) => {
            println!("pipe: error {error:?}");
            1
        }
    }
}
