// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX process-primitive constants and status encoding.
//!
//! This module is shared by the kernel and the host fixture library, so the
//! wait-status and rlimit rules are testable without booting a kernel.

#![allow(dead_code)] // the shared constant table is exercised by the host tests

/// `waitid`/`waitpid` id type: a single process id (FreeBSD `P_PID`).
pub const P_PID: u32 = 0;
/// `waitid`/`waitpid` id type: a process group id (FreeBSD `P_PGID`).
pub const P_PGID: u32 = 2;
/// `waitid`/`waitpid` id type: any child (FreeBSD `P_ALL`).
pub const P_ALL: u32 = 7;

/// Return immediately when no child event is ready.
pub const WNOHANG: u32 = 1;
/// Report a stopped child.
pub const WUNTRACED: u32 = 2;
/// Report a continued child.
pub const WCONTINUED: u32 = 4;
/// The supported wait option bits.
pub const WAIT_OPTIONS: u32 = WNOHANG | WUNTRACED | WCONTINUED;

// The native signal domain is the FreeBSD numbering (`sys/sys/signal.h` at the S700 pin), not
// the Linux numbering. Every kernel table and frame uses these values.

/// Hangup.
pub const SIGHUP: u32 = 1;
/// Interrupt.
pub const SIGINT: u32 = 2;
/// Quit (default core dump).
pub const SIGQUIT: u32 = 3;
/// Illegal instruction (default core dump).
pub const SIGILL: u32 = 4;
/// Trace trap (default core dump).
pub const SIGTRAP: u32 = 5;
/// Abort (default core dump).
pub const SIGABRT: u32 = 6;
/// EMT instruction (default core dump).
pub const SIGEMT: u32 = 7;
/// Floating-point exception (default core dump).
pub const SIGFPE: u32 = 8;
/// Uncatchable termination.
pub const SIGKILL: u32 = 9;
/// Bus error (default core dump).
pub const SIGBUS: u32 = 10;
/// Segmentation violation (default core dump).
pub const SIGSEGV: u32 = 11;
/// Non-existent system call (default core dump).
pub const SIGSYS: u32 = 12;
/// Write on a pipe with no reader.
pub const SIGPIPE: u32 = 13;
/// Alarm clock.
pub const SIGALRM: u32 = 14;
/// Termination.
pub const SIGTERM: u32 = 15;
/// Urgent I/O condition (default ignore).
pub const SIGURG: u32 = 16;
/// Uncatchable stop.
pub const SIGSTOP: u32 = 17;
/// Terminal stop.
pub const SIGTSTP: u32 = 18;
/// Continue a stopped process.
pub const SIGCONT: u32 = 19;
/// Child exit/stop notification (default ignore).
pub const SIGCHLD: u32 = 20;
/// Background read stop.
pub const SIGTTIN: u32 = 21;
/// Background write stop.
pub const SIGTTOU: u32 = 22;
/// Asynchronous I/O (default ignore).
pub const SIGIO: u32 = 23;
/// Exceeded CPU time limit.
pub const SIGXCPU: u32 = 24;
/// Exceeded file size limit.
pub const SIGXFSZ: u32 = 25;
/// Virtual time alarm.
pub const SIGVTALRM: u32 = 26;
/// Profiling timer alarm.
pub const SIGPROF: u32 = 27;
/// Window size change (default ignore).
pub const SIGWINCH: u32 = 28;
/// Status request (default ignore).
pub const SIGINFO: u32 = 29;
/// User-defined signal 1.
pub const SIGUSR1: u32 = 30;
/// User-defined signal 2.
pub const SIGUSR2: u32 = 31;
/// Reserved by the thread library.
pub const SIGTHR: u32 = 32;
/// Highest accepted native signal number; FreeBSD reserves 32 for `SIGTHR`.
pub const SIG_MAX: u32 = 32;

/// True for a signal number the kernel accepts.
#[must_use]
pub const fn signal_valid(signo: u32) -> bool {
    signo >= 1 && signo <= SIG_MAX
}

