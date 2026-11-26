// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S181 process and exec conformance probe (test-only, Rust).
//!
//! Runs the POSIX/FreeBSD process contract on the release image: `waitpid` with `WNOHANG` and
//! `ECHILD`, exit-status encoding for 0/1/42, a killed child reported as a signal, status
//! retention until the parent waits, signal disposition and mask inheritance across `fork`,
//! handler reset and mask preservation across `exec`, descriptor inheritance with `O_CLOEXEC`,
//! the `argv`/`envp`/`auxv` start layout with a 16-byte aligned entry stack, the `AT_RANDOM`
//! policy, and an orphan reparented to init and reaped. One row runs an external command
//! (`/bin/brush -c 'exit 134'`) because that is the status the SSH gate quotes.
//!
//! The donor contract is POSIX process control and FreeBSD `sys/kern/kern_exit.c` (`exit1`,
//! `proc_reap`, `proc_reparent`), `sys/kern/kern_exec.c` (`exec_copyout_strings`, `execsigs`),
//! `sys/kern/kern_fork.c` (`fork1`) and `sys/kern/kern_descrip.c` (`fdcopy`, `fdcloseexec`);
//! the status macros are `sys/sys/wait.h` (`W_EXITCODE`, `WIFEXITED`, `WEXITSTATUS`).  No donor
//! expression is copied; the rows assert the observable contract.
//!
//! Two controlled mutations encode what a broken implementation would believe and therefore turn
//! their rows red against the correct product:
//!   signal-as-exit        -- a SIGKILL child is expected to report a normal exit
//!   reparent-stays-child  -- an orphan is expected to keep its dead parent

#![no_std]
#![no_main]

use core::ffi::{c_char, c_int, c_void};
use core::fmt::{self, Write};
use core::ptr::{null, null_mut};

#[path = "../harness/conformance.rs"]
mod conformance;

use conformance::State;

const TOPIC: &str = "process";
const SELF: &[u8] = b"/bin/conformance-probe\0";
const BRUSH: &[u8] = b"/bin/brush\0";

const O_RDWR: c_int = 2;
const O_CREAT: c_int = 0o100;
const O_TRUNC: c_int = 0o1000;
const O_CLOEXEC: c_int = 0o2000000;

const SEEK_SET: c_int = 0;

const F_GETFD: c_int = 1;

const WNOHANG: c_int = 1;

const EINTR: c_int = 4;
const EBADF: c_int = 9;
const ECHILD: c_int = 10;
const ESRCH: c_int = 3;

const SIGKILL: c_int = 9;
const SIGUSR1: c_int = 10;
const SIGUSR2: c_int = 12;

const SIG_BLOCK: c_int = 0;
const SIG_DFL: usize = 0;

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
const _: () = assert!(core::mem::size_of::<Sigset>() == 128);

#[repr(C)]
#[derive(Clone, Copy)]
struct Timespec {
    tv_sec: i64,
    tv_nsec: i64,
}

/// The child-side record written to the parent pipe before `exec`, and the fields the
/// replacement image fills in after `exec`.
#[repr(C)]
#[derive(Clone, Copy)]
struct ExecReport {
    argc: u64,
    envc: u64,
    argv_ok: u64,
    env_ok: u64,
    layout_ok: u64,
    stack_mod16: u64,
    execfn_ok: u64,
    aux_present: u64,
    aux_nonzero: u64,
    aux_readable: u64,
    aux_bytes_nonzero: u64,
    random_first: u64,
    sigreset_ok: u64,
    mask_ok: u64,
    plain_open: u64,
    plain_byte_ok: u64,
    cloexec_closed: u64,
    reserved: u64,
}

impl ExecReport {
    const ZERO: Self = Self {
        argc: 0,
        envc: 0,
        argv_ok: 0,
        env_ok: 0,
        layout_ok: 0,
        stack_mod16: 0,
        execfn_ok: 0,
        aux_present: 0,
        aux_nonzero: 0,
        aux_readable: 0,
        aux_bytes_nonzero: 0,
        random_first: 0,
        sigreset_ok: 0,
        mask_ok: 0,
        plain_open: 0,
        plain_byte_ok: 0,
        cloexec_closed: 0,
        reserved: 0,
    };
}

/// The pre-exec record: which inherited descriptors are still open after `fork`.
#[repr(C)]
#[derive(Clone, Copy)]
struct ForkRecord {
    plain_open: u64,
    cloexec_open: u64,
    reserved: u64,
}

