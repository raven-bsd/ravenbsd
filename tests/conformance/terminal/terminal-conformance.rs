// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S742 terminal conformance probe (test-only, Rust).
//!
//! Replaces the C fixture build so the old `terminal-conformance.c` blob stays byte-identical.
//! It runs the same S175 rows: termios round-trip through `TCSANOW`/`TCSADRAIN`/`TCSAFLUSH`, a pty
//! byte pair in both directions, controlling-terminal and foreground-group changes, `EIO` after
//! the last slave closes, the terminal-generated `SIGWINCH`, `SIGHUP` and `SIGINT` (ISIG on and
//! off) rows, and the target-libc `VINTR`/`VMIN`/`VTIME` regression that compares the Rust target
//! libc constants with the tty's control-character indexes on a real pty.
//!
//! The request encodings and record shape are pinned independently by the runner's host clang
//! oracle against the public headers; this probe exercises the same numbers through libc.
//!
//! The donor contract is FreeBSD `sys/kern/tty.c` (`ttydev_ioctl_spgrp`, `tty_set_winsize`,
//! `tty_signal_sessleader`) and POSIX termios. No donor expression is copied.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_uint, c_ulong, c_void};
use core::fmt;
use core::ptr::{null_mut, slice_from_raw_parts};
use core::sync::atomic::{AtomicI32, AtomicU32, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

const TOPIC: &str = "terminal";

const O_RDWR: c_int = 0o2;
const O_NOCTTY: c_int = 0o400;

const TCSANOW: c_int = 0;
const TCSADRAIN: c_int = 1;
const TCSAFLUSH: c_int = 2;

const NCCS: usize = 20;
const VEOF: usize = 0;
const VERASE: usize = 3;
const VKILL: usize = 5;
const VINTR: usize = 8;
const VQUIT: usize = 9;
const VMIN: usize = 16;
const VTIME: usize = 17;

const ICRNL: u32 = 0x0000_0100;
const IXON: u32 = 0x0000_0200;
const OPOST: u32 = 0x0000_0001;
const ONLCR: u32 = 0x0000_0002;
const CSIZE: u32 = 0x0000_0300;
const CS8: u32 = 0x0000_0300;
const CREAD: u32 = 0x0000_0800;
const PARENB: u32 = 0x0000_1000;
const CLOCAL: u32 = 0x0000_8000;
const ISIG: u32 = 0x0000_0080;
const ICANON: u32 = 0x0000_0100;
const ECHO: u32 = 0x0000_0008;
const ECHOE: u32 = 0x0000_0002;
const ECHOK: u32 = 0x0000_0004;
const ECHONL: u32 = 0x0000_0010;
const IEXTEN: u32 = 0x0000_0400;
const B9600: u32 = 9600;

// The `c_cc` indexes of the Rust target libc as committed in the toolchain. S742 proves that the
// target module and the native tty agree; the build writes this file from
// `toolchain/vendor/libc/src/unix/ravenbsd/mod.rs`, so the test uses the target's own constants
// instead of a copy of the public header.
include!(concat!(env!("TERMIOS_TARGET_CONSTANTS")));

const EPERM: c_int = 1;
const EINTR: c_int = 4;
const EIO: c_int = 5;
const ENXIO: c_int = 6;
const EAGAIN: c_int = 35;

const SIGHUP: c_int = 1;
const SIGINT: c_int = 2;
const SIGWINCH: c_int = 28;

const TIOCSCTTY: c_ulong = 0x2000_7461;
const TIOCSWINSZ: c_ulong = 0x8008_7467;

const IO_TIMEOUT_MS: c_int = 2000;
const CLOCK_MONOTONIC: c_int = 1;

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

static FAILURES: AtomicU32 = AtomicU32::new(0);
static SIGNAL_SEEN: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_cc: [u8; NCCS],
    c_ispeed: u32,
    c_ospeed: u32,
}

impl Termios {
    const ZERO: Self = Self {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_cc: [0; NCCS],
        c_ispeed: 0,
        c_ospeed: 0,
    };
}

const _: () = assert!(core::mem::size_of::<Termios>() == 44);
const _: () = assert!(core::mem::align_of::<Termios>() == 4);