/// True for the stop signals that change job-control state.
#[must_use]
pub const fn is_stop_signal(signo: u32) -> bool {
    matches!(signo, SIGSTOP | SIGTSTP | SIGTTIN | SIGTTOU)
}

/// Number of supported rlimit resources.
pub const RLIMIT_COUNT: usize = 16;
/// CPU time in seconds.
pub const RLIMIT_CPU: usize = 0;
/// Largest file size.
pub const RLIMIT_FSIZE: usize = 1;
/// Data segment size.
pub const RLIMIT_DATA: usize = 2;
/// Stack size.
pub const RLIMIT_STACK: usize = 3;
/// Core file size.
pub const RLIMIT_CORE: usize = 4;
/// Resident set size.
pub const RLIMIT_RSS: usize = 5;
/// Number of processes for this user.
pub const RLIMIT_NPROC: usize = 6;
/// Open file descriptors.
pub const RLIMIT_NOFILE: usize = 7;
/// Locked memory.
pub const RLIMIT_MEMLOCK: usize = 8;
/// Address-space size.
pub const RLIMIT_AS: usize = 9;
/// Infinite limit value.
pub const RLIM_INFINITY: u64 = u64::MAX;

/// One soft/hard resource limit pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rlimit {
    /// Soft limit.
    pub cur: u64,
    /// Hard limit.
    pub max: u64,
}

impl Rlimit {
    /// A limit pair.
    #[must_use]
    pub const fn new(cur: u64, max: u64) -> Self {
        Self { cur, max }
    }

    /// True when `cur <= max`.
    #[must_use]
    pub const fn valid(self) -> bool {
        self.cur <= self.max
    }
}

const INF: Rlimit = Rlimit::new(RLIM_INFINITY, RLIM_INFINITY);

/// Default limits inherited by a new process.
#[must_use]
pub const fn default_limits() -> [Rlimit; RLIMIT_COUNT] {
    [
        INF,
        INF,
        INF,
        Rlimit::new(8 * 1024 * 1024, 8 * 1024 * 1024),
        Rlimit::new(0, RLIM_INFINITY),
        INF,
        Rlimit::new(1024, 1024),
        Rlimit::new(256, 256),
        Rlimit::new(64 * 1024, 64 * 1024),
        INF,
        INF,
        INF,
        INF,
        INF,
        INF,
        INF,
    ]
}

/// The POSIX wait status for a normal exit.
#[must_use]
pub const fn exit_status(code: i32) -> i32 {
    (code & 0xff) << 8
}

/// The POSIX wait status for a process terminated by `signo` (core flag optional).
#[must_use]
pub const fn signal_status(signo: u32, core: bool) -> i32 {
    ((signo & 0x7f) as i32) | if core { 0x80 } else { 0 }
}

/// What a signal does when its disposition is `SIG_DFL`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultAction {
    /// Terminate with the signal status.
    Terminate,
    /// Terminate with the signal status and the core flag.
    Core,
    /// Discard the signal.
    Ignore,
    /// Stop the process.
    Stop,
    /// Continue a stopped process.
    Continue,
}

const DEFAULT_TERMINATE: u8 = 1;
const DEFAULT_CORE: u8 = 2;
const DEFAULT_IGNORE: u8 = 3;
const DEFAULT_STOP: u8 = 4;
const DEFAULT_CONTINUE: u8 = 5;

/// Signals 0..=32 in the FreeBSD numbering (`sys/kern/kern_sig.c` `sigproptbl`).
const DEFAULT_ACTIONS: [u8; 33] = [
    0, 1, 1, 2, 2, 2, 2, 2, 2, 1, 2, 2, 2, 1, 1, 1, 3, 4, 4, 5, 3, 4, 4, 3, 1, 1, 1, 1, 3, 3, 1, 1,
    0,
];