unsafe extern "C" {
    fn open(path: *const c_char, flags: c_int, ...) -> c_int;
    fn close(fd: c_int) -> c_int;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    #[link_name = "write"]
    fn c_write(fd: c_int, buffer: *const c_void, count: usize) -> isize;
    fn lseek(fd: c_int, offset: i64, whence: c_int) -> i64;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
    fn pipe(fds: *mut c_int) -> c_int;
    fn unlink(path: *const c_char) -> c_int;
    fn fork() -> c_int;
    fn execve(path: *const c_char, argv: *const *const c_char, envp: *const *const c_char)
    -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn getpid() -> c_int;
    fn getppid() -> c_int;
    fn kill(pid: c_int, signal: c_int) -> c_int;
    fn pause() -> c_int;
    fn sigaction(signal: c_int, action: *const Sigaction, old: *mut Sigaction) -> c_int;
    fn sigemptyset(set: *mut Sigset) -> c_int;
    fn sigismember(set: *const Sigset, signal: c_int) -> c_int;
    fn sigprocmask(how: c_int, set: *const Sigset, old: *mut Sigset) -> c_int;
    fn sched_yield() -> c_int;
    fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> c_int;
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
        if self.len + bytes.len() >= self.data.len() {
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
    let _ = write!(&mut text, "/tmp/process-{}-{}\0", unsafe { getpid() }, suffix);
    text
}

fn number(value: i32) -> Text<16> {
    let mut text = Text::new();
    let _ = write!(&mut text, "{value}\0");
    text
}

fn flag(value: bool) -> i32 {
    if value { 1 } else { 0 }
}

fn printable(byte: u8) -> char {
    if byte.is_ascii_graphic() {
        byte as char
    } else {
        '.'
    }
}

fn write_all(fd: c_int, mut data: &[u8]) -> bool {
    while !data.is_empty() {
        // SAFETY: `data` is readable for the length passed.
        let count = unsafe { c_write(fd, data.as_ptr().cast::<c_void>(), data.len()) };
        if count < 0 {
            if errno() == EINTR {
                continue;
            }
            return false;
        }
        if count == 0 {
            return false;
        }
        data = &data[count as usize..];
    }
    true
}

fn read_exact(fd: c_int, buffer: &mut [u8]) -> bool {
    let mut offset = 0usize;
    while offset < buffer.len() {
        // SAFETY: the remaining buffer slice is writable for its length.
        let count = unsafe {
            read(
                fd,
                buffer[offset..].as_mut_ptr().cast::<c_void>(),
                buffer.len() - offset,
            )
        };
        if count < 0 {
            if errno() == EINTR {
                continue;
            }
            return false;
        }
        if count == 0 {
            return false;
        }
        offset += count as usize;
    }
    true
}

fn sleep_ms(ms: i64) {
    let request = Timespec {
        tv_sec: ms / 1000,
        tv_nsec: (ms % 1000) * 1_000_000,
    };
    // SAFETY: the request is readable and the remaining pointer is null.
    let _ = unsafe { nanosleep(&request, null_mut()) };
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

fn c_str_i32(pointer: *const c_char) -> i32 {
    let mut cursor = pointer as *const u8;
    let mut sign = 1;
    // SAFETY: the caller supplies a NUL-terminated argv entry.
    if unsafe { *cursor } == b'-' {
        sign = -1;
        // SAFETY: advance within the same numeric string.
        cursor = unsafe { cursor.add(1) };
    }
    let mut value = 0i32;
    let mut digits = 0usize;
    loop {
        // SAFETY: advance within the same NUL-terminated string.
        let byte = unsafe { *cursor };
        if byte == 0 {
            break;
        }
        if !byte.is_ascii_digit() || digits >= 9 {
            return -1;
        }
        value = value * 10 + i32::from(byte - b'0');
        digits += 1;
        // SAFETY: advance within the same numeric string.
        cursor = unsafe { cursor.add(1) };
    }
    if digits == 0 { -1 } else { sign * value }
}

fn wif_exited(status: i32) -> bool {
    status & 0x7f == 0
}

fn wexit_status(status: i32) -> i32 {
    (status >> 8) & 0xff
}

fn wif_signaled(status: i32) -> bool {
    let low = status & 0x7f;
    low != 0 && low != 0x7f
}

fn wterm_sig(status: i32) -> i32 {
    status & 0x7f
}

fn fd_is_open(fd: c_int) -> bool {
    // SAFETY: fcntl only inspects the descriptor.
    unsafe { fcntl(fd, F_GETFD, 0) >= 0 }
}

fn install_handler(signal: c_int, handler: extern "C" fn(c_int)) -> bool {
    let mut action = Sigaction {
        sa_handler: handler as *const () as usize,
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

fn default_action(signal: c_int) -> bool {
    let mut action = Sigaction {
        sa_handler: 0,
        sa_mask: Sigset::ZERO,
        sa_flags: 0,
        _pad: 0,
        sa_restorer: 0,
    };
    // SAFETY: the old-action destination is writable.
    unsafe { sigaction(signal, null(), &mut action) == 0 && action.sa_handler == SIG_DFL }
}

fn mask_contains(signal: c_int) -> bool {
    let mut set = Sigset::ZERO;
    // SAFETY: the destination set is writable.
    unsafe {
        sigprocmask(SIG_BLOCK, null(), &mut set) == 0 && sigismember(&set, signal) == 1
    }
}

fn block_signal(signal: c_int) -> bool {
    let mut set = Sigset::ZERO;
    // SAFETY: sigaddset is not linked; bit `signal - 1` is the public sigset encoding.
    let word = ((signal - 1) / 64) as usize;
    let bit = (signal - 1) % 64;
    if word >= set.bytes.len() {
        return false;
    }
    set.bytes[word] |= 1u64 << bit;
    // SAFETY: `set` is readable for the call.
    unsafe { sigprocmask(SIG_BLOCK, &set, null_mut()) == 0 }
}

fn clear_signal_mask() -> bool {
    // SIG_SETMASK (2) with an empty set clears the mask; the probe installs no other blockers.
    let set = Sigset::ZERO;
    // SAFETY: `set` is readable for the call.
    unsafe { sigprocmask(2, &set, null_mut()) == 0 }
}

fn wait_blocking(pid: c_int) -> (bool, i32) {
    let mut status = 0i32;
    loop {
        // SAFETY: the status destination is writable.
        let waited = unsafe { waitpid(pid, &mut status, 0) };
        if waited == pid {
            return (true, status);
        }
        if waited < 0 && errno() == EINTR {
            continue;
        }
        return (false, status);
    }
}

extern "C" fn note_usr1(_signal: c_int) {}

/// First row: no children means `waitpid` reports `ECHILD`, not 0 or a block.
fn row_wait_echild(state: &mut State) {
    let mut status = 0i32;
    set_errno(0);
    // SAFETY: the status destination is writable.
    let waited = unsafe { waitpid(-1, &mut status, WNOHANG) };
    let error = errno();
    let ok = waited == -1 && error == ECHILD;
    state.check(
        "wait-echild",
        ok,
        format_args!("wait=-1/{} errno={error} expect_errno={ECHILD}", waited),
    );
}

/// Exit status 0, 1 and 42 decode through `WIFEXITED`/`WEXITSTATUS`.
fn row_exit_status(state: &mut State) {
    let codes = [0, 1, 42];
    let mut ok = true;
    let mut detail = [0i32; 3];
    let mut statuses = [0i32; 3];
    for (index, code) in codes.iter().enumerate() {
        // SAFETY: fork has no preconditions.
        let child = unsafe { fork() };
        if child == 0 {
            // SAFETY: `_exit` terminates this child only.
            unsafe { _exit(*code) };
        }
        let (waited, status) = if child > 0 {
            wait_blocking(child)
        } else {
            (false, 0)
        };
        let passed = waited
            && wif_exited(status)
            && wexit_status(status) == *code
            && !wif_signaled(status);
        detail[index] = if passed { *code } else { -1 };
        statuses[index] = status;
        ok = ok && passed;
    }
    state.check(
        "exit-status",
        ok,
        format_args!(
            "codes=0/1/42 status=0x{:x}/0x{:x}/0x{:x} decoded={}/{}/{}",
            statuses[0], statuses[1], statuses[2], detail[0], detail[1], detail[2]
        ),
    );
}

/// The release shell returns the command status the SSH gate quotes.
fn row_external_status(state: &mut State) {
    // SAFETY: fork has no preconditions.
    let child = unsafe { fork() };
    if child == 0 {
        let arguments = [
            b"brush\0".as_ptr() as *const c_char,
            b"-c\0".as_ptr() as *const c_char,
            b"exit 134\0".as_ptr() as *const c_char,
            null::<c_char>(),
        ];
        let environment = [
            b"PATH=/bin\0".as_ptr() as *const c_char,
            b"HOME=/tmp\0".as_ptr() as *const c_char,
            null::<c_char>(),
        ];
        // SAFETY: the path and both arrays are NUL-terminated and live for the call.
        unsafe {
            execve(
                BRUSH.as_ptr() as *const c_char,
                arguments.as_ptr(),
                environment.as_ptr(),
            );
            _exit(127);
        }
    }
    let (waited, status) = if child > 0 {
        wait_blocking(child)
    } else {
        (false, 0)
    };
    let ok = child > 0 && waited && wif_exited(status) && wexit_status(status) == 134;
    state.check(
        "external-status",
        ok,
        format_args!(
            "waited={} status=0x{:x} exited={} code={} signaled={} signal={} expect=exit 134",
            flag(waited),
            status,
            flag(wif_exited(status)),
            wexit_status(status),
            flag(wif_signaled(status)),
            wterm_sig(status)
        ),
    );
}

/// A killed child reports the signal, never a normal exit.
fn row_signal_status(state: &mut State, mutation: bool) {
    // SAFETY: fork has no preconditions.
    let child = unsafe { fork() };
    if child == 0 {
        // SAFETY: this child owns itself.
        unsafe {
            pause();
            _exit(0);
        }
    }
    let mut kill_ok = false;
    if child > 0 {
        // SAFETY: the child pid was just returned by fork.
        kill_ok = unsafe { kill(child, SIGKILL) } == 0;
    }
    let (waited, status) = if child > 0 {
        wait_blocking(child)
    } else {
        (false, 0)
    };
    let signaled = waited && wif_signaled(status) && wterm_sig(status) == SIGKILL;
    // The mutation expects a SIGKILL child to report a normal exit, so the correct product fails
    // it; the normal row requires the signal.
    let expect_signaled = !mutation;
    let ok = kill_ok && (signaled == expect_signaled) && !wif_exited(status);
    state.check(
        "signal-status",
        ok,
        format_args!(
            "kill={} waited={} status=0x{:x} signaled={} signal={} expect={} mutation={}",
            flag(kill_ok),
            flag(waited),
            status,
            flag(wif_signaled(status)),
            wterm_sig(status),
            flag(expect_signaled),
            flag(mutation)
        ),
    );
}

/// `WNOHANG` returns 0 while a live child has no event; the blocking wait then returns 42.
fn row_wait_nohang(state: &mut State) {
    // SAFETY: each pipe writes two valid descriptor slots.
    let mut release = [-1i32; 2];
    let mut created = unsafe { pipe(release.as_mut_ptr()) } == 0;
    let mut status = 0i32;
    let mut first = -2i32;
    let mut first_errno = 0;
    let mut second = 0i32;
    let mut second_status = 0i32;
    if created {
        // SAFETY: fork has no preconditions.
        let child = unsafe { fork() };
        if child == 0 {
            // SAFETY: the descriptors are this child's own copies.
            unsafe {
                close(release[1]);
            }
            let mut byte = [0u8; 1];
            // SAFETY: the buffer is one writable byte.
            let _ = read_exact(release[0], &mut byte);
            // SAFETY: `_exit` terminates this child only.
            unsafe { _exit(42) };
        }
        if child > 0 {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(release[0]);
            }
            set_errno(0);
            // SAFETY: the status destination is writable.
            first = unsafe { waitpid(child, &mut status, WNOHANG) };
            first_errno = errno();
            // Release the child and wait for its exit.
            let byte = [b'x'];
            let released = write_all(release[1], &byte);
            second = child;
            let outcome = wait_blocking(child);
            second_status = outcome.1;
            created = created && released;
        } else {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(release[0]);
            }
            created = false;
        }
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(release[1]);
        }
    }
    let ok = created
        && first == 0
        && second > 0
        && wif_exited(second_status)
        && wexit_status(second_status) == 42;
    state.check(
        "wait-nohang",
        ok,
        format_args!(
            "nohang={first} errno={first_errno} child={second} status=0x{second_status:x} code={}",
            wexit_status(second_status)
        ),
    );
}

/// The exit status stays pending until the parent waits, and the pid is gone afterwards.
fn row_zombie_until_waited(state: &mut State) {
    // SAFETY: pipe writes two valid descriptor slots.
    let mut fds = [-1i32; 2];
    let created = unsafe { pipe(fds.as_mut_ptr()) } == 0;
    let mut written = false;
    let mut waited = false;
    let mut status = 0i32;
    let mut polls = 0u32;
    let mut existed = false;
    let mut gone = false;
    let mut second = -2i32;
    let mut second_errno = 0;
    if created {
        // SAFETY: fork has no preconditions.
        let child = unsafe { fork() };
        if child == 0 {
            // SAFETY: the descriptors are this child's own copies.
            unsafe {
                close(fds[0]);
            }
            let _ = write_all(fds[1], b"z");
            // SAFETY: `_exit` terminates this child only.
            unsafe { _exit(7) };
        }
        if child > 0 {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            let mut byte = [0u8; 1];
            written = read_exact(fds[0], &mut byte) && byte[0] == b'z';
            // Poll for the exit event; the child may still be running when the byte arrives.
            let mut polls_left = 2000u32;
            while polls_left > 0 {
                // SAFETY: the status destination is writable.
                let result = unsafe { waitpid(child, &mut status, WNOHANG) };
                polls = polls.saturating_add(1);
                if result == child {
                    waited = true;
                    break;
                }
                if result == 0 {
                    // SAFETY: signal 0 only checks that the pid exists.
                    existed = unsafe { kill(child, 0) } == 0;
                    polls_left = polls_left.saturating_sub(1);
                    if polls % 64 == 0 {
                        sleep_ms(1);
                    } else {
                        // SAFETY: sched_yield has no preconditions.
                        let _ = unsafe { sched_yield() };
                    }
                    continue;
                }
                break;
            }
            // SAFETY: signal 0 only checks that the pid exists.
            gone = unsafe { kill(child, 0) } == -1 && errno() == ESRCH;
            set_errno(0);
            // SAFETY: the status destination is writable.
            second = unsafe { waitpid(child, &mut status, WNOHANG) };
            second_errno = errno();
        } else {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
        }
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(fds[0]);
        }
    }
    let ok = created
        && written
        && waited
        && wif_exited(status)
        && wexit_status(status) == 7
        && gone
        && second == -1
        && second_errno == ECHILD;
    state.check(
        "zombie-until-waited",
        ok,
        format_args!(
            "written={} waited={} polls={polls} exists_seen={} status=0x{status:x} code={} gone={} second={} errno={second_errno}",
            flag(written),
            flag(waited),
            flag(existed),
            wexit_status(status),
            flag(gone),
            second
        ),
    );
}

