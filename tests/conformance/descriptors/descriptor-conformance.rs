// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S172 descriptor conformance probe (test-only, Rust).
//!
//! Runs the POSIX descriptor contract on the release image: one open file description shared by
//! dup/dup2/fcntl(F_DUPFD), O_APPEND positioning, open/creat/O_TRUNC/O_EXCL flags, per-descriptor
//! (FD_CLOEXEC) versus per-open-file (O_APPEND/O_NONBLOCK) flags, O_CLOEXEC/F_SETFD across exec,
//! EOF, short reads, EINTR with a handler and only after a partial transfer, EAGAIN with
//! O_NONBLOCK, pipe EPIPE/SIGPIPE, and fstat identity for duplicated descriptors.
//!
//! The donor contract is FreeBSD sys/kern/kern_descrip.c (kern_dup, fdcloseexec, kern_fcntl),
//! sys/kern/sys_generic.c (dofileread/dofilewrite partial-transfer EINTR suppression),
//! sys/kern/vfs_vnops.c (IO_APPEND from f_flag O_APPEND) and sys/kern/sys_pipe.c (EPIPE and
//! SIGPIPE).  No donor expression is copied; the rows assert the observable contract.
//!
//! Two controlled mutations encode what a broken implementation would believe and therefore turn
//! their rows red against the correct product:
//!   dup-copies-offset   -- a dup is expected to own an independent file offset
//!   cloexec-survives    -- a FD_CLOEXEC descriptor is expected to survive exec

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_void};
use core::fmt::{self, Write};
use core::ptr::{null, null_mut};
use core::sync::atomic::{AtomicI32, Ordering};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "descriptors";
const SELF: &[u8] = b"/bin/conformance-probe\0";

const O_RDONLY: c_int = 0;
const O_WRONLY: c_int = 1;
const O_RDWR: c_int = 2;
const O_CREAT: c_int = 0o100;
const O_EXCL: c_int = 0o200;
const O_TRUNC: c_int = 0o1000;
const O_APPEND: c_int = 0o2000;
const O_NONBLOCK: c_int = 0o4000;
const O_CLOEXEC: c_int = 0o2000000;

const F_DUPFD: c_int = 0;
const F_GETFD: c_int = 1;
const F_SETFD: c_int = 2;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
const F_DUPFD_CLOEXEC: c_int = 1030;
const FD_CLOEXEC: c_int = 1;

const SEEK_SET: c_int = 0;
const SEEK_CUR: c_int = 1;

const EINTR: c_int = 4;
const EBADF: c_int = 9;
const EAGAIN: c_int = 11;
const EEXIST: c_int = 17;
const EINVAL: c_int = 22;
const EPIPE: c_int = 32;

const SIGPIPE: c_int = 13;
const SIGALRM: c_int = 14;
const SIG_IGN: usize = 1;

const S_IFMT: u32 = 0o170000;
const S_IFREG: u32 = 0o100000;

static SIGPIPE_SEEN: AtomicI32 = AtomicI32::new(0);
static SIGALRM_SEEN: AtomicI32 = AtomicI32::new(0);

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Stat {
    st_dev: u64,
    st_ino: u64,
    st_nlink: u64,
    st_mode: u16,
    st_bsdflags: i16,
    st_uid: u32,
    st_gid: u32,
    st_padding1: i32,
    st_rdev: u64,
    st_atim: Timespec,
    st_mtim: Timespec,
    st_ctim: Timespec,
    st_birthtim: Timespec,
    st_size: i64,
    st_blocks: i64,
    st_blksize: i32,
    st_flags: u32,
    st_gen: u64,
    st_filerev: u64,
    st_spare: [u64; 9],
}

impl Stat {
    const fn zero() -> Self {
        Self {
            st_dev: 0,
            st_ino: 0,
            st_nlink: 0,
            st_mode: 0,
            st_bsdflags: 0,
            st_uid: 0,
            st_gid: 0,
            st_padding1: 0,
            st_rdev: 0,
            st_atim: Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            st_mtim: Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            st_ctim: Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            st_birthtim: Timespec {
                tv_sec: 0,
                tv_nsec: 0,
            },
            st_size: 0,
            st_blocks: 0,
            st_blksize: 0,
            st_flags: 0,
            st_gen: 0,
            st_filerev: 0,
            st_spare: [0; 9],
        }
    }
}

