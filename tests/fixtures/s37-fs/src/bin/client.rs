// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S37 filesystem client: K66 flags, sequential offsets, DUP/last-close, 10,000 cycles.

#![no_std]
#![no_main]

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights};
use raven_proto_fs::{
    O_APPEND, O_CREAT, O_DIRECTORY, O_EXCL, O_NOFOLLOW, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY,
    OpenFlags, Stat,
};
use raven_rt::entry;
use raven_rt::println;
use raven_s37_fs::{call, close, dup, fstat, open, read, write};

entry!(main);

const LOOP_CYCLES: usize = 10_000;

fn flags(linux: u32) -> OpenFlags {
    OpenFlags::from_linux(linux).unwrap_or(OpenFlags::READ)
}

fn open_flags(path: &str, flags: OpenFlags) -> Result<Handle, Errno> {
    open(path, flags.to_wire())
}

fn read_expect(handle: Handle, expected: &[u8]) -> bool {
    let mut buffer = [0u8; 16];
    let length = expected.len().min(buffer.len());
    match read(handle, &mut buffer[..length]) {
        Ok(count) if count == length && buffer[..length] == *expected => true,
        _ => false,
    }
}

fn flags_test() -> bool {
    if !matches!(
        open_flags("fs:data", flags(O_RDWR | O_CREAT | O_EXCL)),
        Err(Errno::EEXIST)
    ) {
        println!("s37: O_EXCL existing did not fail");
        return false;
    }
    let created = match open_flags("fs:new1", flags(O_RDWR | O_CREAT | O_EXCL)) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: O_EXCL create failed");
            return false;
        }
    };
    if !matches!(
        open_flags("fs:new1", flags(O_RDWR | O_CREAT | O_EXCL)),
        Err(Errno::EEXIST)
    ) {
        println!("s37: second O_EXCL create did not fail");
        return false;
    }
    let _ = close(created);

    let truncated = match open_flags("fs:truncflag", flags(O_RDWR | O_TRUNC)) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: O_TRUNC open failed");
            return false;
        }
    };
    let mut record = [0u8; 64];
    let size = match fstat(truncated, &mut record) {
        Ok(()) => Stat::decode(&record)
            .map(|stat| stat.size)
            .unwrap_or(u64::MAX),
        Err(_) => u64::MAX,
    };
    let _ = close(truncated);
    if size != 0 {
        println!("s37: O_TRUNC left size {size}");
        return false;
    }

    if !matches!(
        open_flags("fs:data", flags(O_RDONLY | O_DIRECTORY)),
        Err(Errno::ENOTDIR)
    ) {
        println!("s37: O_DIRECTORY on a file was accepted");
        return false;
    }
    match open_flags("fs:dir", flags(O_RDONLY | O_DIRECTORY)) {
        Ok(handle) => {
            let _ = close(handle);
        }
        Err(_) => {
            println!("s37: O_DIRECTORY on a directory failed");
            return false;
        }
    }

    if !matches!(
        open_flags("fs:link", flags(O_RDONLY | O_NOFOLLOW)),
        Err(Errno::ELOOP)
    ) {
        println!("s37: O_NOFOLLOW did not fail");
        return false;
    }
    let link = match open_flags("fs:link", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: link follow failed");
            return false;
        }
    };
    let followed = read_expect(link, b"0123");
    let _ = close(link);
    if !followed {
        println!("s37: link target data mismatch");
        return false;
    }

    if !matches!(open("fs:data", 0x0100), Err(Errno::EINVAL)) {
        println!("s37: unknown wire bits were accepted");
        return false;
    }
    for raw in [0x09u16, 0x11, 0x45] {
        if !matches!(open("fs:data", raw), Err(Errno::EINVAL)) {
            println!("s37: invalid flag combination {raw:#x} was accepted");
            return false;
        }
    }
    true
}

fn append_test() -> bool {
    let flags = match OpenFlags::from_linux(O_WRONLY | O_APPEND) {
        Ok(flags) => flags,
        Err(_) => return false,
    };
    let first = match open_flags("fs:append", flags) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: append first open failed");
            return false;
        }
    };
    let second = match open_flags("fs:append", flags) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: append second open failed");
            let _ = close(first);
            return false;
        }
    };
    let first_write = write(first, b"A").ok() == Some(1);
    let second_write = write(second, b"B").ok() == Some(1);
    let _ = close(first);
    let _ = close(second);
    if !first_write || !second_write {
        println!("s37: append write failed");
        return false;
    }
    let check = match open_flags("fs:append", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    let matched = read_expect(check, b"AB");
    let _ = close(check);
    if !matched {
        println!("s37: append order mismatch");
        return false;
    }
    true
}