/// A forked child inherits the caught handler and the blocked mask.
fn row_fork_signal_inheritance(state: &mut State) {
    // SAFETY: pipe writes two valid descriptor slots.
    let mut fds = [-1i32; 2];
    let mut created = unsafe { pipe(fds.as_mut_ptr()) } == 0;
    created = created && install_handler(SIGUSR1, note_usr1) && block_signal(SIGUSR2);
    let mut report = 0u64;
    let mut waited = false;
    if created {
        // SAFETY: fork has no preconditions.
        let child = unsafe { fork() };
        if child == 0 {
            // SAFETY: the descriptors are this child's own copies.
            unsafe {
                close(fds[0]);
            }
            let handler_ok = {
                let mut action = Sigaction {
                    sa_handler: 0,
                    sa_mask: Sigset::ZERO,
                    sa_flags: 0,
                    _pad: 0,
                    sa_restorer: 0,
                };
                // SAFETY: the old-action destination is writable.
                (unsafe { sigaction(SIGUSR1, null(), &mut action) == 0 })
                    && action.sa_handler == note_usr1 as *const () as usize
            };
            let mask_ok = mask_contains(SIGUSR2);
            let word = u64::from(handler_ok) | (u64::from(mask_ok) << 1);
            let bytes = word.to_le_bytes();
            let sent = write_all(fds[1], &bytes);
            // SAFETY: `_exit` terminates this child only.
            unsafe { _exit(if sent && handler_ok && mask_ok { 0 } else { 14 }) };
        }
        if child > 0 {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            let mut bytes = [0u8; 8];
            let read_ok = read_exact(fds[0], &mut bytes);
            report = u64::from_le_bytes(bytes);
            let outcome = wait_blocking(child);
            waited = outcome.0 && outcome.1 == 0;
            created = created && read_ok;
        } else {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            created = false;
        }
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(fds[0]);
        }
    }
    let _ = default_action(SIGUSR1);
    let _ = clear_signal_mask();
    let handler_ok = report & 1 != 0;
    let mask_ok = report & 2 != 0;
    let ok = created && waited && handler_ok && mask_ok;
    state.check(
        "fork-signal-inheritance",
        ok,
        format_args!(
            "pipe={} waited={} handler={} mask={} report=0x{report:x}",
            flag(created),
            flag(waited),
            flag(handler_ok),
            flag(mask_ok)
        ),
    );
}