const _: () = assert!(core::mem::size_of::<Stat>() == 224);

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

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn creat(path: *const c_char, mode: u32) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    #[link_name = "write"]
    fn c_write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn lseek(fd: c_int, offset: i64, whence: c_int) -> i64;
    fn dup(fd: c_int) -> c_int;
    fn dup2(old: c_int, new: c_int) -> c_int;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn fstat(fd: c_int, buffer: *mut Stat) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn pipe2(fds: *mut c_int, flags: c_int) -> c_int;
    fn fork() -> c_int;
    fn execve(path: *const c_char, argv: *const *const c_char, envp: *const *const c_char)
    -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn getpid() -> c_int;
    fn unlink(path: *const c_char) -> c_int;
    fn alarm(seconds: u32) -> u32;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
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

struct Text<const N: usize> {
    data: [u8; N],
    len: usize,
}

impl<const N: usize> Text<N> {
    const fn new() -> Self {
        Self {
            data: [0; N],
            len: 0,
        }
    }

    fn as_ptr(&self) -> *const c_char {
        self.data.as_ptr() as *const c_char
    }
}

impl<const N: usize> Write for Text<N> {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        let bytes = text.as_bytes();
        if self.len + bytes.len() > self.data.len() {
            return Err(fmt::Error);
        }
        self.data[self.len..self.len + bytes.len()].copy_from_slice(bytes);
        self.len += bytes.len();
        Ok(())
    }
}

fn path_for(suffix: &str) -> Text<128> {
    let mut text = Text::new();
    // SAFETY: getpid has no preconditions.
    let _ = write!(&mut text, "/tmp/descriptor-{}-{}\0", unsafe { getpid() }, suffix);
    text
}

fn number(fd: c_int) -> Text<16> {
    let mut text = Text::new();
    let _ = write!(&mut text, "{fd}\0");
    text
}

fn printable(byte: u8) -> char {
    if byte.is_ascii_graphic() {
        byte as char
    } else {
        '.'
    }
}

