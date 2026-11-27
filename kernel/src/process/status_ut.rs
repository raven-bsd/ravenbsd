// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the process-primitive encodings.

use super::*;

#[test]
fn exit_status_uses_the_high_byte() {
    assert_eq!(exit_status(0), 0);
    assert_eq!(exit_status(42), 42 << 8);
    assert_eq!(exit_status(255), 255 << 8);
    assert_eq!(exit_status(256), 0);
    assert_eq!(exit_status(-1), 255 << 8);
}

#[test]
fn stop_and_continue_statuses_match_posix() {
    assert_eq!(stop_status(SIGSTOP), (17 << 8) | 0x7f);
    assert_eq!(stop_status(SIGTSTP), (18 << 8) | 0x7f);
    assert_eq!(CONTINUED_STATUS, 0x13);
}

#[test]
fn wait_events_encode_once() {
    assert_eq!(WaitEvent::Exited(7).status(), 7 << 8);
    assert_eq!(WaitEvent::Stopped(SIGSTOP).status(), (17 << 8) | 0x7f);
    // FreeBSD `sys/sys/wait.h`: `WIFCONTINUED(x)` is `x == 0x13`, matching the product header.
    assert_eq!(WaitEvent::Continued.status(), 0x13);
}

#[test]
fn signal_death_uses_the_low_status_byte() {
    assert_eq!(signal_status(SIGTERM, false), SIGTERM as i32);
    assert_eq!(signal_status(3, true), 3 | 0x80);
    assert_eq!(
        WaitEvent::Exited(-(SIGTERM as i32)).status(),
        SIGTERM as i32
    );
    assert_eq!(WaitEvent::Exited(-((3 | 0x80) as i32)).status(), 3 | 0x80);
}

#[test]
fn default_actions_match_the_signal_table() {
    assert_eq!(default_action(SIGTERM), Some(DefaultAction::Terminate));
    assert_eq!(default_action(SIGCHLD), Some(DefaultAction::Ignore));
    assert_eq!(default_action(SIGKILL), Some(DefaultAction::Terminate));
    assert_eq!(default_action(SIGSTOP), Some(DefaultAction::Stop));
    assert_eq!(default_action(SIGCONT), Some(DefaultAction::Continue));
    assert_eq!(default_action(0), None);
    assert_eq!(default_action(65), None);
}

#[test]
fn signal_numbers_are_bounded() {
    assert!(signal_valid(SIGHUP));
    assert!(signal_valid(SIGKILL));
    assert!(signal_valid(SIG_MAX));
    assert!(!signal_valid(0));
    assert!(!signal_valid(SIG_MAX + 1));
    assert!(is_stop_signal(SIGSTOP));
    assert!(is_stop_signal(SIGTSTP));
    assert!(!is_stop_signal(SIGCHLD));
}

#[test]
fn native_signal_numbers_match_freebsd() {
    // FreeBSD sys/sys/signal.h at the S700 pin: the native domain is the FreeBSD numbering.
    assert_eq!(SIGUSR1, 30);
    assert_eq!(SIGCHLD, 20);
    assert_eq!(SIGSTOP, 17);
    assert_eq!(SIGTSTP, 18);
    assert_eq!(SIGCONT, 19);
    assert_eq!(SIGTTIN, 21);
    assert_eq!(SIGTTOU, 22);
}

#[test]
fn freebsd_default_actions_follow_sigproptbl() {
    // FreeBSD sys/kern/kern_sig.c sigproptbl: the numbers differ from the Linux table.
    assert_eq!(default_action(10), Some(DefaultAction::Core)); // SIGBUS
    assert_eq!(default_action(12), Some(DefaultAction::Core)); // SIGSYS
    assert_eq!(default_action(16), Some(DefaultAction::Ignore)); // SIGURG
    assert_eq!(default_action(17), Some(DefaultAction::Stop)); // SIGSTOP
    assert_eq!(default_action(18), Some(DefaultAction::Stop)); // SIGTSTP
    assert_eq!(default_action(19), Some(DefaultAction::Continue)); // SIGCONT
    assert_eq!(default_action(20), Some(DefaultAction::Ignore)); // SIGCHLD
    assert_eq!(default_action(30), Some(DefaultAction::Terminate)); // SIGUSR1
}