/// The fork+exec child: verify inherited descriptors, then replace the image.
fn exec_child(pipe_write: c_int, plain_fd: c_int, cloexec_fd: c_int) -> ! {
    let fork_record = ForkRecord {
        plain_open: u64::from(fd_is_open(plain_fd)),
        cloexec_open: u64::from(fd_is_open(cloexec_fd)),
        reserved: 0,
    };
    let record_bytes = bytes_of(&fork_record);
    let sent = write_all(pipe_write, record_bytes);

    let pipe_text = number(pipe_write);
    let plain_text = number(plain_fd);
    let cloexec_text = number(cloexec_fd);
    let arguments = [
        b"process-conformance\0".as_ptr() as *const c_char,
        b"--exec-image\0".as_ptr() as *const c_char,
        pipe_text.as_ptr(),
        plain_text.as_ptr(),
        cloexec_text.as_ptr(),
        b"alpha\0".as_ptr() as *const c_char,
        b"beta gamma\0".as_ptr() as *const c_char,
        b"\0".as_ptr() as *const c_char,
        null::<c_char>(),
    ];
    let environment = [
        b"PATH=/bin\0".as_ptr() as *const c_char,
        b"PROC_TOKEN=a b\0".as_ptr() as *const c_char,
        b"EMPTY=\0".as_ptr() as *const c_char,
        b"HOME=/tmp\0".as_ptr() as *const c_char,
        null::<c_char>(),
    ];
    // SAFETY: the path and both arrays are NUL-terminated and live for the call.
    unsafe {
        execve(
            SELF.as_ptr() as *const c_char,
            arguments.as_ptr(),
            environment.as_ptr(),
        );
        let _ = sent;
        _exit(13);
    }
}

