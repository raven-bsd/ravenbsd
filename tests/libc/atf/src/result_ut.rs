// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::{Expect, ProcessStatus, Verdict};

fn exited(code: i32) -> ProcessStatus {
    ProcessStatus::Exited(code)
}

fn signalled(signal: i32) -> ProcessStatus {
    ProcessStatus::Signaled(signal)
}

#[test]
fn passed_is_pass() {
    assert_eq!(parse_result("passed\n"), Ok(Expect::Pass));
    let (verdict, reason) = judge(&Expect::Pass, exited(0), false, 1);
    assert_eq!((verdict, reason.as_str()), (Verdict::Pass, ""));
}

#[test]
fn passed_with_extra_text_is_not_a_directive() {
    assert!(matches!(
        parse_result("passed: extra\n"),
        Err(ResultError::Unknown(_))
    ));
}

#[test]
fn failed_carries_its_reason() {
    assert_eq!(
        parse_result("failed: strchr returned NULL\n"),
        Ok(Expect::Fail("strchr returned NULL".to_string()))
    );
    let (verdict, reason) = judge(
        &Expect::Fail("strchr returned NULL".into()),
        exited(1),
        false,
        1,
    );
    assert_eq!(
        (verdict, reason.as_str()),
        (Verdict::Fail, "strchr returned NULL")
    );
}

#[test]
fn failed_with_exit_zero_is_still_fail() {
    let (verdict, _) = judge(&Expect::Fail("bad".into()), exited(0), false, 1);
    assert_eq!(verdict, Verdict::Fail);
}

#[test]
fn skipped_is_skip() {
    assert_eq!(
        parse_result("skipped: no network\n"),
        Ok(Expect::Skip("no network".to_string()))
    );
    let (verdict, reason) = judge(&Expect::Skip("no network".into()), exited(0), false, 1);
    assert_eq!((verdict, reason.as_str()), (Verdict::Skip, "no network"));
}

#[test]
fn expected_failure_is_a_pass_with_a_note() {
    assert_eq!(
        parse_result("expected_failure: PR 4711\n"),
        Ok(Expect::XFail("PR 4711".to_string()))
    );
    let (verdict, reason) = judge(&Expect::XFail("PR 4711".into()), exited(1), false, 1);
    assert_eq!((verdict, reason.as_str()), (Verdict::Pass, "xfail:PR 4711"));
}

#[test]
fn expected_exit_matches_the_status() {
    let expect = parse_result("expected_exit(3): exits three\n").expect("parse");
    let (verdict, _) = judge(&expect, exited(3), false, 1);
    assert_eq!(verdict, Verdict::Pass);
}

#[test]
fn expected_exit_mismatch_is_a_behaviour_failure() {
    let expect = parse_result("expected_exit(3): exits three\n").expect("parse");
    let (verdict, reason) = judge(&expect, exited(0), false, 1);
    assert_eq!(verdict, Verdict::Fail);
    assert!(reason.starts_with("behaviour:"));
    assert!(reason.contains("expected exit 3, got 0"));
}

#[test]
fn expected_exit_minus_one_accepts_any_exit() {
    let expect = parse_result("expected_exit(-1): any\n").expect("parse");
    assert_eq!(expect, Expect::ExitCode(None, "any".to_string()));
    assert_eq!(judge(&expect, exited(42), false, 1).0, Verdict::Pass);
}

#[test]
fn expected_signal_matches_a_signal() {
    let expect = parse_result("expected_signal(11): null deref\n").expect("parse");
    assert_eq!(judge(&expect, signalled(11), false, 1).0, Verdict::Pass);
}

#[test]
fn expected_signal_rejects_a_clean_exit() {
    let expect = parse_result("expected_signal(11): null deref\n").expect("parse");
    let (verdict, reason) = judge(&expect, exited(0), false, 1);
    assert_eq!(verdict, Verdict::Fail);
    assert!(reason.contains("expected signal 11"));
}