fn offsets_test() -> bool {
    let data = match open_flags("fs:data", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: offset open failed");
            return false;
        }
    };
    let first = read_expect(data, b"0123");
    let second = read_expect(data, b"4567");
    let _ = close(data);
    let fresh = match open_flags("fs:data", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    let all = read_expect(fresh, b"01234567");
    let _ = close(fresh);
    if !first || !second || !all {
        println!("s37: sequential read mismatch");
        return false;
    }

    let seq = match open_flags("fs:seq", flags(O_RDWR | O_TRUNC)) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: sequential write open failed");
            return false;
        }
    };
    let wrote_first = write(seq, b"abcdef").ok() == Some(6);
    let wrote_second = write(seq, b"gh").ok() == Some(2);
    let _ = close(seq);
    if !wrote_first || !wrote_second {
        println!("s37: sequential write failed");
        return false;
    }
    let check = match open_flags("fs:seq", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    let matched = read_expect(check, b"abcdefgh");
    let _ = close(check);
    if !matched {
        println!("s37: sequential write mismatch");
        return false;
    }
    true
}

fn dup_test() -> bool {
    let first = match open_flags("fs:data", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    if !read_expect(first, b"01") {
        println!("s37: dup read before duplication failed");
        return false;
    }
    let survivor = match dup(first, Rights::ALL.bits()) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: dup failed");
            return false;
        }
    };
    if close(first).is_err() {
        println!("s37: close original failed");
        return false;
    }
    let cursor_ok = read_expect(survivor, b"23");
    if close(survivor).is_err() {
        println!("s37: close survivor failed");
        return false;
    }
    if !cursor_ok {
        println!("s37: survivor lost the cursor");
        return false;
    }
    true
}

fn loop_test() -> bool {
    let open_flags_wire = flags(O_RDWR | O_CREAT);
    let mut cycle = 0usize;
    while cycle < LOOP_CYCLES {
        let file = match open_flags("fs:cycle", open_flags_wire) {
            Ok(handle) => handle,
            Err(_) => {
                println!("s37: cycle open failed at {cycle}");
                return false;
            }
        };
        let duplicate = match dup(file, Rights::ALL.bits()) {
            Ok(handle) => handle,
            Err(_) => {
                println!("s37: cycle dup failed at {cycle}");
                return false;
            }
        };
        if close(file).is_err() || close(duplicate).is_err() {
            println!("s37: cycle close failed at {cycle}");
            return false;
        }
        cycle = cycle.saturating_add(1);
    }
    true
}

fn forged_test() -> bool {
    let handle = match open_flags("fs:data", OpenFlags::READ) {
        Ok(handle) => handle,
        Err(_) => return false,
    };
    let value = handle;
    let _ = close(handle);
    let mut buffer = [0u8; 4];
    if !matches!(read(value, &mut buffer), Err(Errno::EBADF)) {
        println!("s37: closed handle was accepted");
        return false;
    }
    if !matches!(read(Handle(0), &mut buffer), Err(Errno::EBADF)) {
        println!("s37: forged handle was accepted");
        return false;
    }
    true
}

fn main(_args: raven_rt::Args) -> i32 {
    let control = match open_flags("fs:ctl", OpenFlags::READ_WRITE) {
        Ok(handle) => handle,
        Err(_) => {
            println!("s37: control open failed");
            return 1;
        }
    };

    if !flags_test() {
        return 1;
    }
    println!("s37: flags ok");

    if !append_test() {
        return 1;
    }
    println!("s37: append ok");

    if !offsets_test() {
        return 1;
    }
    println!("s37: offsets ok");

    if !dup_test() {
        return 1;
    }
    println!("s37: dup ok");

    if !loop_test() {
        return 1;
    }
    let mut response = [0u8; 64];
    let _ = call(control, b"sync", &mut response);
    let length = match call(control, b"count", &mut response) {
        Ok(length) => length,
        Err(_) => {
            println!("s37: loop count failed");
            return 1;
        }
    };
    let text = core::str::from_utf8(&response[..length]).unwrap_or("bad");
    println!("s37: loop ok");
    println!("s37: loop count {text}");

    if !forged_test() {
        return 1;
    }
    println!("s37: forged ok");

    let _ = close(control);
    0
}