fn bytes_of<T>(value: &T) -> &[u8] {
    // SAFETY: `value` is a live, fully initialized value of a plain-old-data struct.
    unsafe {
        core::slice::from_raw_parts(
            (value as *const T).cast::<u8>(),
            core::mem::size_of::<T>(),
        )
    }
}

fn bytes_of_mut<T>(value: &mut T) -> &mut [u8] {
    // SAFETY: `value` is a live, writable value of a plain-old-data struct.
    unsafe {
        core::slice::from_raw_parts_mut(
            (value as *mut T).cast::<u8>(),
            core::mem::size_of::<T>(),
        )
    }
}

fn find_auxv(envp: *const *const u8, wanted: u64) -> (bool, u64) {
    let mut cursor = envp as *const usize;
    let mut entries = 0usize;
    // SAFETY: envp is the C8 environment array and is NUL-terminated.
    while unsafe { *cursor } != 0 && entries < 4096 {
        cursor = unsafe { cursor.add(1) };
        entries += 1;
    }
    // SAFETY: `cursor` now names the terminator; the auxiliary vector starts after it.
    cursor = unsafe { cursor.add(1) };
    let mut pairs = 0usize;
    while pairs < 128 {
        // SAFETY: the auxiliary vector is (key, value) pairs ending in AT_NULL.
        let key = unsafe { *cursor } as u64;
        // SAFETY: same vector, value slot.
        let value = unsafe { *cursor.add(1) } as u64;
        if key == 0 {
            return (false, 0);
        }
        if key == wanted {
            return (true, value);
        }
        // SAFETY: advance to the next pair.
        cursor = unsafe { cursor.add(2) };
        pairs += 1;
    }
    (false, 0)
}

fn c_str_len(pointer: *const c_char) -> usize {
    let mut cursor = pointer as *const u8;
    let mut length = 0usize;
    // SAFETY: argv entries are NUL-terminated; the bound keeps a broken image from spinning.
    while unsafe { *cursor } != 0 && length < 4096 {
        cursor = unsafe { cursor.add(1) };
        length += 1;
    }
    length
}

