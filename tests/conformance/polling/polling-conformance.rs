// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S173 polling conformance probe (test-only, Rust).
//!
//! Runs the POSIX polling contract on the release image: poll and select report a pipe's
//! readable level only after a byte is written, report POLLHUP after the writer closes,
//! return 0 from a bounded timeout, return EINTR for a signal with a handler, report a socket
//! error as POLLERR with one SO_ERROR consumption, and make eventfd/timerfd readable exactly
//! when their counter/expiry condition holds.
//!
//! The donor contract is FreeBSD sys/kern/sys_generic.c (poll/select), sys/kern/sys_event.c
//! (eventfd) and sys/kern/kern_timer.c (timerfd readiness).  No donor expression is copied;
//! the rows assert the observable revents bits and timeout result.
//!
//! The controlled mutation encodes what a poll that drops hangup would believe: a closed
//! writer is not POLLHUP.  The correct product reports it, so the targeted row turns red.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "polling";

const POLLIN: i16 = 0x1;
const POLLOUT: i16 = 0x4;
const POLLERR: i16 = 0x8;
const POLLHUP: i16 = 0x10;

const O_NONBLOCK: c_int = 0o4000;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;

const AF_INET: c_int = 2;
const SOCK_STREAM: c_int = 1;
const SOL_SOCKET: c_int = 1;
const SO_ERROR: c_int = 4;

const CLOCK_MONOTONIC: c_int = 1;

const EINTR: c_int = 4;
const EAGAIN: c_int = 11;
const ECONNREFUSED: c_int = 111;
const EINPROGRESS: c_int = 115;

const SIGALRM: c_int = 14;

static SIGALRM_SEEN: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct Pollfd {
    fd: c_int,
    events: i16,
    revents: i16,
}

