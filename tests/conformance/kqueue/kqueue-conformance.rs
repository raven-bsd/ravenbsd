// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S743 kqueue conformance probe (test-only, Rust).
//!
//! Runs the S743 declared kqueue subset against the live image: the 64-byte record, the
//! kqueue/kqueue1 entry points, descriptor readiness on pipes and sockets, EV_ADD modify,
//! EV_DISABLE/EV_ENABLE, EV_ONESHOT, EV_RECEIPT, EVFILT_TIMER, EVFILT_USER, the honest
//! EOPNOTSUPP for filters and fflags outside the subset, fd close/reuse lifetime, and EINTR.
//!
//! The donor contract is FreeBSD `133344405f532bd54ff3629a77d4c4e062f01fdb`
//! `sys/kern/kern_event.c` (`kqueue_register`, `kqueue_kevent`, `filt_timer*`, `filt_user*`,
//! `filt_piperead`/`filt_soread`) and `sys/sys/event.h`. No donor expression is copied; the
//! rows assert the observable records and errno values.
//!
//! Controlled mutations encode what a broken product would believe: a registration survives
//! close/reuse, an unsupported filter succeeds, or EV_ONESHOT is not deleted after delivery.

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_uint, c_void};
use core::ptr::null_mut;
use core::sync::atomic::{AtomicI32, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "kqueue";

const EVFILT_READ: i16 = -1;
const EVFILT_WRITE: i16 = -2;
const EVFILT_SIGNAL: i16 = -6;
const EVFILT_TIMER: i16 = -7;
const EVFILT_USER: i16 = -11;

const EV_ADD: u16 = 0x0001;
const EV_ONESHOT: u16 = 0x0010;
const EV_RECEIPT: u16 = 0x0040;
const EV_ERROR: u16 = 0x4000;
const EV_DISABLE: u16 = 0x0008;
const EV_ENABLE: u16 = 0x0004;

const NOTE_TRIGGER: u32 = 0x0100_0000;
const NOTE_LOWAT: u32 = 0x0001;

const O_CLOEXEC: c_int = 0o2000000;
const O_NONBLOCK: c_int = 0o4000;

const AF_UNIX: c_int = 1;
const SOCK_STREAM: c_int = 1;

const SIGALRM: c_int = 14;

const EINVAL: c_int = 22;
const EOPNOTSUPP: c_int = 45;
const EINTR: c_int = 4;

static SIGALRM_SEEN: AtomicI32 = AtomicI32::new(0);

/// The installed C `struct kevent` (64 bytes on LP64).
#[repr(C)]
#[derive(Clone, Copy)]
struct Kevent {
    ident: usize,
    filter: i16,
    flags: u16,
    fflags: u32,
    data: i64,
    udata: usize,
    ext: [u64; 4],
}

const _: () = assert!(core::mem::size_of::<Kevent>() == 64);
const _: () = assert!(core::mem::offset_of!(Kevent, ident) == 0);
const _: () = assert!(core::mem::offset_of!(Kevent, filter) == 8);
const _: () = assert!(core::mem::offset_of!(Kevent, flags) == 10);
const _: () = assert!(core::mem::offset_of!(Kevent, fflags) == 12);
const _: () = assert!(core::mem::offset_of!(Kevent, data) == 16);
const _: () = assert!(core::mem::offset_of!(Kevent, udata) == 24);
const _: () = assert!(core::mem::offset_of!(Kevent, ext) == 32);

impl Kevent {
    const fn new(ident: usize, filter: i16, flags: u16, fflags: u32, data: i64) -> Self {
        Self {
            ident,
            filter,
            flags,
            fflags,
            data,
            udata: 0,
            ext: [0; 4],
        }
    }

    const fn with_udata(mut self, udata: usize) -> Self {
        self.udata = udata;
        self
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
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

const ZERO_TIMEOUT: Timespec = Timespec {
    tv_sec: 0,
    tv_nsec: 0,
};

const ONE_SECOND: Timespec = Timespec {
    tv_sec: 1,
    tv_nsec: 0,
};

const THREE_SECONDS: Timespec = Timespec {
    tv_sec: 3,
    tv_nsec: 0,
};

unsafe extern "C" {
    fn kqueue() -> c_int;
    fn kqueue1(flags: c_int) -> c_int;
    fn kevent(
        kq: c_int,
        changes: *const Kevent,
        nchanges: c_int,
        events: *mut Kevent,
        nevents: c_int,
        timeout: *const Timespec,
    ) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn socketpair(domain: c_int, kind: c_int, protocol: c_int, fds: *mut c_int) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
    fn alarm(seconds: c_uint) -> c_uint;
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

fn close_all(fds: [c_int; 2]) {
    for fd in fds {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn poll_events(kq: c_int, events: &mut [Kevent], timeout: &Timespec) -> c_int {
    // SAFETY: the event buffer is writable for the length and the timeout is readable.
    unsafe {
        kevent(
            kq,
            core::ptr::null(),
            0,
            events.as_mut_ptr(),
            c_int::try_from(events.len()).unwrap_or(0),
            timeout,
        )
    }
}

fn apply(kq: c_int, changes: &[Kevent], events: &mut [Kevent], timeout: &Timespec) -> c_int {
    // SAFETY: both arrays are live for the lengths passed and the timeout is readable.
    unsafe {
        kevent(
            kq,
            changes.as_ptr(),
            c_int::try_from(changes.len()).unwrap_or(0),
            events.as_mut_ptr(),
            c_int::try_from(events.len()).unwrap_or(0),
            timeout,
        )
    }
}

fn row_layout(state: &mut State) {
    let kq = unsafe { kqueue() };
    let cloexec = unsafe { kqueue1(O_CLOEXEC | O_NONBLOCK) };
    set_errno(0);
    let rejected = unsafe { kqueue1(0x10) };
    let bad_errno = errno();
    let ok = core::mem::size_of::<Kevent>() == 64
        && kq >= 0
        && cloexec >= 0
        && rejected == -1
        && bad_errno == EINVAL;
    state.check(
        "layout",
        ok,
        format_args!(
            "size={} kq={} cloexec={} rejected={} errno={}",
            core::mem::size_of::<Kevent>(),
            kq,
            cloexec,
            rejected,
            bad_errno
        ),
    );
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
    if cloexec >= 0 {
        unsafe {
            close(cloexec);
        }
    }
}

fn row_read_ready(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let add = Kevent::new(pipefd[0] as usize, EVFILT_READ, EV_ADD, 0, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let applied = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let before = if applied == 0 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(pipefd[1], b"x".as_ptr().cast::<c_void>(), 1) } == 1;
    let after = if wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let record = events[0];
    let ok = kq >= 0
        && created
        && applied == 0
        && before == 0
        && wrote
        && after == 1
        && record.ident == pipefd[0] as usize
        && record.filter == EVFILT_READ
        && record.flags & EV_ERROR == 0
        && record.data > 0;
    state.check(
        "read-ready",
        ok,
        format_args!(
            "kq={} created={} applied={} before={} wrote={} after={} ident={} filter={} flags=0x{:x} data={}",
            kq,
            flag(created),
            applied,
            before,
            flag(wrote),
            after,
            record.ident,
            record.filter,
            record.flags,
            record.data
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_socket_ready(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut fds = [-1i32; 2];
    let created = unsafe { socketpair(AF_UNIX, SOCK_STREAM, 0, fds.as_mut_ptr()) } == 0;
    let add = Kevent::new(fds[0] as usize, EVFILT_READ, EV_ADD, 0, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let applied = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(fds[1], b"s".as_ptr().cast::<c_void>(), 1) } == 1;
    let after = if wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let record = events[0];
    let ok = kq >= 0
        && created
        && applied == 0
        && wrote
        && after == 1
        && record.ident == fds[0] as usize
        && record.filter == EVFILT_READ
        && record.flags & EV_ERROR == 0;
    state.check(
        "socket-ready",
        ok,
        format_args!(
            "kq={} created={} applied={} wrote={} after={} ident={} filter={} flags=0x{:x}",
            kq,
            flag(created),
            applied,
            flag(wrote),
            after,
            record.ident,
            record.filter,
            record.flags
        ),
    );
    close_all(fds);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_read_write_same_fd(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut fds = [-1i32; 2];
    let created = unsafe { socketpair(AF_UNIX, SOCK_STREAM, 0, fds.as_mut_ptr()) } == 0;
    let changes = [
        Kevent::new(fds[0] as usize, EVFILT_READ, EV_ADD, 0, 0),
        Kevent::new(fds[0] as usize, EVFILT_WRITE, EV_ADD, 0, 0),
    ];
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 2];
    let applied = if kq >= 0 && created {
        apply(kq, &changes, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(fds[1], b"b".as_ptr().cast::<c_void>(), 1) } == 1;
    let after = if wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let mut saw_read = false;
    let mut saw_write = false;
    let mut saw_error = false;
    for event in events.iter().take(after.max(0) as usize) {
        saw_error |= event.flags & EV_ERROR != 0;
        saw_read |= event.filter == EVFILT_READ && event.ident == fds[0] as usize;
        saw_write |= event.filter == EVFILT_WRITE && event.ident == fds[0] as usize;
    }
    let ok = kq >= 0
        && created
        && applied >= 0
        && wrote
        && after >= 1
        && saw_read
        && saw_write
        && !saw_error;
    state.check(
        "read-write-same-fd",
        ok,
        format_args!(
            "kq={} created={} applied={} wrote={} after={} read={} write={} error={}",
            kq,
            flag(created),
            applied,
            flag(wrote),
            after,
            flag(saw_read),
            flag(saw_write),
            flag(saw_error)
        ),
    );
    close_all(fds);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_modify_udata(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let first = Kevent::new(pipefd[0] as usize, EVFILT_READ, EV_ADD, 0, 0).with_udata(11);
    let second = Kevent::new(pipefd[0] as usize, EVFILT_READ, EV_ADD, 0, 0).with_udata(22);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let added = if kq >= 0 && created {
        apply(kq, &[first], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let modified = if added == 0 {
        apply(kq, &[second], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(pipefd[1], b"m".as_ptr().cast::<c_void>(), 1) } == 1;
    let after = if wrote && modified == 0 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let ok = kq >= 0
        && created
        && added == 0
        && modified == 0
        && wrote
        && after == 1
        && events[0].udata == 22
        && events[0].flags & EV_ERROR == 0;
    state.check(
        "modify-udata",
        ok,
        format_args!(
            "kq={} created={} added={} modified={} wrote={} after={} udata={} flags=0x{:x}",
            kq,
            flag(created),
            added,
            modified,
            flag(wrote),
            after,
            events[0].udata,
            events[0].flags
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_disable_enable(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let add = Kevent::new(
        pipefd[0] as usize,
        EVFILT_READ,
        EV_ADD | EV_DISABLE,
        0,
        0,
    );
    let added = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(pipefd[1], b"d".as_ptr().cast::<c_void>(), 1) } == 1;
    let disabled = if wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let enable = Kevent::new(pipefd[0] as usize, EVFILT_READ, EV_ENABLE, 0, 0);
    let enabled = if added == 0 {
        apply(kq, &[enable], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    // The enabling call scans with a zero timeout, so it may already report the ready record.
    let after = if enabled == 1 {
        1
    } else if enabled == 0 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let ok = kq >= 0
        && created
        && added == 0
        && wrote
        && disabled == 0
        && enabled >= 0
        && after == 1
        && events[0].filter == EVFILT_READ;
    state.check(
        "disable-enable",
        ok,
        format_args!(
            "kq={} created={} added={} wrote={} disabled={} enabled={} after={} filter={}",
            kq,
            flag(created),
            added,
            flag(wrote),
            disabled,
            enabled,
            after,
            events[0].filter
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_oneshot(state: &mut State, mutation: bool) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let add = Kevent::new(
        pipefd[0] as usize,
        EVFILT_READ,
        EV_ADD | EV_ONESHOT,
        0,
        0,
    );
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let added = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let wrote = created && unsafe { write(pipefd[1], b"o".as_ptr().cast::<c_void>(), 1) } == 1;
    let first = if wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let second = if first == 1 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    // The correct product deletes the one-shot registration after the first record; the
    // mutation expects the registration to survive and report the same level again.
    let expected_second = if mutation { 1 } else { 0 };
    let ok =
        kq >= 0 && created && added == 0 && wrote && first == 1 && second == expected_second;
    state.check(
        "oneshot",
        ok,
        format_args!(
            "kq={} created={} added={} wrote={} first={} second={} expected_second={}",
            kq,
            flag(created),
            added,
            flag(wrote),
            first,
            second,
            expected_second
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_timer(state: &mut State) {
    let kq = unsafe { kqueue() };
    let add = Kevent::new(1, EVFILT_TIMER, EV_ADD, 0, 10);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let added = if kq >= 0 {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let before = if added == 0 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let fired = if added == 0 {
        poll_events(kq, &mut events, &ONE_SECOND)
    } else {
        -1
    };
    let ok = kq >= 0
        && added == 0
        && before == 0
        && fired == 1
        && events[0].filter == EVFILT_TIMER
        && events[0].data > 0;
    state.check(
        "timer",
        ok,
        format_args!(
            "kq={} added={} before={} fired={} filter={} data={}",
            kq, added, before, fired, events[0].filter, events[0].data
        ),
    );
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_user(state: &mut State) {
    let kq = unsafe { kqueue() };
    let add = Kevent::new(1, EVFILT_USER, EV_ADD, NOTE_TRIGGER, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let added = if kq >= 0 {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    // A NOTE_TRIGGER registration may already be reported by the scan of the same call.
    let fired = if added == 1 {
        1
    } else if added == 0 {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let ok = kq >= 0
        && added >= 0
        && fired == 1
        && events[0].filter == EVFILT_USER
        && events[0].fflags & NOTE_TRIGGER != 0;
    state.check(
        "user",
        ok,
        format_args!(
            "kq={} added={} fired={} filter={} fflags=0x{:x}",
            kq, added, fired, events[0].filter, events[0].fflags
        ),
    );
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_unsupported_filter(state: &mut State, mutation: bool) {
    let kq = unsafe { kqueue() };
    let add = Kevent::new(2, EVFILT_SIGNAL, EV_ADD, 0, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let result = if kq >= 0 {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let reported_error = events[0].flags & EV_ERROR != 0;
    let reported_support = events[0].data != i64::from(EOPNOTSUPP);
    // The correct product reports a per-change EV_ERROR/EOPNOTSUPP; the mutation believes an
    // unsupported filter was registered successfully (no error record).
    let expect_error = !mutation;
    let ok = kq >= 0
        && result == 1
        && reported_error == expect_error
        && (mutation || !reported_support);
    state.check(
        "unsupported-filter",
        ok,
        format_args!(
            "kq={} result={} flags=0x{:x} data={} expect_error={} mutation={}",
            kq,
            result,
            events[0].flags,
            events[0].data,
            flag(expect_error),
            flag(mutation)
        ),
    );
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_unsupported_fflags(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let add = Kevent::new(pipefd[0] as usize, EVFILT_READ, EV_ADD, NOTE_LOWAT, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let result = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let ok = kq >= 0
        && created
        && result == 1
        && events[0].flags & EV_ERROR != 0
        && events[0].data == i64::from(EOPNOTSUPP);
    state.check(
        "unsupported-fflags",
        ok,
        format_args!(
            "kq={} created={} result={} flags=0x{:x} data={}",
            kq,
            flag(created),
            result,
            events[0].flags,
            events[0].data
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_receipt(state: &mut State) {
    let kq = unsafe { kqueue() };
    let mut pipefd = [-1i32; 2];
    let created = unsafe { pipe(pipefd.as_mut_ptr()) } == 0;
    let add = Kevent::new(
        pipefd[0] as usize,
        EVFILT_READ,
        EV_ADD | EV_RECEIPT,
        0,
        0,
    );
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let result = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let ok = kq >= 0
        && created
        && result == 1
        && events[0].flags & EV_ERROR != 0
        && events[0].data == 0;
    state.check(
        "receipt",
        ok,
        format_args!(
            "kq={} created={} result={} flags=0x{:x} data={}",
            kq,
            flag(created),
            result,
            events[0].flags,
            events[0].data
        ),
    );
    close_all(pipefd);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn row_close_reuse(state: &mut State, mutation: bool) {
    let kq = unsafe { kqueue() };
    let mut first = [-1i32; 2];
    let created = unsafe { pipe(first.as_mut_ptr()) } == 0;
    let old_read = first[0];
    let add = Kevent::new(old_read as usize, EVFILT_READ, EV_ADD, 0, 0);
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    let added = if kq >= 0 && created {
        apply(kq, &[add], &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    let closed = created && unsafe { close(old_read) } == 0;
    // Reuse the number with a different pipe object.
    let mut reused = false;
    let mut second = [-1i32; 2];
    if closed {
        for _ in 0..32 {
            if unsafe { pipe(second.as_mut_ptr()) } != 0 {
                break;
            }
            if second[0] == old_read {
                reused = true;
                break;
            }
            unsafe {
                close(second[0]);
                close(second[1]);
            }
            second = [-1i32; 2];
        }
    }
    let wrote = created
        && unsafe { write(first[1], b"r".as_ptr().cast::<c_void>(), 1) } == 1;
    let after = if added == 0 && closed && reused && wrote {
        poll_events(kq, &mut events, &ZERO_TIMEOUT)
    } else {
        -1
    };
    // The registration was tied to the closed object: the old writer cannot produce an event
    // for the recycled number. The mutation expects the numeric registration to survive.
    let expected = if mutation { 1 } else { 0 };
    let ok = kq >= 0
        && created
        && added == 0
        && closed
        && reused
        && wrote
        && after == expected;
    state.check(
        "close-reuse",
        ok,
        format_args!(
            "kq={} created={} added={} closed={} reused={} old_read={} new_read={} wrote={} after={} expected={}",
            kq,
            flag(created),
            added,
            flag(closed),
            flag(reused),
            old_read,
            second[0],
            flag(wrote),
            after,
            expected
        ),
    );
    close_all(second);
    close_all(first);
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
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

fn row_eintr(state: &mut State) {
    let installed = install_sigalrm();
    let kq = unsafe { kqueue() };
    let mut events = [Kevent::new(0, 0, 0, 0, 0); 1];
    SIGALRM_SEEN.store(0, Ordering::SeqCst);
    set_errno(0);
    unsafe {
        alarm(1);
    }
    let result = if kq >= 0 {
        poll_events(kq, &mut events, &THREE_SECONDS)
    } else {
        -1
    };
    let error = errno();
    unsafe {
        alarm(0);
    }
    let ok = installed && kq >= 0 && result == -1 && error == EINTR;
    state.check(
        "eintr",
        ok,
        format_args!(
            "installed={} kq={} result={} errno={} seen={}",
            flag(installed),
            kq,
            result,
            error,
            SIGALRM_SEEN.load(Ordering::SeqCst)
        ),
    );
    if kq >= 0 {
        unsafe {
            close(kq);
        }
    }
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_layout(&mut state);
    row_read_ready(&mut state);
    row_socket_ready(&mut state);
    row_read_write_same_fd(&mut state);
    row_modify_udata(&mut state);
    row_disable_enable(&mut state);
    row_oneshot(&mut state, false);
    row_timer(&mut state);
    row_user(&mut state);
    row_unsupported_filter(&mut state, false);
    row_unsupported_fflags(&mut state);
    row_receipt(&mut state);
    row_close_reuse(&mut state, false);
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
        "retain-after-close" => row_close_reuse(&mut state, true),
        "unsupported-filter-success" => row_unsupported_filter(&mut state, true),
        "lose-oneshot" => row_oneshot(&mut state, true),
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
            failures |= run_mutation("retain-after-close");
            failures |= run_mutation("unsupported-filter-success");
            failures |= run_mutation("lose-oneshot");
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