/// The replacement image: verify argv/envp, the start layout, auxv, signals and descriptors.
fn exec_image_child(argc: c_int, argv: *const *const c_char, envp: *const *const c_char) -> i32 {
    let mut report = ExecReport::ZERO;
    report.argc = u64::try_from(argc).unwrap_or(0);
    let argument = |index: i32| {
        if index < argc {
            // SAFETY: the index is inside argv.
            unsafe { *argv.add(index as usize) }
        } else {
            null::<c_char>()
        }
    };
    let pipe_fd = c_str_i32(argument(2));
    let plain_fd = c_str_i32(argument(3));
    let cloexec_fd = c_str_i32(argument(4));

    report.argv_ok = u64::from(
        argc == 8
            && c_str_eq(argument(0), b"process-conformance")
            && c_str_eq(argument(1), b"--exec-image")
            && pipe_fd >= 0
            && plain_fd >= 0
            && cloexec_fd >= 0
            && c_str_eq(argument(5), b"alpha")
            && c_str_eq(argument(6), b"beta gamma")
            && c_str_len(argument(7)) == 0,
    );

    let mut env_count = 0i32;
    // SAFETY: envp is the C8 environment array and is NUL-terminated.
    while unsafe { *envp.add(env_count as usize) } != null() && env_count < 4096 {
        env_count += 1;
    }
    report.envc = u64::try_from(env_count).unwrap_or(0);
    let environment = |index: i32| {
        if index < env_count {
            // SAFETY: the index is inside envp.
            unsafe { *envp.add(index as usize) }
        } else {
            null::<c_char>()
        }
    };
    report.env_ok = u64::from(
        env_count == 4
            && c_str_eq(environment(0), b"PATH=/bin")
            && c_str_eq(environment(1), b"PROC_TOKEN=a b")
            && c_str_eq(environment(2), b"EMPTY=")
            && c_str_eq(environment(3), b"HOME=/tmp"),
    );

    // The C8 block starts at argc; argv is the next word, so entry sp0 is argv - 8.
    let sp0 = (argv as usize).wrapping_sub(8);
    report.stack_mod16 = (sp0 % 16) as u64;
    let argc_word = if sp0 == 0 {
        0usize
    } else {
        // SAFETY: sp0 is the C8 stack word the kernel built; argv came from it.
        unsafe { *(sp0 as *const usize) }
    };
    let argv_terminated = unsafe { *argv.add(argc as usize) } == null();
    report.layout_ok = u64::from(
        sp0 % 16 == 0 && argc_word == argc as usize && argv_terminated,
    );

    let execfn = find_auxv(envp as *const *const u8, 31);
    report.execfn_ok = u64::from(
        execfn.0 && execfn.1 != 0 && c_str_eq(execfn.1 as *const c_char, b"/bin/conformance-probe"),
    );

    let (present, random) = find_auxv(envp as *const *const u8, 25);
    report.aux_present = u64::from(present);
    report.aux_nonzero = u64::from(random != 0);
    if random != 0 {
        let mut bytes = [0u8; 16];
        // SAFETY: AT_RANDOM names 16 readable bytes on the initial stack.
        for (index, slot) in bytes.iter_mut().enumerate() {
            *slot = unsafe { *((random as *const u8).add(index)) };
        }
        report.aux_readable = 1;
        report.aux_bytes_nonzero = u64::from(bytes.iter().any(|byte| *byte != 0));
        report.random_first = u64::from_le_bytes([
            bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
        ]);
    }

    report.sigreset_ok = u64::from(default_action(SIGUSR1));
    report.mask_ok = u64::from(mask_contains(SIGUSR2));

    report.plain_open = u64::from(plain_fd >= 0 && fd_is_open(plain_fd));
    if report.plain_open == 1 {
        let mut byte = [0u8; 1];
        // SAFETY: the descriptor is the inherited readable end.
        report.plain_byte_ok = u64::from(read_exact(plain_fd, &mut byte) && byte[0] == b'P');
    }
    set_errno(0);
    // SAFETY: fcntl only inspects the descriptor.
    let closed = cloexec_fd >= 0
        && unsafe { fcntl(cloexec_fd, F_GETFD, 0) } == -1
        && errno() == EBADF;
    report.cloexec_closed = u64::from(closed);

    let all_ok = report.argv_ok == 1
        && report.env_ok == 1
        && report.layout_ok == 1
        && report.execfn_ok == 1
        && report.aux_present == 1
        && report.aux_nonzero == 1
        && report.aux_readable == 1
        && report.aux_bytes_nonzero == 1
        && report.sigreset_ok == 1
        && report.mask_ok == 1
        && report.plain_open == 1
        && report.plain_byte_ok == 1
        && report.cloexec_closed == 1;
    if pipe_fd >= 0 {
        let record = bytes_of(&report);
        let _ = write_all(pipe_fd, record);
    }
    if all_ok { 0 } else { 12 }
}