#[test]
fn wait_idtypes_and_continued_status_match_freebsd() {
    // FreeBSD sys/sys/wait.h: the idtype enum values are Solaris-compatible.
    assert_eq!(P_PID, 0);
    assert_eq!(P_PGID, 2);
    assert_eq!(P_ALL, 7);
    assert_eq!(CONTINUED_STATUS, 0x13);
}

#[test]
fn rlimits_validate_and_default() {
    assert!(Rlimit::new(1, 1).valid());
    assert!(Rlimit::new(1, 2).valid());
    assert!(!Rlimit::new(2, 1).valid());
    let limits = default_limits();
    assert_eq!(limits.len(), RLIMIT_COUNT);
    assert!(limits[RLIMIT_NPROC].valid());
    assert!(limits[RLIMIT_NOFILE].valid());
    assert_eq!(limits[RLIMIT_CORE].cur, 0);
}

#[test]
fn exit_code_range_is_the_low_byte() {
    assert!(exit_code_fits(0));
    assert!(exit_code_fits(255));
    assert!(!exit_code_fits(256));
    assert!(!exit_code_fits(-1));
}

#[test]
fn an_ignored_signal_is_discarded_not_fatal() {
    // SIGUSR1 has a default-terminate action; SIG_IGN must win over it.
    assert_eq!(
        signal_disposition(SIGUSR1, false, true, false),
        SignalDisposition::Discard
    );
    assert_eq!(
        signal_disposition(SIGTERM, false, true, false),
        SignalDisposition::Discard
    );
}

#[test]
fn a_blocked_default_signal_is_held_until_unblock() {
    // FreeBSD `tdsendsignal` takes SIG_HOLD for a masked signal; the default action runs in
    // `issignal`/`postsig` after the mask opens, never at generation.
    assert_eq!(
        signal_disposition(SIGTERM, false, false, true),
        SignalDisposition::Defer
    );
    assert_eq!(
        signal_disposition(SIGTSTP, false, false, true),
        SignalDisposition::Defer
    );
}

#[test]
fn an_unblocked_default_signal_takes_its_action_now() {
    assert_eq!(
        signal_disposition(SIGTERM, false, false, false),
        SignalDisposition::Default(DefaultAction::Terminate)
    );
    assert_eq!(
        signal_disposition(SIGSEGV, false, false, false),
        SignalDisposition::Default(DefaultAction::Core)
    );
    assert_eq!(
        signal_disposition(SIGTSTP, false, false, false),
        SignalDisposition::Default(DefaultAction::Stop)
    );
}

#[test]
fn kill_and_stop_can_never_be_held() {
    // SIGKILL/SIGSTOP are unmaskable, so a mask bit must not defer them (POSIX/FreeBSD).
    assert_eq!(
        signal_disposition(SIGKILL, false, false, true),
        SignalDisposition::Default(DefaultAction::Terminate)
    );
    assert_eq!(
        signal_disposition(SIGSTOP, false, false, true),
        SignalDisposition::Default(DefaultAction::Stop)
    );
}

#[test]
fn a_caught_signal_is_queued_even_when_blocked() {
    assert_eq!(
        signal_disposition(SIGUSR1, true, false, true),
        SignalDisposition::Deliver
    );
    assert_eq!(
        signal_disposition(SIGUSR1, true, false, false),
        SignalDisposition::Deliver
    );
}

#[test]
fn a_default_ignore_signal_is_discarded() {
    assert_eq!(
        signal_disposition(SIGCHLD, false, false, false),
        SignalDisposition::Discard
    );
}

/// Read one little-endian `i32` field of the `siginfo_t` bytes.
fn siginfo_i32(bytes: &[u8; SIGINFO_SIZE], offset: usize) -> i32 {
    let field = &bytes[offset..offset + 4];
    i32::from_le_bytes([field[0], field[1], field[2], field[3]])
}

/// Read one little-endian `i64` field of the `siginfo_t` bytes.
fn siginfo_i64(bytes: &[u8; SIGINFO_SIZE], offset: usize) -> i64 {
    let field = &bytes[offset..offset + 8];
    i64::from_le_bytes([
        field[0], field[1], field[2], field[3], field[4], field[5], field[6], field[7],
    ])
}