/// The default action of `signo`, or `None` for 0, 32 and values above 32.
#[must_use]
#[allow(clippy::indexing_slicing)] // the range is checked before the index
pub const fn default_action(signo: u32) -> Option<DefaultAction> {
    if signo == 0 || signo > SIG_MAX {
        return None;
    }
    match DEFAULT_ACTIONS[signo as usize] {
        DEFAULT_TERMINATE => Some(DefaultAction::Terminate),
        DEFAULT_CORE => Some(DefaultAction::Core),
        DEFAULT_IGNORE => Some(DefaultAction::Ignore),
        DEFAULT_STOP => Some(DefaultAction::Stop),
        DEFAULT_CONTINUE => Some(DefaultAction::Continue),
        _ => None,
    }
}

/// The POSIX wait status for a stopped child.
#[must_use]
pub const fn stop_status(signo: u32) -> i32 {
    (((signo & 0x7f) as i32) << 8) | 0x7f
}

/// The FreeBSD `W_CONTINUED` wait status (`0x13`, the `SIGCONT` number).
pub const CONTINUED_STATUS: i32 = 0x13;

/// One observable child event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitEvent {
    /// The child exited with this code.
    Exited(i32),
    /// The child stopped on this signal.
    Stopped(u32),
    /// A stopped child continued.
    Continued,
}

impl WaitEvent {
    /// Encode the event as a POSIX wait status.
    #[must_use]
    pub const fn status(self) -> i32 {
        match self {
            Self::Exited(code) if code < 0 => {
                let raw = code.unsigned_abs();
                signal_status(raw & 0x7f, raw & 0x80 != 0)
            }
            Self::Exited(code) => exit_status(code),
            Self::Stopped(signo) => stop_status(signo),
            Self::Continued => CONTINUED_STATUS,
        }
    }
}

/// True when an exit code fits the POSIX 8-bit status field.
#[must_use]
pub const fn exit_code_fits(code: i32) -> bool {
    code >= 0 && code <= 0xff
}

/// `wait6` reports an exited child (FreeBSD `WEXITED`).
pub const WEXITED: u32 = 16;
/// `wait6` reports an event without reaping it (FreeBSD `WNOWAIT`).
pub const WNOWAIT: u32 = 8;
/// Every option bit accepted by `PROC_WAIT6`.
pub const WAIT6_OPTIONS: u32 = WNOHANG | WUNTRACED | WCONTINUED | WNOWAIT | WEXITED;
/// Size of the product `siginfo_t` record.
pub const SIGINFO_SIZE: usize = 128;
/// Size of the zeroed `struct __wrusage` output of `wait6`.
pub const WAIT6_WRUSAGE_SIZE: usize = 288;

/// `si_code` of a child that exited normally.
pub const CLD_EXITED: i32 = 1;
/// `si_code` of a child killed by a signal.
pub const CLD_KILLED: i32 = 2;
/// `si_code` of a child killed by a signal that dumped core.
pub const CLD_DUMPED: i32 = 3;
/// `si_code` of a stopped child.
pub const CLD_STOPPED: i32 = 5;
/// `si_code` of a continued child.
pub const CLD_CONTINUED: i32 = 6;

/// True when a `PROC_WAIT6` option word is acceptable (FreeBSD `wait6_checkopt`).
///
/// `wait6_checkopt` rejects an unknown bit and an option word that selects none of
/// `WEXITED`/`WUNTRACED`/`WCONTINUED`; `options == 0` is therefore `EINVAL` before any child is
/// scanned.
#[must_use]
pub const fn wait6_options_valid(options: u32) -> bool {
    if options & !WAIT6_OPTIONS != 0 {
        return false;
    }
    options & (WEXITED | WUNTRACED | WCONTINUED) != 0
}