/// Fork, exec the probe itself, and assert the inherited/exec contract from the child's report.
fn row_exec_image(state: &mut State) {
    // SAFETY: each pipe writes two valid descriptor slots.
    let mut fds = [-1i32; 2];
    let mut created = unsafe { pipe(fds.as_mut_ptr()) } == 0;

    let plain_path = path_for("plain");
    let cloexec_path = path_for("cloexec");
    // SAFETY: the paths are NUL-terminated.
    let plain_fd = unsafe {
        open(
            plain_path.as_ptr(),
            O_RDWR | O_CREAT | O_TRUNC,
            0o600 as c_int,
        )
    };
    let plain_errno = errno();
    let wrote_plain = plain_fd >= 0 && write_all(plain_fd, b"P");
    created = created && wrote_plain;
    if plain_fd >= 0 {
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            lseek(plain_fd, 0, SEEK_SET);
        }
    }
    // SAFETY: the path is NUL-terminated.
    let cloexec_fd = unsafe { open(cloexec_path.as_ptr(), O_RDWR | O_CLOEXEC) };
    let cloexec_errno = errno();
    created = created && cloexec_fd >= 0;
    conformance::note(
        state,
        format_args!(
            "exec-setup created={} pipe={}/{} plain={}/{} wrote={} cloexec={}/{}",
            flag(created),
            fds[0],
            fds[1],
            plain_fd,
            plain_errno,
            flag(wrote_plain),
            cloexec_fd,
            cloexec_errno
        ),
    );

    created = created && install_handler(SIGUSR1, note_usr1) && block_signal(SIGUSR2);

    let mut fork_record = ForkRecord {
        plain_open: 0,
        cloexec_open: 0,
        reserved: 0,
    };
    let mut report = ExecReport::ZERO;
    let mut read_fork = false;
    let mut read_report = false;
    let mut waited = false;
    let mut child_status = 0i32;

    if created {
        // SAFETY: fork has no preconditions.
        let child = unsafe { fork() };
        if child == 0 {
            // SAFETY: the descriptors are this child's own copies.
            unsafe {
                close(fds[0]);
            }
            exec_child(fds[1], plain_fd, cloexec_fd);
        }
        if child > 0 {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            read_fork = read_exact(fds[0], bytes_of_mut(&mut fork_record));
            read_report = read_exact(fds[0], bytes_of_mut(&mut report));
            let outcome = wait_blocking(child);
            waited = outcome.0;
            child_status = outcome.1;
            conformance::note(
                state,
                format_args!(
                    "exec-read fork={} report={} waited={} status=0x{:x}",
                    flag(read_fork),
                    flag(read_report),
                    flag(waited),
                    child_status
                ),
            );
        } else {
            conformance::note(
                state,
                format_args!("exec-fork failed errno={}", errno()),
            );
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
        }
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(fds[0]);
        }
    }

    let _ = default_action(SIGUSR1);
    let _ = clear_signal_mask();
    if plain_fd >= 0 {
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(plain_fd);
        }
    }
    if cloexec_fd >= 0 {
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(cloexec_fd);
        }
    }
    // SAFETY: the paths name the probe's own temporary files.
    unsafe {
        unlink(plain_path.as_ptr());
        unlink(cloexec_path.as_ptr());
    }

    let child_ok = waited && wif_exited(child_status) && wexit_status(child_status) == 0;
    let detail_status = child_status;

    let fork_plain = fork_record.plain_open == 1;
    let fork_cloexec = fork_record.cloexec_open == 1;
    let fd_ok = read_fork
        && fork_plain
        && fork_cloexec
        && report.plain_open == 1
        && report.plain_byte_ok == 1
        && report.cloexec_closed == 1;
    state.check(
        "fd-inheritance",
        fd_ok,
        format_args!(
            "read={} fork_plain={} fork_cloexec={} plain={}/{} cloexec_closed={} status=0x{:x}",
            flag(read_fork),
            flag(fork_plain),
            flag(fork_cloexec),
            flag(report.plain_open == 1),
            flag(report.plain_byte_ok == 1),
            flag(report.cloexec_closed == 1),
            detail_status
        ),
    );

    let layout_ok = read_report
        && report.argv_ok == 1
        && report.env_ok == 1
        && report.layout_ok == 1
        && report.execfn_ok == 1;
    state.check(
        "exec-start-layout",
        layout_ok && child_ok,
        format_args!(
            "argc={} envc={} argv={} env={} layout={} mod16={} execfn={} status=0x{:x}",
            report.argc,
            report.envc,
            flag(report.argv_ok == 1),
            flag(report.env_ok == 1),
            flag(report.layout_ok == 1),
            report.stack_mod16,
            flag(report.execfn_ok == 1),
            detail_status
        ),
    );

    let random_ok = read_report
        && report.aux_present == 1
        && report.aux_nonzero == 1
        && report.aux_readable == 1
        && report.aux_bytes_nonzero == 1;
    state.check(
        "at-random-policy",
        random_ok && child_ok,
        format_args!(
            "present={} nonzero={} readable={} bytes_nonzero={} first=0x{:x} policy=nonzero-or-unseeded",
            flag(report.aux_present == 1),
            flag(report.aux_nonzero == 1),
            flag(report.aux_readable == 1),
            flag(report.aux_bytes_nonzero == 1),
            report.random_first
        ),
    );

    let sigreset_ok = read_report && report.sigreset_ok == 1 && report.mask_ok == 1;
    state.check(
        "exec-sigreset",
        sigreset_ok && child_ok,
        format_args!(
            "read={} handler={} mask={} status=0x{:x}",
            flag(read_report),
            flag(report.sigreset_ok == 1),
            flag(report.mask_ok == 1),
            detail_status
        ),
    );
}