#[test]
fn wait6_options_follow_freebsd_checkopt() {
    // FreeBSD sys/kern/kern_exit.c:1387 wait6_checkopt: unknown bits are EINVAL and at
    // least one of WEXITED/WUNTRACED/WCONTINUED must be set.
    assert_eq!(WEXITED, 16);
    assert_eq!(WNOWAIT, 8);
    assert!(!wait6_options_valid(0));
    assert!(!wait6_options_valid(WNOHANG));
    assert!(!wait6_options_valid(WNOWAIT));
    assert!(!wait6_options_valid(WNOHANG | WNOWAIT));
    assert!(!wait6_options_valid(WEXITED | 0x8000_0000));
    assert!(wait6_options_valid(WEXITED));
    assert!(wait6_options_valid(WUNTRACED));
    assert!(wait6_options_valid(WCONTINUED));
    assert!(wait6_options_valid(WEXITED | WNOHANG));
    assert!(wait6_options_valid(WEXITED | WNOWAIT));
    assert!(
        wait6_options_valid(WUNTRACED | WCONTINUED | WNOWAIT),
        "WSTOPPED/WEXITED/WCONTINUED are the event selectors"
    );
}

#[test]
fn wait6_siginfo_reports_a_normal_exit() {
    // NetBSD tests/lib/libc/sys/t_wait.c wait6_exited: full 32-bit exit code, child pid/uid
    // and CLD_EXITED; the time fields stay zero until rusage accounting.
    let info = wait6_siginfo(WaitEvent::Exited(0x5a5a_5a5a), 42, 1000, false);
    assert_eq!(siginfo_i32(&info, 0), 20, "si_signo is SIGCHLD");
    assert_eq!(siginfo_i32(&info, 4), 0, "si_errno is zero");
    assert_eq!(siginfo_i32(&info, 8), CLD_EXITED);
    assert_eq!(siginfo_i32(&info, 16), 42, "si_pid");
    assert_eq!(siginfo_i32(&info, 20), 1000, "si_uid");
    assert_eq!(siginfo_i32(&info, 24), 0x5a5a_5a5a, "si_status");
    assert_eq!(siginfo_i64(&info, 32), 0, "si_utime");
    assert_eq!(siginfo_i64(&info, 40), 0, "si_stime");
}

#[test]
fn wait6_siginfo_reports_a_signalled_child() {
    let killed = wait6_siginfo(WaitEvent::Exited(-(SIGTERM as i32)), 7, 0, false);
    assert_eq!(siginfo_i32(&killed, 8), CLD_KILLED);
    assert_eq!(siginfo_i32(&killed, 24), SIGTERM as i32);

    let dumped = wait6_siginfo(WaitEvent::Exited(-(SIGSEGV as i32)), 8, 0, true);
    assert_eq!(siginfo_i32(&dumped, 8), CLD_DUMPED);
    assert_eq!(siginfo_i32(&dumped, 24), SIGSEGV as i32);

    let no_core = wait6_siginfo(WaitEvent::Exited(-(SIGQUIT as i32)), 9, 0, false);
    assert_eq!(siginfo_i32(&no_core, 8), CLD_KILLED);
    assert_eq!(siginfo_i32(&no_core, 24), SIGQUIT as i32);
}

#[test]
fn wait6_core_dump_follows_the_core_limit() {
    // FreeBSD `WCOREDUMP` in `p_xsig` is set only when the core file is produced; a zero
    // RLIMIT_CORE turns the same core-action signal into CLD_KILLED.
    assert!(core_dump(SIGSEGV, 1));
    assert!(core_dump(SIGQUIT, u64::MAX));
    assert!(!core_dump(SIGSEGV, 0));
    assert!(!core_dump(SIGTERM, u64::MAX));
    assert_eq!(
        wait6_status(WaitEvent::Exited(-(SIGSEGV as i32)), true),
        signal_status(SIGSEGV, true)
    );
    assert_eq!(
        wait6_status(WaitEvent::Exited(-(SIGSEGV as i32)), false),
        signal_status(SIGSEGV, false)
    );
}

#[test]
fn wait6_siginfo_reports_stop_and_continue() {
    let stopped = wait6_siginfo(WaitEvent::Stopped(SIGSTOP), 9, 0, false);
    assert_eq!(siginfo_i32(&stopped, 8), CLD_STOPPED);
    assert_eq!(siginfo_i32(&stopped, 24), SIGSTOP as i32);

    let continued = wait6_siginfo(WaitEvent::Continued, 9, 0, false);
    assert_eq!(siginfo_i32(&continued, 8), CLD_CONTINUED);
    assert_eq!(siginfo_i32(&continued, 24), SIGCONT as i32);
}