const _: () = assert!(core::mem::size_of::<Pollfd>() == 8);

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Timeval {
    tv_sec: i64,
    tv_usec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Itimerspec {
    it_interval: Timespec,
    it_value: Timespec,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct FdSet {
    fds_bits: [u64; 16],
}

impl FdSet {
    const fn zero() -> Self {
        Self { fds_bits: [0; 16] }
    }

    fn set(&mut self, fd: c_int) {
        let fd = fd as usize;
        if fd < self.fds_bits.len() * 64 {
            self.fds_bits[fd / 64] |= 1u64 << (fd % 64);
        }
    }

    fn is_set(&self, fd: c_int) -> bool {
        let fd = fd as usize;
        fd < self.fds_bits.len() * 64 && self.fds_bits[fd / 64] & (1u64 << (fd % 64)) != 0
    }
}

/// The public 128-byte `sigset_t`.
#[repr(C)]
#[derive(Clone, Copy)]
struct Sigset {
    bytes: [u64; 16],
}

impl Sigset {
    const ZERO: Self = Self { bytes: [0; 16] };
}

/// The public 152-byte C `struct sigaction`.
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

#[repr(C)]
#[derive(Clone, Copy)]
struct SockaddrIn {
    sin_family: u16,
    sin_port: u16,
    sin_addr: u32,
    sin_zero: [u8; 8],
}

const _: () = assert!(core::mem::size_of::<SockaddrIn>() == 16);

unsafe extern "C" {
    fn poll(fds: *mut Pollfd, nfds: usize, timeout: c_int) -> c_int;
    fn select(
        nfds: c_int,
        readfds: *mut FdSet,
        writefds: *mut FdSet,
        exceptfds: *mut FdSet,
        timeout: *mut Timeval,
    ) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn eventfd(initval: c_uint, flags: c_int) -> c_int;
    fn eventfd_write(fd: c_int, value: u64) -> c_int;
    fn eventfd_read(fd: c_int, value: *mut u64) -> c_int;
    fn timerfd_create(clock: c_int, flags: c_int) -> c_int;
    fn timerfd_settime(
        fd: c_int,
        flags: c_int,
        new: *const Itimerspec,
        old: *mut Itimerspec,
    ) -> c_int;
    fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
    fn connect(fd: c_int, address: *const SockaddrIn, length: u32) -> c_int;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn getsockopt(
        fd: c_int,
        level: c_int,
        name: c_int,
        value: *mut c_void,
        length: *mut u32,
    ) -> c_int;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
    fn alarm(seconds: c_uint) -> c_uint;
    fn clock_gettime(clock: c_int, time: *mut Timespec) -> c_int;
    fn __errno_location() -> *mut c_int;
}

fn errno() -> c_int {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() }
}

fn set_errno(value: c_int) {
    // SAFETY: the libc entry point returns this thread's errno slot.
    unsafe { *__errno_location() = value };
}

fn flag(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

fn write_all(fd: c_int, mut data: &[u8]) -> isize {
    let mut total = 0isize;
    while !data.is_empty() {
        // SAFETY: `data` is readable for the length passed.
        let count = unsafe { write(fd, data.as_ptr().cast::<c_void>(), data.len()) };
        if count < 0 {
            if errno() == EINTR {
                continue;
            }
            return -1;
        }
        if count == 0 {
            break;
        }
        total += count;
        data = &data[count as usize..];
    }
    total
}

fn monotonic_ms() -> i64 {
    let mut time = Timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: the output is a writable Timespec.
    if unsafe { clock_gettime(CLOCK_MONOTONIC, &mut time) } != 0 {
        return -1;
    }
    time.tv_sec
        .saturating_mul(1000)
        .saturating_add(time.tv_nsec / 1_000_000)
}

fn install_sigalrm() -> bool {
    let mut action = Sigaction {
        sa_handler: note_sigalrm as *const () as usize,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: the action and mask are fully initialized.
    unsafe {
        sigemptyset(&mut action.sa_mask);
        sigaction(SIGALRM, &action, null_mut()) == 0
    }
}

extern "C" fn note_sigalrm(_signal: c_int) {
    SIGALRM_SEEN.fetch_add(1, Ordering::SeqCst);
}

fn row_pipe_readable(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let mut before = Pollfd {
        fd: pipefd[0],
        events: POLLIN,
        revents: 0,
    };
    let before_result = if created {
        unsafe { poll(&mut before, 1, 0) }
    } else {
        -1
    };
    let wrote = created && write_all(pipefd[1], b"x") == 1;
    let mut after = Pollfd {
        fd: pipefd[0],
        events: POLLIN,
        revents: 0,
    };
    let after_result = if wrote {
        unsafe { poll(&mut after, 1, 0) }
    } else {
        -1
    };
    let ok = created
        && before_result == 0
        && before.revents == 0
        && wrote
        && after_result == 1
        && after.revents & POLLIN != 0;
    state.check(
        "pipe-readable",
        ok,
        format_args!(
            "created={} before={}/0x{:x} wrote={} after={}/0x{:x}",
            flag(created),
            before_result,
            before.revents,
            flag(wrote),
            after_result,
            after.revents
        ),
    );
    for fd in pipefd {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn row_pipe_hup(state: &mut State, mutation: bool) {
    let mut pipefd = [-1i32; 2];
    let mut created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let mut byte = [0u8; 1];
    created = created && write_all(pipefd[1], b"x") == 1;
    let drained = created
        && unsafe { read(pipefd[0], byte.as_mut_ptr().cast::<c_void>(), 1) } == 1
        && byte[0] == b'x';
    let closed = drained && unsafe { close(pipefd[1]) } == 0;
    pipefd[1] = -1;
    let mut entry = Pollfd {
        fd: pipefd[0],
        events: POLLIN,
        revents: 0,
    };
    let result = if closed {
        unsafe { poll(&mut entry, 1, 0) }
    } else {
        -1
    };
    let reported_hup = entry.revents & POLLHUP != 0;
    // The mutation expects a poll that drops hangup: the correct product reports POLLHUP, so
    // the targeted row turns red.
    let expect_hup = !mutation;
    let ok = closed && result == 1 && reported_hup == expect_hup && entry.revents & POLLIN == 0;
    state.check(
        "pipe-hup",
        ok,
        format_args!(
            "created={} drained={} closed={} result={} revents=0x{:x} expect_hup={} mutation={}",
            flag(created),
            flag(drained),
            flag(closed),
            result,
            entry.revents,
            flag(expect_hup),
            flag(mutation)
        ),
    );
    if pipefd[0] >= 0 {
        unsafe {
            close(pipefd[0]);
        }
    }
}

fn row_select_readable(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let wrote = created && write_all(pipefd[1], b"y") == 1;
    let mut readfds = FdSet::zero();
    readfds.set(pipefd[0]);
    let mut timeout = Timeval {
        tv_sec: 0,
        tv_usec: 0,
    };
    let result = if wrote {
        unsafe {
            select(
                pipefd[0] + 1,
                &mut readfds,
                null_mut(),
                null_mut(),
                &mut timeout,
            )
        }
    } else {
        -1
    };
    let ready = readfds.is_set(pipefd[0]);
    let ok = created && wrote && result == 1 && ready;
    state.check(
        "select-readable",
        ok,
        format_args!(
            "created={} wrote={} result={} isset={}",
            flag(created),
            flag(wrote),
            result,
            flag(ready)
        ),
    );
    for fd in pipefd {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn row_timeout(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let mut entry = Pollfd {
        fd: pipefd[0],
        events: POLLIN,
        revents: 0,
    };
    let start = monotonic_ms();
    let result = if created {
        unsafe { poll(&mut entry, 1, 50) }
    } else {
        -1
    };
    let elapsed = monotonic_ms().saturating_sub(start);
    let ok = created && result == 0 && entry.revents == 0 && elapsed >= 45 && elapsed < 2000;
    state.check(
        "timeout",
        ok,
        format_args!(
            "created={} result={} revents=0x{:x} elapsed_ms={}",
            flag(created),
            result,
            entry.revents,
            elapsed
        ),
    );
    for fd in pipefd {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn row_eintr(state: &mut State) {
    let installed = install_sigalrm();
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let mut entry = Pollfd {
        fd: pipefd[0],
        events: POLLIN,
        revents: 0,
    };
    SIGALRM_SEEN.store(0, Ordering::SeqCst);
    set_errno(0);
    unsafe {
        alarm(1);
    }
    let result = if created {
        unsafe { poll(&mut entry, 1, 3000) }
    } else {
        -1
    };
    let error = errno();
    let seen = SIGALRM_SEEN.load(Ordering::SeqCst);
    unsafe {
        alarm(0);
    }
    let ok = installed && created && result == -1 && error == EINTR && seen == 1;
    state.check(
        "eintr",
        ok,
        format_args!(
            "installed={} created={} result={} errno={} seen={}",
            flag(installed),
            flag(created),
            result,
            error,
            seen
        ),
    );
    for fd in pipefd {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn row_socket_error(state: &mut State) {
    let fd = unsafe { socket(AF_INET, SOCK_STREAM, 0) };
    let mut ok = fd >= 0;
    let mut flags = -1;
    if ok {
        flags = unsafe { fcntl(fd, F_GETFL, 0) };
        ok = flags >= 0 && unsafe { fcntl(fd, F_SETFL, flags | O_NONBLOCK) } == 0;
    }
    let peer = SockaddrIn {
        sin_family: AF_INET as u16,
        sin_port: 9u16.to_be(),
        sin_addr: u32::from_ne_bytes([127, 0, 0, 1]),
        sin_zero: [0; 8],
    };
    let mut connect_errno = 0;
    if ok {
        set_errno(0);
        // SAFETY: the sockaddr is fully initialized and 16 bytes long.
        let connected = unsafe { connect(fd, &peer, core::mem::size_of::<SockaddrIn>() as u32) };
        connect_errno = errno();
        ok = connected == 0 || connect_errno == EINPROGRESS || connect_errno == EAGAIN;
    }
    let mut attempts = 0;
    let mut revents = 0i16;
    let mut error = 0i32;
    while ok && error == 0 && attempts < 100 {
        attempts += 1;
        let mut entry = Pollfd {
            fd,
            events: POLLOUT,
            revents: 0,
        };
        let result = unsafe { poll(&mut entry, 1, 50) };
        if result < 0 {
            if errno() == EINTR {
                continue;
            }
            ok = false;
            break;
        }
        revents = entry.revents;
        let mut length = core::mem::size_of::<c_int>() as u32;
        // SAFETY: `error` is a writable c_int and the length is its size.
        let got = unsafe {
            getsockopt(
                fd,
                SOL_SOCKET,
                SO_ERROR,
                (&mut error as *mut c_int).cast::<c_void>(),
                &mut length,
            )
        };
        if got != 0 {
            ok = false;
            break;
        }
    }
    let first_error = error;
    let mut second = -1i32;
    let mut length = core::mem::size_of::<c_int>() as u32;
    let second_ok = ok
        && first_error != 0
        && unsafe {
            getsockopt(
                fd,
                SOL_SOCKET,
                SO_ERROR,
                (&mut second as *mut c_int).cast::<c_void>(),
                &mut length,
            )
        } == 0;
    ok = ok
        && attempts < 100
        && first_error == ECONNREFUSED
        && revents & POLLERR != 0
        && second_ok
        && second == 0;
    state.check(
        "socket-error",
        ok,
        format_args!(
            "fd={} flags=0x{:x} connect_errno={} attempts={} revents=0x{:x} soerror={} second={} consumed_once={}",
            fd,
            flags,
            connect_errno,
            attempts,
            revents,
            first_error,
            second,
            flag(second_ok)
        ),
    );
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
}

fn row_eventfd(state: &mut State) {
    let fd = unsafe { eventfd(0, 0) };
    let mut ok = fd >= 0;
    let mut before = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let before_result = if ok {
        unsafe { poll(&mut before, 1, 0) }
    } else {
        -1
    };
    let wrote = ok && unsafe { eventfd_write(fd, 1) } == 0;
    let mut after = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let after_result = if wrote {
        unsafe { poll(&mut after, 1, 0) }
    } else {
        -1
    };
    let mut again = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let again_result = if after_result == 1 {
        unsafe { poll(&mut again, 1, 0) }
    } else {
        -1
    };
    let mut value = 0u64;
    let read_ok = again_result == 1 && unsafe { eventfd_read(fd, &mut value) } == 0 && value == 1;
    let mut drained = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let drained_result = if read_ok {
        unsafe { poll(&mut drained, 1, 0) }
    } else {
        -1
    };
    ok = ok
        && before_result == 0
        && before.revents == 0
        && wrote
        && after_result == 1
        && after.revents & POLLIN != 0
        && again_result == 1
        && again.revents & POLLIN != 0
        && read_ok
        && drained_result == 0
        && drained.revents == 0;
    state.check(
        "eventfd",
        ok,
        format_args!(
            "fd={} before={}/0x{:x} wrote={} after={}/0x{:x} again={}/0x{:x} value={} drained={}/0x{:x}",
            fd,
            before_result,
            before.revents,
            flag(wrote),
            after_result,
            after.revents,
            again_result,
            again.revents,
            value,
            drained_result,
            drained.revents
        ),
    );
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
}

fn row_timerfd(state: &mut State) {
    let fd = unsafe { timerfd_create(CLOCK_MONOTONIC, 0) };
    let mut ok = fd >= 0;
    let mut before = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let before_result = if ok {
        unsafe { poll(&mut before, 1, 0) }
    } else {
        -1
    };
    let spec = Itimerspec {
        it_interval: Timespec {
            tv_sec: 0,
            tv_nsec: 0,
        },
        it_value: Timespec {
            tv_sec: 0,
            tv_nsec: 200_000_000,
        },
    };
    let armed = ok && unsafe { timerfd_settime(fd, 0, &spec, null_mut()) } == 0;
    let mut immediate = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let immediate_result = if armed {
        unsafe { poll(&mut immediate, 1, 0) }
    } else {
        -1
    };
    let mut expired = Pollfd {
        fd,
        events: POLLIN,
        revents: 0,
    };
    let expired_result = if armed {
        unsafe { poll(&mut expired, 1, 1000) }
    } else {
        -1
    };
    ok = ok
        && before_result == 0
        && before.revents == 0
        && armed
        && immediate_result == 0
        && immediate.revents == 0
        && expired_result == 1
        && expired.revents & POLLIN != 0;
    state.check(
        "timerfd",
        ok,
        format_args!(
            "fd={} before={}/0x{:x} armed={} immediate={}/0x{:x} expired={}/0x{:x}",
            fd,
            before_result,
            before.revents,
            flag(armed),
            immediate_result,
            immediate.revents,
            expired_result,
            expired.revents
        ),
    );
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_pipe_readable(&mut state);
    row_pipe_hup(&mut state, false);
    row_select_readable(&mut state);
    row_timeout(&mut state);
    row_eventfd(&mut state);
    row_timerfd(&mut state);
    row_socket_error(&mut state);
    // Last: a signal-wait defect must not hide the other rows.
    row_eintr(&mut state);
    state.finish()
}

fn run_mutation(name: &str) -> i32 {
    let mut state = State {
        topic: TOPIC,
        total: 0,
        passed: 0,
        failed: 0,
    };
    conformance::mutation_begin(TOPIC, name);
    match name {
        "hup-not-reported" => row_pipe_hup(&mut state, true),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    let mut failures = run_normal();
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv, which is NUL-terminated.
        let argument = unsafe { *argv.add(index as usize) };
        let mut cursor = argument as *const u8;
        let mut all = true;
        for expected in b"--all" {
            // SAFETY: the caller supplies a NUL-terminated argv entry.
            if unsafe { *cursor } != *expected {
                all = false;
                break;
            }
            // SAFETY: advance within the same NUL-terminated string.
            cursor = unsafe { cursor.add(1) };
        }
        // SAFETY: the expected text is a prefix; the next byte must end the string.
        if all && unsafe { *cursor } == 0 {
            failures |= run_mutation("hup-not-reported");
        }
        index += 1;
    }
    if failures == 0 { 0 } else { 1 }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    conformance::emit(format_args!("CONFORM {TOPIC} panic\n"));
    loop {
        core::hint::spin_loop();
    }
}