/// True when a signal death is reported as a core dump.
///
/// FreeBSD sets `WCOREDUMP` in `p_xsig` only when the core file is actually produced, which the
/// process core limit gates. The kernel stores the bare signal in `exit_code`, so the wait6
/// encoder re-applies the FreeBSD `RLIMIT_CORE` decision at reap time.
#[must_use]
pub const fn core_dump(signo: u32, core_limit: u64) -> bool {
    core_limit > 0 && matches!(default_action(signo), Some(DefaultAction::Core))
}

/// Encode one child event as the POSIX wait status word, honouring the core decision.
#[must_use]
pub const fn wait6_status(event: WaitEvent, core: bool) -> i32 {
    match event {
        WaitEvent::Exited(code) if code < 0 => {
            let raw = code.unsigned_abs();
            signal_status(raw & 0x7f, core)
        }
        other => other.status(),
    }
}

/// Build the product `siginfo_t` bytes for one child event.
///
/// The record is the musl-layout `siginfo_t` at the C21 offsets: signo 0, errno 4, code 8,
/// pid 16, uid 20, status 24, utime 32 and stime 40. `si_status` carries the full exit code,
/// the terminating signal, the stop signal or `SIGCONT`; the time fields stay zero until the
/// rusage accounting ticket (FreeBSD `wait_fill_siginfo`, `sys/kern/kern_exit.c:1141`).
#[must_use]
pub fn wait6_siginfo(event: WaitEvent, pid: i32, uid: u32, core: bool) -> [u8; SIGINFO_SIZE] {
    let (code, status) = match event {
        WaitEvent::Exited(code) if code < 0 => {
            let signo = i32::try_from(code.unsigned_abs() & 0x7f).unwrap_or(0);
            if core {
                (CLD_DUMPED, signo)
            } else {
                (CLD_KILLED, signo)
            }
        }
        WaitEvent::Exited(code) => (CLD_EXITED, code),
        WaitEvent::Stopped(signo) => (CLD_STOPPED, i32::try_from(signo).unwrap_or(0)),
        WaitEvent::Continued => (CLD_CONTINUED, i32::try_from(SIGCONT).unwrap_or(0)),
    };
    let mut info = [0u8; SIGINFO_SIZE];
    info[0..4].copy_from_slice(&i32::try_from(SIGCHLD).unwrap_or(0).to_le_bytes());
    info[8..12].copy_from_slice(&code.to_le_bytes());
    info[16..20].copy_from_slice(&pid.to_le_bytes());
    info[20..24].copy_from_slice(&uid.to_le_bytes());
    info[24..28].copy_from_slice(&status.to_le_bytes());
    info
}

/// What the kernel does with one generated signal for a target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignalDisposition {
    /// Discard the signal without queueing it (`SIG_IGN` or a default-ignore signal).
    Discard,
    /// Queue the signal for delivery to a user handler.
    Deliver,
    /// Queue the signal and apply its default action when the target unblocks it.
    Defer,
    /// Apply the default action now.
    Default(DefaultAction),
}

/// Decide one generated signal's disposition (FreeBSD `tdsendsignal` and `sigprocess`).
///
/// FreeBSD discards an ignored signal at generation (`kern_sig_discard_ign`), queues a caught
/// signal even while it is masked, and holds a default-action signal as `SIG_HOLD` while a
/// thread masks it; the default action runs at delivery (`issignal`/`postsig`). `SIGKILL` and
/// `SIGSTOP` can never be held.
pub const fn signal_disposition(
    signo: u32,
    caught: bool,
    ignored: bool,
    blocked: bool,
) -> SignalDisposition {
    if ignored {
        return SignalDisposition::Discard;
    }
    if caught {
        return SignalDisposition::Deliver;
    }
    let unblockable = signo == SIGKILL || signo == SIGSTOP;
    if blocked && !unblockable {
        return SignalDisposition::Defer;
    }
    match default_action(signo) {
        None | Some(DefaultAction::Ignore) => SignalDisposition::Discard,
        Some(action) => SignalDisposition::Default(action),
    }
}

#[cfg(test)]
#[path = "status_ut.rs"]
mod tests;