#[repr(C)]
#[derive(Clone, Copy)]
struct Winsize {
    ws_row: u16,
    ws_col: u16,
    ws_xpixel: u16,
    ws_ypixel: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Sigset {
    bytes: [u64; 16],
}

impl Sigset {
    const ZERO: Self = Self { bytes: [0; 16] };
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Sigaction {
    sa_handler: usize,
    sa_mask: Sigset,
    sa_flags: c_int,
    _pad: c_int,
    sa_restorer: usize,
}

const _: () = assert!(core::mem::size_of::<Sigaction>() == 152);

unsafe extern "C" {
    fn posix_openpt(flags: c_int) -> c_int;
    fn grantpt(fd: c_int) -> c_int;
    fn unlockpt(fd: c_int) -> c_int;
    fn ptsname(fd: c_int) -> *mut c_char;
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn poll(fds: *mut PollFd, nfds: c_ulong, timeout: c_int) -> c_int;
    fn tcgetattr(fd: c_int, termios: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, action: c_int, termios: *const Termios) -> c_int;
    fn cfgetispeed(termios: *const Termios) -> c_uint;
    fn cfgetospeed(termios: *const Termios) -> c_uint;
    fn cfsetispeed(termios: *mut Termios, speed: c_uint) -> c_int;
    fn cfsetospeed(termios: *mut Termios, speed: c_uint) -> c_int;
    fn tcgetpgrp(fd: c_int) -> c_int;
    fn tcsetpgrp(fd: c_int, pgrp: c_int) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn getpgrp() -> c_int;
    fn getuid() -> c_uint;
    fn fork() -> c_int;
    fn setsid() -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn usleep(microseconds: c_uint) -> c_int;
    fn clock_gettime(clockid: c_int, value: *mut Timespec) -> c_int;
    fn __errno_location() -> *mut c_int;
    fn _exit(status: c_int) -> !;
}

fn errno() -> c_int {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() }
}

fn set_errno(value: c_int) {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() = value };
}

fn report(name: &str, status: &str, detail: fmt::Arguments<'_>) {
    conformance::emit(format_args!(
        "S175-TERM row={} status={} detail={}\n",
        name, status, detail
    ));
}

fn pass(name: &str, detail: fmt::Arguments<'_>) {
    report(name, "PASS", detail);
}

fn fail(name: &str, detail: fmt::Arguments<'_>) {
    FAILURES.fetch_add(1, Ordering::SeqCst);
    report(name, "FAIL", detail);
}

/// One pty pair, with the slave path retained for the child rows.
struct Pair {
    master: c_int,
    slave: c_int,
    name: [u8; 64],
}

impl Pair {
    fn open() -> Option<Self> {
        let master = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
        if master < 0 {
            return None;
        }
        if unsafe { grantpt(master) } != 0 || unsafe { unlockpt(master) } != 0 {
            unsafe { close(master) };
            return None;
        }
        let path = unsafe { ptsname(master) };
        if path.is_null() || !has_prefix(path, b"/dev/pts/") {
            unsafe { close(master) };
            return None;
        }
        let slave = unsafe { open(path, O_RDWR | O_NOCTTY) };
        if slave < 0 {
            unsafe { close(master) };
            return None;
        }
        let mut name = [0u8; 64];
        let mut index = 0;
        while index + 1 < name.len() {
            // SAFETY: `path` is a NUL-terminated libc-owned string.
            let byte = unsafe { *path.add(index) };
            if byte == 0 {
                break;
            }
            name[index] = byte as u8;
            index += 1;
        }
        Some(Self {
            master,
            slave,
            name,
        })
    }

    fn close(&self) {
        unsafe {
            close(self.master);
            close(self.slave);
        }
    }
}

fn has_prefix(pointer: *const c_char, prefix: &[u8]) -> bool {
    for (index, expected) in prefix.iter().enumerate() {
        // SAFETY: the caller supplies a NUL-terminated string.
        if unsafe { *pointer.add(index) } as u8 != *expected {
            return false;
        }
    }
    true
}

fn c_str_eq(pointer: *const c_char, expected: &[u8]) -> bool {
    let mut cursor = pointer as *const u8;
    for byte in expected {
        // SAFETY: the caller supplies a NUL-terminated argv entry.
        if unsafe { *cursor } != *byte {
            return false;
        }
        // SAFETY: advance within the same NUL-terminated string.
        cursor = unsafe { cursor.add(1) };
    }
    // SAFETY: the expected text is a prefix; the next byte must end the string.
    unsafe { *cursor == 0 }
}

fn read_exact(fd: c_int, buffer: &mut [u8], timeout_ms: c_int) -> bool {
    let mut offset = 0;
    while offset < buffer.len() {
        let mut poll_fd = PollFd {
            fd,
            events: 1,
            revents: 0,
        };
        if unsafe { poll(&mut poll_fd, 1, timeout_ms) } <= 0 {
            return false;
        }
        let count = unsafe {
            read(
                fd,
                buffer[offset..].as_mut_ptr().cast::<c_void>(),
                buffer.len() - offset,
            )
        };
        if count > 0 {
            offset += count as usize;
            continue;
        }
        if count < 0 && matches!(errno(), EAGAIN | EINTR) {
            continue;
        }
        return false;
    }
    true
}