fn flag(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

fn write_all_file(fd: c_int, mut data: &[u8]) -> isize {
    let mut total = 0isize;
    while !data.is_empty() {
        // SAFETY: `data` is readable for the length passed.
        let count = unsafe { c_write(fd, data.as_ptr().cast::<c_void>(), data.len()) };
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

fn read_file(path: &Text<128>, buffer: &mut [u8]) -> isize {
    // SAFETY: the path is NUL-terminated and the buffer is writable.
    let fd = unsafe { open(path.as_ptr(), O_RDONLY) };
    if fd < 0 {
        return -1;
    }
    // SAFETY: the buffer is writable for its length.
    let count = unsafe { read(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) };
    // SAFETY: the descriptor is ours.
    unsafe {
        close(fd);
    }
    count
}

fn fd_is_open(fd: c_int) -> bool {
    // SAFETY: fcntl only inspects the descriptor.
    unsafe { fcntl(fd, F_GETFD, 0) >= 0 }
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

fn c_str_i32(pointer: *const c_char) -> c_int {
    let mut cursor = pointer as *const u8;
    let mut sign = 1;
    // SAFETY: the caller supplies a NUL-terminated argv entry.
    if unsafe { *cursor } == b'-' {
        sign = -1;
        // SAFETY: advance within the same string.
        cursor = unsafe { cursor.add(1) };
    }
    let mut value = 0;
    loop {
        // SAFETY: advance within the same NUL-terminated string.
        let byte = unsafe { *cursor };
        if byte == 0 {
            break;
        }
        value = value * 10 + i32::from(byte - b'0');
        // SAFETY: advance within the same string.
        cursor = unsafe { cursor.add(1) };
    }
    sign * value
}

fn install_handler(signal: c_int, handler: extern "C" fn(c_int)) -> bool {
    let mut action = Sigaction {
        sa_handler: handler as usize,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: the action and mask are fully initialized.
    unsafe {
        sigemptyset(&mut action.sa_mask);
        sigaction(signal, &action, null_mut()) == 0
    }
}

extern "C" fn note_sigpipe(_signal: c_int) {
    SIGPIPE_SEEN.fetch_add(1, Ordering::SeqCst);
}

extern "C" fn note_sigalrm(_signal: c_int) {
    SIGALRM_SEEN.fetch_add(1, Ordering::SeqCst);
}

fn child_cloexec(argc: c_int, argv: *const *const c_char) -> c_int {
    if argc < 7 {
        conformance::emit(format_args!(
            "CONFORM {TOPIC} child=cloexec bad-argc={argc}\n"
        ));
        return 90;
    }
    // SAFETY: argv has at least seven entries.
    let argument = |index: c_int| unsafe { *argv.add(index as usize) };
    let keep = c_str_i32(argument(2));
    let cloexec = c_str_i32(argument(3));
    let setfd = c_str_i32(argument(4));
    let plain = c_str_i32(argument(5));
    let dupfd = c_str_i32(argument(6));
    let mutation = argc > 7 && c_str_eq(argument(7), b"--mutation-cloexec-survives");

    let keep_open = fd_is_open(keep);
    let cloexec_open = fd_is_open(cloexec);
    let setfd_open = fd_is_open(setfd);
    let plain_open = fd_is_open(plain);
    let dupfd_open = fd_is_open(dupfd);

    let mut keep_byte = 0u8;
    let mut plain_byte = 0u8;
    // SAFETY: the descriptors are valid in this child and the buffers are one byte.
    let keep_readable = keep_open
        && unsafe { read(keep, (&mut keep_byte as *mut u8).cast::<c_void>(), 1) } == 1
        && keep_byte == b'K';
    // SAFETY: same as above for the plain descriptor.
    let plain_readable = plain_open
        && unsafe { read(plain, (&mut plain_byte as *mut u8).cast::<c_void>(), 1) } == 1
        && plain_byte == b'P';

    // Correct product: the plain descriptors survive, the CLOEXEC ones do not.  The mutation
    // expects a CLOEXEC descriptor to survive, so the correct product fails it.
    let expect_cloexec_open = mutation;
    let ok = keep_open
        && keep_readable
        && plain_open
        && plain_readable
        && cloexec_open == expect_cloexec_open
        && setfd_open == expect_cloexec_open
        && dupfd_open == expect_cloexec_open;

    conformance::emit(format_args!(
        concat!(
            "CONFORM {} child=cloexec mutation={} keep={}/{} cloexec={} setfd={} ",
            "plain={}/{} dupfd={} ok={}\n"
        ),
        TOPIC,
        flag(mutation),
        flag(keep_open),
        flag(keep_readable),
        flag(cloexec_open),
        flag(setfd_open),
        flag(plain_open),
        flag(plain_readable),
        flag(dupfd_open),
        flag(ok)
    ));
    if ok { 0 } else { 12 }
}

fn row_dup_offset(state: &mut State, mutation: bool) {
    let path = path_for("dup");
    // SAFETY: the path is NUL-terminated.
    unsafe {
        unlink(path.as_ptr());
    }
    // SAFETY: the path is NUL-terminated and the mode is supplied for O_CREAT.
    let fd = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    if fd < 0 {
        state.check(
            "dup-offset",
            false,
            format_args!("open_errno={}", errno()),
        );
        return;
    }
    let mut ok = write_all_file(fd, b"abcdef") == 6;
    let seek = unsafe { lseek(fd, 1, SEEK_SET) };
    let dupfd = if ok && seek == 1 { unsafe { dup(fd) } } else { -1 };
    let mut first = [0u8; 2];
    let first_count = if dupfd >= 0 {
        unsafe { read(dupfd, first.as_mut_ptr().cast::<c_void>(), 2) }
    } else {
        -1
    };
    let after_dup = if dupfd >= 0 {
        unsafe { lseek(fd, 0, SEEK_CUR) }
    } else {
        -1
    };
    let mut second = [0u8; 2];
    let second_count = unsafe { read(fd, second.as_mut_ptr().cast::<c_void>(), 2) };
    let (mut a, mut b) = (Stat::zero(), Stat::zero());
    let same_object = dupfd >= 0
        && unsafe { fstat(fd, &mut a) } == 0
        && unsafe { fstat(dupfd, &mut b) } == 0
        && a.st_dev == b.st_dev
        && a.st_ino == b.st_ino
        && a.st_size == b.st_size
        && u32::from(a.st_mode) & S_IFMT == S_IFREG
        && (unsafe { fcntl(fd, F_GETFL, 0) } & !O_APPEND)
            == (unsafe { fcntl(dupfd, F_GETFL, 0) } & !O_APPEND);

    // A shared file offset means the original fd continues where the duplicate stopped.
    let expect_offset = if mutation { 1 } else { 3 };
    ok = ok
        && first_count == 2
        && first == *b"bc"
        && second_count == 2
        && second == *b"de"
        && after_dup == expect_offset
        && same_object;

    state.check(
        "dup-offset",
        ok,
        format_args!(
            "fd={} dup={} r1={}/{}{} r2={}/{}{} offset={} expect_offset={} ino_match={} mutation={}",
            fd,
            dupfd,
            first_count,
            printable(first[0]),
            printable(first[1]),
            second_count,
            printable(second[0]),
            printable(second[1]),
            after_dup,
            expect_offset,
            flag(same_object),
            flag(mutation)
        ),
    );
    if dupfd >= 0 {
        unsafe {
            close(dupfd);
        }
    }
    unsafe {
        close(fd);
        unlink(path.as_ptr());
    }
}

fn row_dup2_dupfd(state: &mut State) {
    let path = path_for("dup2");
    unsafe {
        unlink(path.as_ptr());
    }
    let fd = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    let target = unsafe { open(b"/dev/null\0".as_ptr() as *const c_char, O_RDONLY) };
    let mut ok = fd >= 0
        && target >= 0
        && write_all_file(fd, b"abcdef") == 6
        && unsafe { lseek(fd, 2, SEEK_SET) } == 2
        && unsafe { dup2(fd, target) } == target;
    let mut text = [0u8; 3];
    let from_target = if ok {
        unsafe { read(target, text.as_mut_ptr().cast::<c_void>(), 2) }
    } else {
        -1
    };
    let after_target = if ok { unsafe { lseek(fd, 0, SEEK_CUR) } } else { -1 };
    let same = ok && unsafe { dup2(fd, fd) } == fd;
    let min_fd = if ok { unsafe { fcntl(fd, F_DUPFD, 100 as c_int) } } else { -1 };
    let mut byte = [0u8; 1];
    let from_min = if min_fd >= 100 {
        unsafe { read(min_fd, byte.as_mut_ptr().cast::<c_void>(), 1) }
    } else {
        -1
    };
    let after_min = if ok { unsafe { lseek(fd, 0, SEEK_CUR) } } else { -1 };
    let invalid_min = if ok { unsafe { fcntl(fd, F_DUPFD, 4096 as c_int) } } else { -2 };
    let invalid_errno = errno();
    let bad_old = if ok { unsafe { dup2(-1, target) } } else { -2 };
    let bad_old_errno = errno();
    let target_alive = target >= 0 && fd_is_open(target);

    ok = ok
        && from_target == 2
        && &text[..2] == b"cd"
        && after_target == 4
        && same
        && min_fd >= 100
        && from_min == 1
        && byte[0] == b'e'
        && after_min == 5
        && invalid_min == -1
        && invalid_errno == EINVAL
        && bad_old == -1
        && bad_old_errno == EBADF
        && target_alive;

    state.check(
        "dup2-dupfd",
        ok,
        format_args!(
            "dup2={}/{}{} offset={} same={} min={} min_read={}/{} min_offset={} invalid_min={} errno={} dup2_bad={} errno={} target_alive={}",
            from_target,
            printable(text[0]),
            printable(text[1]),
            after_target,
            flag(same),
            min_fd,
            from_min,
            printable(byte[0]),
            after_min,
            invalid_min,
            invalid_errno,
            bad_old,
            bad_old_errno,
            flag(target_alive)
        ),
    );
    if min_fd >= 100 {
        unsafe {
            close(min_fd);
        }
    }
    if target >= 0 {
        unsafe {
            close(target);
        }
    }
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_append(state: &mut State) {
    let path = path_for("append");
    unsafe {
        unlink(path.as_ptr());
    }
    let fd = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    let mut ok = fd >= 0
        && write_all_file(fd, b"12345") == 5
        && unsafe { lseek(fd, 0, SEEK_SET) } == 0;
    let append = if ok {
        unsafe { open(path.as_ptr(), O_WRONLY | O_APPEND) }
    } else {
        -1
    };
    let first = if append >= 0 {
        write_all_file(append, b"XY")
    } else {
        -1
    };
    let after_first = if append >= 0 {
        unsafe { lseek(append, 0, SEEK_CUR) }
    } else {
        -1
    };
    let rewound = if append >= 0 {
        unsafe { lseek(append, 0, SEEK_SET) }
    } else {
        -1
    };
    let second = if append >= 0 && rewound == 0 {
        write_all_file(append, b"Z")
    } else {
        -1
    };
    let mut content = [0u8; 16];
    let length = read_file(&path, &mut content);

    ok = ok
        && first == 2
        && after_first == 7
        && rewound == 0
        && second == 1
        && length == 8
        && &content[..8] == b"12345XYZ";

    let shown = if length >= 0 {
        core::str::from_utf8(&content[..length as usize]).unwrap_or("?")
    } else {
        "?"
    };
    state.check(
        "append",
        ok,
        format_args!(
            "write1={} offset1={} seek={} write2={} length={} content={}",
            first, after_first, rewound, second, length, shown
        ),
    );
    if append >= 0 {
        unsafe {
            close(append);
        }
    }
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_open_flags(state: &mut State) {
    let path = path_for("flags");
    unsafe {
        unlink(path.as_ptr());
    }
    let created = unsafe {
        open(
            path.as_ptr(),
            O_WRONLY | O_CREAT | O_EXCL,
            0o600 as c_int,
        )
    };
    let mut ok = created >= 0 && write_all_file(created, b"abcdef") == 6;
    if created >= 0 {
        unsafe {
            close(created);
        }
    }
    let existing = unsafe { open(path.as_ptr(), O_WRONLY | O_CREAT | O_EXCL, 0o600 as c_int) };
    let existing_errno = errno();
    let truncated = unsafe { creat(path.as_ptr(), 0o600) };
    let mut info = Stat::zero();
    let after_creat = truncated >= 0 && unsafe { fstat(truncated, &mut info) } == 0 && info.st_size == 0;
    if truncated >= 0 {
        unsafe {
            close(truncated);
        }
    }
    let reopened = unsafe { open(path.as_ptr(), O_WRONLY) };
    ok = ok && write_all_file(reopened, b"xy") == 2;
    if reopened >= 0 {
        unsafe {
            close(reopened);
        }
    }
    let re_truncated = unsafe { open(path.as_ptr(), O_RDWR | O_TRUNC) };
    let after_trunc =
        re_truncated >= 0 && unsafe { fstat(re_truncated, &mut info) } == 0 && info.st_size == 0;
    if re_truncated >= 0 {
        unsafe {
            close(re_truncated);
        }
    }

    ok = ok
        && existing == -1
        && existing_errno == EEXIST
        && after_creat
        && after_trunc;

    state.check(
        "open-flags",
        ok,
        format_args!(
            "created={} existing={} errno={} creat_size0={} trunc_size0={}",
            created,
            existing,
            existing_errno,
            flag(after_creat),
            flag(after_trunc)
        ),
    );
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_fd_flags(state: &mut State) {
    let path = path_for("fdflags");
    unsafe {
        unlink(path.as_ptr());
    }
    let first = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    let second = if first >= 0 {
        unsafe { open(path.as_ptr(), O_RDWR) }
    } else {
        -1
    };
    let first_flags = if first >= 0 {
        unsafe { fcntl(first, F_GETFL, 0) }
    } else {
        -1
    };
    let second_flags = if second >= 0 {
        unsafe { fcntl(second, F_GETFL, 0) }
    } else {
        -1
    };
    let set = if first >= 0 {
        unsafe { fcntl(first, F_SETFL, first_flags | O_APPEND) }
    } else {
        -1
    };
    let first_after = if first >= 0 {
        unsafe { fcntl(first, F_GETFL, 0) }
    } else {
        -1
    };
    let second_after = if second >= 0 {
        unsafe { fcntl(second, F_GETFL, 0) }
    } else {
        -1
    };
    let duplicate = if first >= 0 { unsafe { dup(first) } } else { -1 };
    let duplicate_flags = if duplicate >= 0 {
        unsafe { fcntl(duplicate, F_GETFL, 0) }
    } else {
        -1
    };
    let descriptor_set = if first >= 0 {
        unsafe { fcntl(first, F_SETFD, FD_CLOEXEC) }
    } else {
        -1
    };
    let first_fd_flags = if first >= 0 {
        unsafe { fcntl(first, F_GETFD, 0) }
    } else {
        -1
    };
    let duplicate_fd_flags = if duplicate >= 0 {
        unsafe { fcntl(duplicate, F_GETFD, 0) }
    } else {
        -1
    };

    let ok = first >= 0
        && second >= 0
        && set == 0
        && first_after & O_APPEND != 0
        && second_after & O_APPEND == 0
        && second_flags & O_APPEND == 0
        && duplicate >= 0
        && duplicate_flags & O_APPEND != 0
        && descriptor_set == 0
        && first_fd_flags & FD_CLOEXEC != 0
        && duplicate_fd_flags & FD_CLOEXEC == 0;

    state.check(
        "fd-flags",
        ok,
        format_args!(
            "open1=0x{:x} open2=0x{:x} after1=0x{:x} after2=0x{:x} dup=0x{:x} fd1=0x{:x} dup_fd=0x{:x}",
            first_flags,
            second_flags,
            first_after,
            second_after,
            duplicate_flags,
            first_fd_flags,
            duplicate_fd_flags
        ),
    );
    if duplicate >= 0 {
        unsafe {
            close(duplicate);
        }
    }
    if second >= 0 {
        unsafe {
            close(second);
        }
    }
    if first >= 0 {
        unsafe {
            close(first);
        }
    }
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_cloexec(state: &mut State, mutation: bool) {
    let names = ["keep", "cloexec", "setfd", "plain"];
    let mut paths: [Text<128>; 4] = [Text::new(), Text::new(), Text::new(), Text::new()];
    let mut fds = [-1i32; 5];
    for index in 0..4 {
        paths[index] = path_for(names[index]);
        unsafe {
            unlink(paths[index].as_ptr());
        }
        fds[index] = unsafe {
            open(
                paths[index].as_ptr(),
                O_RDWR
                    | O_CREAT
                    | O_TRUNC
                    | if index == 1 { O_CLOEXEC } else { 0 },
                0o600 as c_int,
            )
        };
        if fds[index] < 0 {
            state.check(
                "cloexec",
                false,
                format_args!("open_{}_errno={}", names[index], errno()),
            );
            return;
        }
    }
    let mut ok = write_all_file(fds[0], b"K") == 1
        && write_all_file(fds[3], b"P") == 1
        && unsafe { lseek(fds[0], 0, SEEK_SET) } == 0
        && unsafe { lseek(fds[3], 0, SEEK_SET) } == 0
        && unsafe { fcntl(fds[2], F_SETFD, FD_CLOEXEC) } == 0;
    fds[4] = unsafe { fcntl(fds[3], F_DUPFD_CLOEXEC, 0) };
    let flags_keep = unsafe { fcntl(fds[0], F_GETFD, 0) };
    let flags_cloexec = unsafe { fcntl(fds[1], F_GETFD, 0) };
    let flags_setfd = unsafe { fcntl(fds[2], F_GETFD, 0) };
    let flags_plain = unsafe { fcntl(fds[3], F_GETFD, 0) };
    let flags_dupfd = if fds[4] >= 0 {
        unsafe { fcntl(fds[4], F_GETFD, 0) }
    } else {
        -1
    };

    let keep_text = number(fds[0]);
    let cloexec_text = number(fds[1]);
    let setfd_text = number(fds[2]);
    let plain_text = number(fds[3]);
    let dupfd_text = number(fds[4]);

    let mut status = -1;
    let child = if ok { unsafe { fork() } } else { -1 };
    if child == 0 {
        let mut arguments = [null::<c_char>(); 9];
        arguments[0] = b"descriptor-conformance\0".as_ptr() as *const c_char;
        arguments[1] = b"--exec-child\0".as_ptr() as *const c_char;
        arguments[2] = keep_text.as_ptr();
        arguments[3] = cloexec_text.as_ptr();
        arguments[4] = setfd_text.as_ptr();
        arguments[5] = plain_text.as_ptr();
        arguments[6] = dupfd_text.as_ptr();
        if mutation {
            arguments[7] = b"--mutation-cloexec-survives\0".as_ptr() as *const c_char;
        }
        let environment = [
            b"PATH=/bin\0".as_ptr() as *const c_char,
            b"HOME=/tmp\0".as_ptr() as *const c_char,
            null::<c_char>(),
        ];
        unsafe {
            execve(SELF.as_ptr() as *const c_char, arguments.as_ptr(), environment.as_ptr());
            _exit(127);
        }
    }
    if child > 0 {
        unsafe {
            waitpid(child, &mut status, 0);
        }
    }
    let child_ok = child > 0 && (status & 0x7f) == 0 && ((status >> 8) & 0xff) == 0;

    ok = ok
        && child_ok
        && flags_keep & FD_CLOEXEC == 0
        && flags_cloexec & FD_CLOEXEC != 0
        && flags_setfd & FD_CLOEXEC != 0
        && flags_plain & FD_CLOEXEC == 0
        && fds[4] >= 0
        && flags_dupfd & FD_CLOEXEC != 0;

    state.check(
        "cloexec",
        ok,
        format_args!(
            "child={} status=0x{:x} keep=0x{:x} cloexec=0x{:x} setfd=0x{:x} plain=0x{:x} dupfd={}/0x{:x} mutation={}",
            child,
            status,
            flags_keep,
            flags_cloexec,
            flags_setfd,
            flags_plain,
            fds[4],
            flags_dupfd,
            flag(mutation)
        ),
    );

    for index in 0..5 {
        if fds[index] >= 0 {
            unsafe {
                close(fds[index]);
            }
        }
        if index < 4 {
            unsafe {
                unlink(paths[index].as_ptr());
            }
        }
    }
}

fn row_eof(state: &mut State) {
    let path = path_for("eof");
    unsafe {
        unlink(path.as_ptr());
    }
    let fd = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    let mut ok = fd >= 0
        && write_all_file(fd, b"abc") == 3
        && unsafe { lseek(fd, 0, SEEK_SET) } == 0;
    let mut buffer = [0u8; 8];
    let first = if ok {
        unsafe { read(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let second = if ok {
        unsafe { read(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let third = if ok {
        unsafe { read(fd, buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    ok = ok && first == 3 && second == 0 && third == 0;

    state.check(
        "eof",
        ok,
        format_args!("read1={} read2={} read3={}", first, second, third),
    );
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
    unsafe {
        unlink(path.as_ptr());
    }
}

fn row_short_read(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let mut buffer = [0u8; 32];
    let mut ok = unsafe { pipe(pipefd.as_mut_ptr()) } == 0
        && write_all_file(pipefd[1], b"12345678") == 8;
    let first = if ok {
        unsafe { read(pipefd[0], buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let closed = pipefd[1] >= 0 && unsafe { close(pipefd[1]) } == 0;
    pipefd[1] = -1;
    let second = if ok {
        unsafe { read(pipefd[0], buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    ok = ok && closed && first == 8 && second == 0;

    state.check(
        "short-read",
        ok,
        format_args!("read1={} read2={}", first, second),
    );
    if pipefd[0] >= 0 {
        unsafe {
            close(pipefd[0]);
        }
    }
    if pipefd[1] >= 0 {
        unsafe {
            close(pipefd[1]);
        }
    }
}

fn row_eintr(state: &mut State) {
    let mut buffer = [0u8; 16];
    let mut empty = [-1i32; 2];
    let mut partial = [-1i32; 2];
    let handler = install_handler(SIGALRM, note_sigalrm);
    let mut ok = handler
        && unsafe { pipe(empty.as_mut_ptr()) } == 0
        && unsafe { pipe(partial.as_mut_ptr()) } == 0;

    SIGALRM_SEEN.store(0, Ordering::SeqCst);
    unsafe {
        alarm(1);
        set_errno(0);
    }
    let blocked = if ok {
        unsafe { read(empty[0], buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let blocked_errno = errno();
    let empty_signals = SIGALRM_SEEN.load(Ordering::SeqCst);
    unsafe {
        alarm(0);
    }

    ok = ok && write_all_file(partial[1], b"12") == 2;
    SIGALRM_SEEN.store(0, Ordering::SeqCst);
    unsafe {
        alarm(1);
        set_errno(0);
    }
    let short_count = if ok {
        unsafe { read(partial[0], buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let short_errno = errno();
    let partial_signals = SIGALRM_SEEN.load(Ordering::SeqCst);
    unsafe {
        alarm(0);
    }

    // EINTR proves a handler-bearing signal interrupted the blocking read.  The counter may be
    // updated on the return-to-user path, so it is diagnostic and not part of the verdict.  A
    // pipe read returns the available bytes instead of blocking to fill the buffer, so the
    // partial case asserts that a short read is data and never a spurious EINTR.
    ok = ok && blocked == -1 && blocked_errno == EINTR && short_count == 2 && short_errno == 0;

    state.check(
        "eintr",
        ok,
        format_args!(
            "empty={} errno={} signals={} partial={} errno={} partial_signals={}",
            blocked, blocked_errno, empty_signals, short_count, short_errno, partial_signals
        ),
    );
    for fd in empty {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
    for fd in partial {
        if fd >= 0 {
            unsafe {
                close(fd);
            }
        }
    }
}

fn row_nonblock(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let mut buffer = [0u8; 8];
    let mut ok = unsafe { pipe2(pipefd.as_mut_ptr(), O_NONBLOCK) } == 0;
    let flags = if ok {
        unsafe { fcntl(pipefd[0], F_GETFL, 0) }
    } else {
        -1
    };
    set_errno(0);
    let count = if ok {
        unsafe { read(pipefd[0], buffer.as_mut_ptr().cast::<c_void>(), buffer.len()) }
    } else {
        -1
    };
    let error = errno();
    let changed = if ok {
        unsafe { fcntl(pipefd[0], F_SETFL, flags & !O_NONBLOCK) }
    } else {
        -1
    };
    let changed_flags = if ok {
        unsafe { fcntl(pipefd[0], F_GETFL, 0) }
    } else {
        -1
    };

    ok = ok
        && flags & O_NONBLOCK != 0
        && count == -1
        && error == EAGAIN
        && changed == 0
        && changed_flags & O_NONBLOCK == 0;

    state.check(
        "nonblock",
        ok,
        format_args!(
            "flags=0x{:x} read={} errno={} after=0x{:x}",
            flags, count, error, changed_flags
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

fn row_epipe_sigpipe(state: &mut State) {
    let mut pipefd = [-1i32; 2];
    let mut ignored = [-1i32; 2];
    let mut ok = install_handler(SIGPIPE, note_sigpipe)
        && unsafe { pipe(pipefd.as_mut_ptr()) } == 0
        && unsafe { close(pipefd[0]) } == 0;
    pipefd[0] = -1;
    SIGPIPE_SEEN.store(0, Ordering::SeqCst);
    set_errno(0);
    let first = if ok {
        unsafe { c_write(pipefd[1], b"X".as_ptr().cast::<c_void>(), 1) }
    } else {
        -1
    };
    let first_errno = errno();
    let first_signal = SIGPIPE_SEEN.load(Ordering::SeqCst);

    let mut ignore = Sigaction {
        sa_handler: SIG_IGN,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    let ignore_ok = unsafe {
        sigemptyset(&mut ignore.sa_mask);
        sigaction(SIGPIPE, &ignore, null_mut()) == 0
    };
    let ok_ignored = ignore_ok
        && unsafe { pipe(ignored.as_mut_ptr()) } == 0
        && unsafe { close(ignored[0]) } == 0;
    ignored[0] = -1;
    set_errno(0);
    let second = if ok_ignored {
        unsafe { c_write(ignored[1], b"X".as_ptr().cast::<c_void>(), 1) }
    } else {
        -1
    };
    let second_errno = errno();

    ok = ok
        && first == -1
        && first_errno == EPIPE
        && first_signal == 1
        && ok_ignored
        && second == -1
        && second_errno == EPIPE;

    state.check(
        "epipe-sigpipe",
        ok,
        format_args!(
            "write1={} errno={} sigpipe={} write2={} errno={}",
            first, first_errno, first_signal, second, second_errno
        ),
    );
    if pipefd[1] >= 0 {
        unsafe {
            close(pipefd[1]);
        }
    }
    if ignored[1] >= 0 {
        unsafe {
            close(ignored[1]);
        }
    }
}

fn row_fstat_identity(state: &mut State) {
    let path = path_for("fstat");
    unsafe {
        unlink(path.as_ptr());
    }
    let fd = unsafe { open(path.as_ptr(), O_RDWR | O_CREAT | O_TRUNC, 0o600 as c_int) };
    let mut ok = fd >= 0 && write_all_file(fd, b"0123456789") == 10;
    let dupfd = if ok { unsafe { dup(fd) } } else { -1 };
    let (mut first, mut second, mut grown) = (Stat::zero(), Stat::zero(), Stat::zero());
    let before = dupfd >= 0
        && unsafe { fstat(fd, &mut first) } == 0
        && unsafe { fstat(dupfd, &mut second) } == 0;
    let grown_ok =
        dupfd >= 0 && write_all_file(dupfd, b"!") == 1 && unsafe { fstat(fd, &mut grown) } == 0;

    ok = ok
        && before
        && grown_ok
        && first.st_dev == second.st_dev
        && first.st_ino == second.st_ino
        && first.st_size == 10
        && u32::from(first.st_mode) & S_IFMT == S_IFREG
        && grown.st_size == 11
        && grown.st_ino == first.st_ino;

    state.check(
        "fstat-identity",
        ok,
        format_args!(
            "dev={}/{} ino={}/{} size={} grown={} regular={}",
            first.st_dev,
            second.st_dev,
            first.st_ino,
            second.st_ino,
            first.st_size,
            grown.st_size,
            flag(u32::from(first.st_mode) & S_IFMT == S_IFREG)
        ),
    );
    if dupfd >= 0 {
        unsafe {
            close(dupfd);
        }
    }
    if fd >= 0 {
        unsafe {
            close(fd);
        }
    }
    unsafe {
        unlink(path.as_ptr());
    }
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_dup_offset(&mut state, false);
    row_dup2_dupfd(&mut state);
    row_append(&mut state);
    row_open_flags(&mut state);
    row_fd_flags(&mut state);
    row_cloexec(&mut state, false);
    row_eof(&mut state);
    row_short_read(&mut state);
    row_eintr(&mut state);
    row_nonblock(&mut state);
    row_epipe_sigpipe(&mut state);
    row_fstat_identity(&mut state);
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
        "dup-copies-offset" => row_dup_offset(&mut state, true),
        "cloexec-survives" => row_cloexec(&mut state, true),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: c_int, argv: *const *const c_char) -> c_int {
    if argc > 1 && c_str_eq(unsafe { *argv.add(1) }, b"--exec-child") {
        return child_cloexec(argc, argv);
    }
    let mut failures = run_normal();
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv.
        if c_str_eq(unsafe { *argv.add(index as usize) }, b"--all") {
            failures |= run_mutation("dup-copies-offset");
            failures |= run_mutation("cloexec-survives");
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
