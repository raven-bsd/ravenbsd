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
    assert_eq!(stop_status(SIGSTOP), (19 << 8) | 0x7f);
    assert_eq!(stop_status(SIGTSTP), (20 << 8) | 0x7f);
    assert_eq!(CONTINUED_STATUS, 0xffff);
}

#[test]
fn wait_events_encode_once() {
    assert_eq!(WaitEvent::Exited(7).status(), 7 << 8);
    assert_eq!(WaitEvent::Stopped(SIGSTOP).status(), (19 << 8) | 0x7f);
    assert_eq!(WaitEvent::Continued.status(), 0xffff);
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