fn make_raw(fd: c_int) -> bool {
    let mut termios = Termios::ZERO;
    if unsafe { tcgetattr(fd, &mut termios) } != 0 {
        return false;
    }
    // IGNBRK|BRKINT|PARMRK|ISTRIP|INLCR|IGNCR|ICRNL|IXON, OPOST, and
    // ECHO|ECHONL|ICANON|ISIG|IEXTEN (the public `<bits/termios.h>` values).
    termios.c_iflag &= !(0x1 | 0x2 | 0x8 | 0x20 | 0x40 | 0x80 | 0x100 | 0x200);
    termios.c_oflag &= !OPOST;
    termios.c_lflag &= !(ECHO | ECHONL | ICANON | ISIG | IEXTEN);
    termios.c_cflag &= !(CSIZE | PARENB);
    termios.c_cflag |= CS8;
    termios.c_cc[VMIN] = 1;
    termios.c_cc[VTIME] = 0;
    unsafe { tcsetattr(fd, TCSANOW, &termios) == 0 }
}

enum Mismatch {
    Field(&'static str, u32, u32),
    ISpeed(u32, u32),
    OSpeed(u32, u32),
    Control(usize, u8, u8),
}

fn cmp_termios(want: &Termios, got: &Termios) -> Option<Mismatch> {
    for (name, left, right) in [
        ("c_iflag", want.c_iflag, got.c_iflag),
        ("c_oflag", want.c_oflag, got.c_oflag),
        ("c_cflag", want.c_cflag, got.c_cflag),
        ("c_lflag", want.c_lflag, got.c_lflag),
    ] {
        if left != right {
            return Some(Mismatch::Field(name, left, right));
        }
    }
    let want_in = unsafe { cfgetispeed(want) };
    let got_in = unsafe { cfgetispeed(got) };
    if want_in != got_in {
        return Some(Mismatch::ISpeed(want_in, got_in));
    }
    let want_out = unsafe { cfgetospeed(want) };
    let got_out = unsafe { cfgetospeed(got) };
    if want_out != got_out {
        return Some(Mismatch::OSpeed(want_out, got_out));
    }
    for index in 0..NCCS {
        if want.c_cc[index] != got.c_cc[index] {
            return Some(Mismatch::Control(index, want.c_cc[index], got.c_cc[index]));
        }
    }
    None
}

fn row_termios(mutation: bool) {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("termios", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let mut original = Termios::ZERO;
    if unsafe { tcgetattr(pair.slave, &mut original) } != 0 {
        let error = errno();
        pair.close();
        fail("termios", format_args!("tcgetattr(slave) errno={error}"));
        return;
    }
    let mut want = original;
    want.c_iflag = ICRNL | IXON;
    want.c_oflag = OPOST | ONLCR;
    want.c_lflag = ISIG | ICANON | ECHO | ECHOE | ECHOK | IEXTEN;
    want.c_cflag = (original.c_cflag & !(CSIZE | PARENB)) | CS8 | CREAD | CLOCAL;
    want.c_cc[VINTR] = 3;
    want.c_cc[VQUIT] = 28;
    want.c_cc[VERASE] = 127;
    want.c_cc[VKILL] = 21;
    want.c_cc[VEOF] = 4;
    want.c_cc[VMIN] = 1;
    want.c_cc[VTIME] = 0;
    unsafe {
        cfsetispeed(&mut want, B9600);
        cfsetospeed(&mut want, B9600);
    }
    if unsafe { tcsetattr(pair.slave, TCSANOW, &want) } != 0 {
        let error = errno();
        pair.close();
        fail("termios", format_args!("tcsetattr TCSANOW errno={error}"));
        return;
    }
    let mut back = Termios::ZERO;
    let mut master_back = Termios::ZERO;
    let slave_ok = unsafe { tcgetattr(pair.slave, &mut back) } == 0;
    let master_ok = unsafe { tcgetattr(pair.master, &mut master_back) } == 0;
    let mismatch = if slave_ok {
        cmp_termios(&want, &back)
    } else {
        Some(Mismatch::Field("tcgetattr", 0, 1))
    };
    let mismatch = match mismatch {
        None if master_ok => cmp_termios(&want, &master_back),
        other => other,
    };
    if let Some(mismatch) = mismatch {
        pair.close();
        match mismatch {
            Mismatch::Field(name, left, right) => {
                fail("termios", format_args!("{} want={} got={}", name, left, right));
            }
            Mismatch::ISpeed(left, right) => {
                fail("termios", format_args!("ispeed want={left} got={right}"));
            }
            Mismatch::OSpeed(left, right) => {
                fail("termios", format_args!("ospeed want={left} got={right}"));
            }
            Mismatch::Control(index, left, right) => {
                fail("termios", format_args!("c_cc[{index}] want={left} got={right}"));
            }
        }
        return;
    }
    if mutation {
        let mut wrong = want;
        wrong.c_lflag ^= ECHO;
        let detected = cmp_termios(&wrong, &back).is_some();
        pair.close();
        if detected {
            fail("termios", format_args!("mutation: ECHO expected off but the row passed"));
        } else {
            fail("termios", format_args!("mutation: ECHO corruption was not detected"));
        }
        return;
    }
    // TCSADRAIN: a queued slave->master byte must survive the change.
    let mut drain = want;
    drain.c_oflag &= !0x1;
    let wrote = unsafe { write(pair.slave, b"drain".as_ptr().cast::<c_void>(), 5) } == 5;
    let mut buffer = [0u8; 8];
    let drained = wrote
        && unsafe { tcsetattr(pair.slave, TCSADRAIN, &drain) } == 0
        && read_exact(pair.master, &mut buffer[..5], IO_TIMEOUT_MS)
        && &buffer[..5] == b"drain";
    if !drained {
        pair.close();
        fail("termios", format_args!("TCSADRAIN lost or changed the queued bytes"));
        return;
    }
    // TCSAFLUSH: the record is applied and remains readable.
    let flushed = unsafe { tcsetattr(pair.slave, TCSAFLUSH, &want) } == 0
        && unsafe { tcgetattr(pair.slave, &mut back) } == 0
        && cmp_termios(&want, &back).is_none();
    pair.close();
    if flushed {
        pass(
            "termios",
            format_args!("TCSANOW/TCSADRAIN/TCSAFLUSH round-trip of iflag/oflag/lflag/cflag/c_cc/9600"),
        );
    } else {
        fail("termios", format_args!("TCSAFLUSH round-trip failed"));
    }
}

fn row_pair() {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("pty-both-ways", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    if !make_raw(pair.slave) {
        let error = errno();
        pair.close();
        fail("pty-both-ways", format_args!("raw mode errno={error}"));
        return;
    }
    let to_slave = b"master->slave-0123456789";
    let to_master = b"slave->master-9876543210";
    let mut buffer = [0u8; 64];
    let forwards = unsafe { write(pair.master, to_slave.as_ptr().cast::<c_void>(), to_slave.len()) }
        == to_slave.len() as isize
        && read_exact(pair.slave, &mut buffer[..to_slave.len()], IO_TIMEOUT_MS)
        && &buffer[..to_slave.len()] == to_slave;
    let backwards = forwards
        && unsafe { write(pair.slave, to_master.as_ptr().cast::<c_void>(), to_master.len()) }
            == to_master.len() as isize
        && read_exact(pair.master, &mut buffer[..to_master.len()], IO_TIMEOUT_MS)
        && &buffer[..to_master.len()] == to_master;
    let error = errno();
    pair.close();
    if backwards {
        pass(
            "pty-both-ways",
            format_args!(
                "exact {} bytes master->slave and {} bytes slave->master",
                to_slave.len(),
                to_master.len()
            ),
        );
    } else {
        fail("pty-both-ways", format_args!("byte mismatch or timeout errno={error}"));
    }
}

fn child_ctty(name: &[u8; 64]) -> ! {
    let path = name.as_ptr().cast::<c_char>();
    unsafe {
        if setsid() < 0 {
            _exit(11);
        }
        set_errno(0);
        let tty = open(c"/dev/tty".as_ptr(), O_RDWR);
        if tty >= 0 {
            close(tty);
            _exit(12);
        }
        if errno() != ENXIO {
            _exit(13);
        }
        let fd = open(path, O_RDWR);
        if fd < 0 {
            _exit(14);
        }
        if ioctl(fd, TIOCSCTTY) != 0 {
            _exit(15);
        }
        if tcgetpgrp(fd) != getpgrp() {
            _exit(18);
        }
        if tcsetpgrp(fd, getpgrp()) != 0 {
            _exit(19);
        }
        set_errno(0);
        if tcsetpgrp(fd, 0) != -1 || errno() != EPERM {
            _exit(20);
        }
        let tty = open(c"/dev/tty".as_ptr(), O_RDWR);
        if tty < 0 {
            _exit(16);
        }
        if write(tty, b"CTTY".as_ptr().cast::<c_void>(), 4) != 4 {
            _exit(17);
        }
        _exit(0);
    }
}

fn row_ctty() {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("ctty", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let child = unsafe { fork() };
    if child < 0 {
        let error = errno();
        pair.close();
        fail("ctty", format_args!("fork errno={error}"));
        return;
    }
    if child == 0 {
        unsafe {
            close(pair.master);
            close(pair.slave);
        }
        child_ctty(&pair.name);
    }
    let mut status = 0;
    if unsafe { waitpid(child, &mut status, 0) } != child {
        let error = errno();
        pair.close();
        fail("ctty", format_args!("waitpid errno={error}"));
        return;
    }
    let mut buffer = [0u8; 8];
    let routed = read_exact(pair.master, &mut buffer[..4], IO_TIMEOUT_MS) && &buffer[..4] == b"CTTY";
    let code = if status & 0x7f == 0 { (status >> 8) & 0xff } else { -1 };
    pair.close();
    if code == 0 && routed {
        pass(
            "ctty",
            format_args!("setsid -> /dev/tty ENXIO -> TIOCSCTTY -> /dev/tty open and routed"),
        );
    } else {
        fail(
            "ctty",
            format_args!(
                "child exit={} routed={} (11 setsid,12 ctty,13 errno,14 open,15 sctty,16 tty,17 write,18 getpgrp,19 setpgrp,20 unauthorized)",
                code,
                if routed { 1 } else { 0 }
            ),
        );
    }
}

fn row_eio() {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("eio", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    unsafe { close(pair.slave) };
    let mut poll_fd = PollFd {
        fd: pair.master,
        events: 1,
        revents: 0,
    };
    let ready = unsafe { poll(&mut poll_fd, 1, IO_TIMEOUT_MS) };
    set_errno(0);
    let mut byte = 0u8;
    let count = if ready > 0 {
        unsafe { read(pair.master, (&mut byte as *mut u8).cast::<c_void>(), 1) }
    } else {
        -1
    };
    let error = errno();
    unsafe { close(pair.master) };
    if count < 0 && error == EIO {
        pass("eio", format_args!("read(master) after the last slave close returns EIO"));
    } else {
        fail("eio", format_args!("poll={ready} read={count} errno={error} (want -1/EIO)"));
    }
}

extern "C" fn note_signal(signal: c_int) {
    SIGNAL_SEEN.store(signal, Ordering::SeqCst);
}

fn setup_winsize(_fd: c_int) -> c_int {
    0
}

fn setup_isig(fd: c_int, enabled: bool) -> c_int {
    let mut termios = Termios::ZERO;
    if unsafe { tcgetattr(fd, &mut termios) } != 0 {
        return -1;
    }
    if enabled {
        termios.c_lflag |= ISIG;
    } else {
        termios.c_lflag &= !ISIG;
    }
    termios.c_lflag &= !ICANON;
    termios.c_lflag &= !ECHO;
    termios.c_cc[VINTR] = 3;
    termios.c_cc[VMIN] = 1;
    termios.c_cc[VTIME] = 0;
    unsafe { tcsetattr(fd, TCSANOW, &termios) }
}

fn setup_isig_on(fd: c_int) -> c_int {
    setup_isig(fd, true)
}

fn setup_isig_off(fd: c_int) -> c_int {
    setup_isig(fd, false)
}

struct Signals {
    ready: c_int,
    result: c_int,
    child: c_int,
}

fn run_signal_child(pair: &Pair, setup: fn(c_int) -> c_int, signal: c_int) -> Option<Signals> {
    let mut ready: [c_int; 2] = [0; 2];
    let mut result: [c_int; 2] = [0; 2];
    if unsafe { pipe(ready.as_mut_ptr()) } != 0 || unsafe { pipe(result.as_mut_ptr()) } != 0 {
        return None;
    }
    let child = unsafe { fork() };
    if child < 0 {
        return None;
    }
    if child == 0 {
        unsafe {
            close(pair.master);
            close(pair.slave);
            close(ready[0]);
            close(result[0]);
            if setsid() < 0 {
                _exit(20);
            }
            let fd = open(pair.name.as_ptr().cast::<c_char>(), O_RDWR);
            if fd < 0 {
                _exit(21);
            }
            if ioctl(fd, TIOCSCTTY) != 0 {
                _exit(22);
            }
            // A session leader that acquires the controlling terminal also makes its group
            // the foreground group (POSIX); the shell does this with tcsetpgrp after
            // TIOCSCTTY, and the pty server needs the accepted group to address signals.
            let pgrp = getpgrp();
            if pgrp <= 0 || tcsetpgrp(fd, pgrp) != 0 {
                _exit(26);
            }
            if setup(fd) != 0 {
                _exit(23);
            }
        }
        SIGNAL_SEEN.store(0, Ordering::SeqCst);
        let action = Sigaction {
            sa_handler: note_signal as *const () as usize,
            sa_mask: Sigset::ZERO,
            sa_flags: 0,
            _pad: 0,
            sa_restorer: 0,
        };
        if unsafe { sigaction(signal, &action, null_mut()) } != 0 {
            unsafe { _exit(24) };
        }
        if unsafe { write(ready[1], b"R".as_ptr().cast::<c_void>(), 1) } != 1 {
            unsafe { _exit(25) };
        }
        unsafe { close(ready[1]) };
        for _ in 0..400 {
            if SIGNAL_SEEN.load(Ordering::SeqCst) != 0 {
                break;
            }
            unsafe { usleep(5000) };
        }
        let seen = SIGNAL_SEEN.load(Ordering::SeqCst);
        unsafe {
            write(
                result[1],
                (&seen as *const c_int).cast::<c_void>(),
                core::mem::size_of::<c_int>(),
            );
            _exit(0);
        }
    }
    unsafe {
        close(ready[1]);
        close(result[1]);
    }
    Some(Signals {
        ready: ready[0],
        result: result[0],
        child,
    })
}

fn wait_ready(fd: c_int) {
    let mut poll_fd = PollFd {
        fd,
        events: 1,
        revents: 0,
    };
    if unsafe { poll(&mut poll_fd, 1, IO_TIMEOUT_MS) } > 0 {
        let mut marker = 0u8;
        unsafe { read(fd, (&mut marker as *mut u8).cast::<c_void>(), 1) };
    }
}

fn collect_signal(fd: c_int) -> c_int {
    let mut poll_fd = PollFd {
        fd,
        events: 1,
        revents: 0,
    };
    if unsafe { poll(&mut poll_fd, 1, IO_TIMEOUT_MS) } > 0 {
        let mut value: c_int = 0;
        let count = unsafe {
            read(
                fd,
                (&mut value as *mut c_int).cast::<c_void>(),
                core::mem::size_of::<c_int>(),
            )
        };
        if count == core::mem::size_of::<c_int>() as isize {
            return value;
        }
    }
    0
}

fn row_sigwinch() {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("sigwinch", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let signals = match run_signal_child(&pair, setup_winsize, SIGWINCH) {
        Some(signals) => signals,
        None => {
            let error = errno();
            pair.close();
            fail("sigwinch", format_args!("fork/pipe errno={error}"));
            return;
        }
    };
    wait_ready(signals.ready);
    let window = Winsize {
        ws_row: 24,
        ws_col: 80,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    let set = unsafe { ioctl(pair.slave, TIOCSWINSZ, &window) };
    let received = collect_signal(signals.result);
    let mut status = 0;
    unsafe {
        waitpid(signals.child, &mut status, 0);
        close(signals.ready);
        close(signals.result);
    }
    pair.close();
    if set == 0 && received == SIGWINCH {
        pass(
            "sigwinch",
            format_args!("TIOCSWINSZ delivered SIGWINCH to the foreground group"),
        );
    } else {
        fail(
            "sigwinch",
            format_args!("ioctl={set} signal={received} (want SIGWINCH)"),
        );
    }
}

fn row_sighup() {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("sighup", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let signals = match run_signal_child(&pair, setup_winsize, SIGHUP) {
        Some(signals) => signals,
        None => {
            let error = errno();
            pair.close();
            fail("sighup", format_args!("fork/pipe errno={error}"));
            return;
        }
    };
    wait_ready(signals.ready);
    unsafe { close(pair.master) };
    let received = collect_signal(signals.result);
    let mut status = 0;
    unsafe {
        waitpid(signals.child, &mut status, 0);
        close(signals.ready);
        close(signals.result);
        close(pair.slave);
    }
    if received == SIGHUP {
        pass(
            "sighup",
            format_args!("master close delivered SIGHUP to the session leader"),
        );
    } else {
        fail("sighup", format_args!("signal={received} (want SIGHUP)"));
    }
}

fn row_isig(enabled: bool) {
    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("isig", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let setup = if enabled { setup_isig_on } else { setup_isig_off };
    let signals = match run_signal_child(&pair, setup, SIGINT) {
        Some(signals) => signals,
        None => {
            let error = errno();
            pair.close();
            fail("isig", format_args!("fork/pipe errno={error}"));
            return;
        }
    };
    wait_ready(signals.ready);
    let intr = 3u8;
    let written = unsafe { write(pair.master, (&intr as *const u8).cast::<c_void>(), 1) };
    let received = collect_signal(signals.result);
    let mut status = 0;
    unsafe {
        waitpid(signals.child, &mut status, 0);
        close(signals.ready);
        close(signals.result);
    }
    pair.close();
    let error = errno();
    if written != 1 {
        fail("isig", format_args!("write ^C errno={error}"));
    } else if enabled && received == SIGINT {
        pass("isig", format_args!("^C with ISIG set generated SIGINT"));
    } else if !enabled && received != SIGINT {
        pass("isig", format_args!("^C with ISIG cleared did not generate SIGINT"));
    } else {
        fail(
            "isig",
            format_args!("ISIG={} signal={received}", if enabled { 1 } else { 0 }),
        );
    }
}

const DISTINCTIVE_INTR: u8 = 0x07;
const DEFAULT_INTR: u8 = 0x03;

/// Configure `VINTR` at the target libc index to a distinctive byte, with the target
/// `VMIN`/`VTIME` indexes in raw mode.
fn setup_target_vintr(fd: c_int) -> c_int {
    let mut termios = Termios::ZERO;
    if unsafe { tcgetattr(fd, &mut termios) } != 0 {
        return -1;
    }
    termios.c_lflag |= ISIG;
    termios.c_lflag &= !ICANON;
    termios.c_lflag &= !ECHO;
    termios.c_cc[TARGET_VINTR] = DISTINCTIVE_INTR;
    termios.c_cc[TARGET_VMIN] = 1;
    termios.c_cc[TARGET_VTIME] = 0;
    unsafe { tcsetattr(fd, TCSANOW, &termios) }
}

fn configure_raw(fd: c_int, vmin: u8, vtime: u8) -> bool {
    let mut termios = Termios::ZERO;
    if unsafe { tcgetattr(fd, &mut termios) } != 0 {
        return false;
    }
    termios.c_lflag &= !ICANON;
    termios.c_lflag &= !ECHO;
    termios.c_cc[TARGET_VMIN] = vmin;
    termios.c_cc[TARGET_VTIME] = vtime;
    unsafe { tcsetattr(fd, TCSANOW, &termios) == 0 }
}

fn target_raw_mode(fd: c_int) -> bool {
    if setup_target_vintr(fd) != 0 {
        return false;
    }
    let mut back = Termios::ZERO;
    if unsafe { tcgetattr(fd, &mut back) } != 0 {
        return false;
    }
    back.c_cc[TARGET_VINTR] == DISTINCTIVE_INTR
        && back.c_cc[TARGET_VMIN] == 1
        && back.c_cc[TARGET_VTIME] == 0
}

fn signal_exchange(pair: &Pair, byte: u8) -> Option<c_int> {
    let signals = run_signal_child(pair, setup_target_vintr, SIGINT)?;
    wait_ready(signals.ready);
    let written = unsafe { write(pair.master, (&byte as *const u8).cast::<c_void>(), 1) };
    let received = collect_signal(signals.result);
    let mut status = 0;
    unsafe {
        waitpid(signals.child, &mut status, 0);
        close(signals.ready);
        close(signals.result);
    }
    if written == 1 { Some(received) } else { None }
}

fn now_ms() -> i64 {
    let mut value = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid writable timespec for the duration of the call.
    unsafe {
        clock_gettime(CLOCK_MONOTONIC, &mut value);
    }
    value.tv_sec.saturating_mul(1000) + value.tv_nsec / 1_000_000
}

/// Real-pty regression for the Rust target libc `c_cc` indexes.
///
/// The tty interrupts on its private `VINTR` byte. A program built against the target libc
/// writes `0x07` at `TARGET_VINTR`; the TTY must deliver SIGINT for that byte and must not
/// deliver it for the inherited `^C` after `VINTR` is `0x07`. The target `VMIN`/`VTIME`
/// indexes must also implement the POSIX timing: `VMIN=1/VTIME=0` returns one byte at once,
/// `VMIN=0/VTIME=1` returns 0 after about 100 ms.
fn row_target_vintr() {
    let mut detail = "ok";
    let mut ok = true;

    let pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("target-vintr", format_args!("open pair errno={}", errno()));
            return;
        }
    };
    let raw = target_raw_mode(pair.slave);
    let target = signal_exchange(&pair, DISTINCTIVE_INTR);
    pair.close();
    if !raw {
        ok = false;
        detail = "read-back";
    }
    if target != Some(SIGINT) {
        ok = false;
        detail = "distinctive byte did not deliver SIGINT";
    }

    let old_pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("target-vintr", format_args!("old pair errno={}", errno()));
            return;
        }
    };
    let old = signal_exchange(&old_pair, DEFAULT_INTR);
    old_pair.close();
    if old != Some(0) {
        ok = false;
        detail = "inherited ^C still delivered SIGINT";
    }

    let immediate_pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("target-vintr", format_args!("vmin pair errno={}", errno()));
            return;
        }
    };
    let immediate = configure_raw(immediate_pair.slave, 1, 0);
    let sent = unsafe {
        write(
            immediate_pair.master,
            (&b'x' as *const u8).cast::<c_void>(),
            1,
        )
    };
    let start = now_ms();
    let mut buffer = [0u8; 4];
    let count = unsafe {
        read(
            immediate_pair.slave,
            buffer.as_mut_ptr().cast::<c_void>(),
            buffer.len(),
        )
    };
    let elapsed = now_ms().saturating_sub(start);
    immediate_pair.close();
    if !immediate || sent != 1 || count != 1 || elapsed >= 100 {
        ok = false;
        detail = "VMIN=1/VTIME=0 did not return one byte immediately";
    }

    let timed_pair = match Pair::open() {
        Some(pair) => pair,
        None => {
            fail("target-vintr", format_args!("vtime pair errno={}", errno()));
            return;
        }
    };
    let timed = configure_raw(timed_pair.slave, 0, 1);
    let start = now_ms();
    let count = unsafe {
        read(
            timed_pair.slave,
            buffer.as_mut_ptr().cast::<c_void>(),
            buffer.len(),
        )
    };
    let elapsed = now_ms().saturating_sub(start);
    timed_pair.close();
    if !timed || count != 0 || elapsed < 50 || elapsed > 500 {
        ok = false;
        detail = "VMIN=0/VTIME=1 did not return 0 after about 100 ms";
    }

    if ok {
        pass(
            "target-vintr",
            format_args!(
                "target VINTR={TARGET_VINTR} byte 0x07 delivered SIGINT; inherited ^C did not; VMIN={TARGET_VMIN}/VTIME={TARGET_VTIME} timing ok"
            ),
        );
    } else {
        fail(
            "target-vintr",
            format_args!(
                "{detail} (target VINTR={TARGET_VINTR} raw={raw} distinctive={target:?} old={old:?})"
            ),
        );
    }
}

fn slice_bytes<T>(pointer: *const T, count: usize) -> &'static [T] {
    // SAFETY: the caller keeps the pointed-to storage alive for the process lifetime.
    unsafe { &*slice_from_raw_parts(pointer, count) }
}

fn run_pass(run_signals: bool, mutation_termios: bool) {
    conformance::emit(format_args!(
        "S175-TERM start uid={} signals={} mutation={}\n",
        unsafe { getuid() },
        if run_signals { 1 } else { 0 },
        if mutation_termios { 1 } else { 0 }
    ));
    let _ = TOPIC;
    row_termios(mutation_termios);
    row_pair();
    row_ctty();
    row_eio();
    row_target_vintr();
    if run_signals {
        row_sigwinch();
        row_sighup();
        row_isig(true);
        row_isig(false);
    } else {
        report("sigwinch", "PENDING", format_args!("pass --signals to bind"));
        report("sighup", "PENDING", format_args!("pass --signals to bind"));
        report("isig", "PENDING", format_args!("pass --signals to bind"));
    }
    let failures = FAILURES.load(Ordering::SeqCst);
    conformance::emit(format_args!("S175-TERM done failures={failures}\n"));
}

extern "C" fn run(argc: c_int, argv: *const *const c_char) -> c_int {
    let args = slice_bytes(argv, argc as usize);
    let mut run_signals = false;
    let mut mutation_termios = false;
    let mut both = false;
    for argument in &args[1..] {
        if c_str_eq(*argument, b"--signals") {
            run_signals = true;
        } else if c_str_eq(*argument, b"--mutation-termios") {
            mutation_termios = true;
        } else if c_str_eq(*argument, b"--both") {
            both = true;
        }
    }
    if both {
        run_pass(run_signals, false);
        conformance::emit(format_args!("S175-TERM-MUTATION-BEGIN\n"));
        FAILURES.store(0, Ordering::SeqCst);
        run_pass(run_signals, true);
        let failures = FAILURES.load(Ordering::SeqCst);
        return if failures == 0 { 0 } else { 1 };
    }
    run_pass(run_signals, mutation_termios);
    let failures = FAILURES.load(Ordering::SeqCst);
    if failures == 0 { 0 } else { 1 }
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    run(argc, argv)
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("S175-TERM panic\n"));
    // SAFETY: the process is aborting and `_exit` never returns.
    unsafe { _exit(101) }
}