#[test]
fn expected_signal_minus_one_accepts_any_signal() {
    let expect = parse_result("expected_signal(-1): any\n").expect("parse");
    assert_eq!(judge(&expect, signalled(9), false, 1).0, Verdict::Pass);
}

#[test]
fn expected_death_accepts_nonzero_exit() {
    let expect = parse_result("expected_death: dies\n").expect("parse");
    assert_eq!(judge(&expect, exited(1), false, 1).0, Verdict::Pass);
    assert_eq!(judge(&expect, signalled(6), false, 1).0, Verdict::Pass);
    assert_eq!(judge(&expect, exited(0), false, 1).0, Verdict::Fail);
}

#[test]
fn expected_timeout_passes_only_on_a_timeout() {
    let expect = parse_result("expected_timeout: hangs\n").expect("parse");
    assert_eq!(judge(&expect, signalled(9), true, 2).0, Verdict::Pass);
    assert_eq!(judge(&expect, exited(0), false, 2).0, Verdict::Fail);
}

#[test]
fn timeout_without_expected_timeout_fails() {
    let (verdict, reason) = judge(&Expect::Pass, signalled(9), true, 7);
    assert_eq!(verdict, Verdict::Fail);
    assert_eq!(reason, "timeout after 7s");
}

#[test]
fn missing_result_after_clean_exit_is_a_fail() {
    let (verdict, reason) = judge_missing(exited(0), false, 5);
    assert_eq!(verdict, Verdict::Fail);
    assert_eq!(reason, "no result file (exit 0)");
}

#[test]
fn missing_result_after_signal_names_the_signal() {
    let (_, reason) = judge_missing(signalled(6), false, 5);
    assert_eq!(reason, "no result file (signal 6)");
}

#[test]
fn empty_result_file_is_an_error() {
    assert_eq!(parse_result(""), Err(ResultError::Empty));
    assert_eq!(parse_result("\n"), Err(ResultError::Empty));
}

#[test]
fn malformed_expected_exit_is_an_error() {
    assert!(matches!(
        parse_result("expected_exit(x): bad\n"),
        Err(ResultError::Malformed(_))
    ));
    assert!(matches!(
        parse_result("expected_exit(3\n"),
        Err(ResultError::Malformed(_))
    ));
}

#[test]
fn malformed_expected_exit_without_reason_is_an_error() {
    assert!(matches!(
        parse_result("expected_exit(3)\n"),
        Err(ResultError::Malformed(_))
    ));
}

#[test]
fn unknown_directive_is_reported() {
    assert!(matches!(
        parse_result("something else\n"),
        Err(ResultError::Unknown(_))
    ));
}

#[test]
fn unparsable_binary_is_a_fail_with_the_first_bytes() {
    let (verdict, reason) = judge_unparsable(b"\x00\x01not a result\n", exited(0));
    assert_eq!(verdict, Verdict::Fail);
    assert!(reason.starts_with("unparsable result:"));
    assert!(reason.contains("exit 0"));
}

#[test]
fn a_ten_kib_reason_is_kept_intact() {
    let reason = "x".repeat(10 * 1024);
    let expect = parse_result(&format!("failed: {reason}\n")).expect("parse");
    assert_eq!(expect.reason().len(), 10 * 1024);
}

#[test]
fn carriage_return_is_tolerated() {
    assert_eq!(parse_result("passed\r\n"), Ok(Expect::Pass));
}

#[test]
fn only_the_first_line_is_read() {
    assert_eq!(parse_result("passed\nfailed: later\n"), Ok(Expect::Pass));
}

#[test]
fn result_error_has_a_readable_display() {
    assert!(ResultError::Empty.to_string().contains("empty"));
    assert!(ResultError::Unknown("x".into()).to_string().contains('x'));
    assert!(ResultError::Malformed("y".into()).to_string().contains('y'));
}