/// The orphan reports its pid, observes the reparent to init, and is reaped by the new parent.
fn row_reparent_reap(state: &mut State, mutation: bool) {
    // SAFETY: pipe writes two valid descriptor slots.
    let mut fds = [-1i32; 2];
    let mut created = unsafe { pipe(fds.as_mut_ptr()) } == 0;
    let mut parent_status = 0i32;
    let mut parent_waited = false;
    let mut orphan_pid = 0i32;
    let mut pid_read = false;
    let mut marker = 0u8;
    let mut marker_read = false;
    let mut not_child = false;
    let mut gone = false;
    let mut gone_polls = 0u32;

    if created {
        // SAFETY: fork has no preconditions.
        let parent = unsafe { fork() };
        if parent == 0 {
            // The intermediate parent forks the orphan and exits without waiting.
            // SAFETY: fork has no preconditions.
            let orphan = unsafe { fork() };
            if orphan == 0 {
                // SAFETY: the descriptors are this child's own copies.
                unsafe {
                    close(fds[0]);
                }
                let pid = unsafe { getpid() };
                let pid_bytes = i64::from(pid).to_le_bytes();
                let _ = write_all(fds[1], &pid_bytes);
                let mut observed = unsafe { getppid() };
                let mut spins = 0u32;
                while observed != 1 && spins < 2000 {
                    sleep_ms(1);
                    observed = unsafe { getppid() };
                    spins = spins.saturating_add(1);
                }
                let byte = if observed == 1 { b'R' } else { b'N' };
                let _ = write_all(fds[1], &[byte]);
                // SAFETY: `_exit` terminates this child only.
                unsafe { _exit(if observed == 1 { 3 } else { 4 }) };
            }
            if orphan > 0 {
                // SAFETY: the descriptor is this process's own copy.
                unsafe {
                    close(fds[1]);
                    _exit(5);
                }
            }
            // SAFETY: `_exit` terminates this child only.
            unsafe { _exit(6) };
        }
        if parent > 0 {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            let outcome = wait_blocking(parent);
            parent_waited = outcome.0;
            parent_status = outcome.1;
            let mut pid_bytes = [0u8; 8];
            pid_read = read_exact(fds[0], &mut pid_bytes);
            orphan_pid = u64::from_le_bytes(pid_bytes) as i32;
            marker_read = read_exact(fds[0], core::slice::from_mut(&mut marker));
        } else {
            // SAFETY: the descriptor is the probe's own.
            unsafe {
                close(fds[1]);
            }
            created = false;
        }
        // SAFETY: the descriptor is the probe's own.
        unsafe {
            close(fds[0]);
        }
    }

    if pid_read && orphan_pid > 0 {
        set_errno(0);
        let mut status = 0i32;
        // SAFETY: the status destination is writable.
        not_child = unsafe { waitpid(orphan_pid, &mut status, WNOHANG) } == -1 && errno() == ECHILD;
        let mut left = 2000u32;
        while left > 0 {
            // SAFETY: signal 0 only checks that the pid exists.
            let result = unsafe { kill(orphan_pid, 0) };
            if result == -1 && errno() == ESRCH {
                gone = true;
                break;
            }
            left = left.saturating_sub(1);
            gone_polls = gone_polls.saturating_add(1);
            if gone_polls % 64 == 0 {
                sleep_ms(1);
            } else {
                // SAFETY: sched_yield has no preconditions.
                let _ = unsafe { sched_yield() };
            }
        }
    }

    let reparented = marker_read && marker == b'R';
    // The mutation expects the orphan to keep its dead parent (ppid != 1), so the correct
    // product fails the mutated row.
    let expect_reparent = !mutation;
    let ok = created
        && parent_waited
        && wif_exited(parent_status)
        && wexit_status(parent_status) == 5
        && pid_read
        && marker_read
        && (reparented == expect_reparent)
        && (mutation || (not_child && gone));
    state.check(
        "reparent-reap",
        ok,
        format_args!(
            "parent={} status=0x{:x} code={} pid={} marker={} reparented={} expect={} not_child={} gone={} polls={gone_polls} mutation={}",
            flag(parent_waited),
            parent_status,
            wexit_status(parent_status),
            orphan_pid,
            printable(marker),
            flag(reparented),
            flag(expect_reparent),
            flag(not_child),
            flag(gone),
            flag(mutation)
        ),
    );
}

fn run_normal() -> i32 {
    let mut state = State::begin(TOPIC);
    row_wait_echild(&mut state);
    row_exit_status(&mut state);
    row_external_status(&mut state);
    row_signal_status(&mut state, false);
    row_wait_nohang(&mut state);
    row_zombie_until_waited(&mut state);
    row_fork_signal_inheritance(&mut state);
    row_exec_image(&mut state);
    row_reparent_reap(&mut state, false);
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
        "signal-as-exit" => row_signal_status(&mut state, true),
        "reparent-stays-child" => row_reparent_reap(&mut state, true),
        _ => state.check(name, false, format_args!("unknown mutation")),
    }
    conformance::mutation_end(&state, name)
}

#[unsafe(no_mangle)]
pub extern "C" fn main(
    argc: c_int,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    if argc > 1 {
        // SAFETY: argv has at least two entries.
        let mode = unsafe { *argv.add(1) };
        if c_str_eq(mode, b"--exec-image") {
            return exec_image_child(argc, argv, envp);
        }
    }
    let mut failures = run_normal();
    let mut index = 1;
    while index < argc {
        // SAFETY: index is inside argv.
        if c_str_eq(unsafe { *argv.add(index as usize) }, b"--all") {
            failures |= run_mutation("signal-as-exit");
            failures |= run_mutation("reparent-stays-child");
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
