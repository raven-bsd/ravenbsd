// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scripted-dialogue tests through the public API and the `--runner` fake runner.

use std::time::Instant;

use raven_qtest::{Case, ExecuteOptions, ResultKind, execute};

fn case_text(steps_and_expects: &str) -> String {
    format!(
        "name = \"dialogue\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n{steps_and_expects}"
    )
}

fn runner(script: &str) -> ExecuteOptions {
    let mut options = ExecuteOptions::new();
    options.runner = Some(vec!["sh".to_string(), "-c".to_string(), script.to_string()]);
    options
}

fn execute_text(text: &str, script: &str) -> raven_qtest::CaseOutcome {
    let case = Case::parse(text).expect("case parses");
    execute(&case, &runner(script))
}

#[test]
fn a_wait_then_send_dialogue_passes() {
    let text = case_text(
        "[[step]]\nwait = '^ready$'\nsend = \"uname\\n\"\ndelay_ms = 0\n\
         [[expect]]\nregex = '^got: uname$'\n",
    );
    let outcome = execute_text(
        &text,
        "printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33",
    );
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert!(outcome.lines.iter().any(|line| line == "got: uname"));
}

#[test]
fn a_partial_prompt_matches_without_a_newline() {
    let text = case_text(
        "[[step]]\nwait = '^raven# $'\nsend = \"uname\\n\"\ndelay_ms = 0\n\
         [[expect]]\nregex = 'got: uname$'\n",
    );
    let outcome = execute_text(
        &text,
        "printf 'raven# '; read line; printf 'got: %s\\n' \"$line\"; exit 33",
    );
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
}

#[test]
fn waits_consume_lines_one_after_another() {
    let text = case_text("[[step]]\nwait = '^same$'\n[[step]]\nwait = '^same$'\n");
    let outcome = execute_text(&text, "printf 'same\\nsame\\n'; exit 33");
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );

    let outcome = execute_text(&text, "printf 'same\\n'; exit 33");
    assert_eq!(outcome.report.result, ResultKind::Fail);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("step 2"), "{detail}");
}

#[test]
fn a_step_timeout_reports_index_regex_and_the_serial_tail() {
    let text = case_text("[[step]]\nwait = '^never$'\ntimeout_secs = 1\n");
    let started = Instant::now();
    let script = "i=0; while [ $i -lt 25 ]; do printf 'line %s\\n' $i; i=$((i+1)); done; sleep 5";
    let outcome = execute_text(&text, script);
    assert!(started.elapsed().as_secs() < 4);
    assert_eq!(outcome.report.result, ResultKind::Fail);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("step 1"), "{detail}");
    assert!(detail.contains("^never$"), "{detail}");
    assert!(detail.contains("last 20 serial line(s)"), "{detail}");
    assert!(detail.contains("line 24"), "{detail}");
    assert!(
        !detail.contains("line 4\n"),
        "the tail starts at line 5: {detail}"
    );
}

#[test]
fn a_process_exit_before_a_step_is_reported() {
    let text = case_text("[[step]]\nwait = '^later$'\n");
    let outcome = execute_text(&text, "printf 'bye\\n'; exit 33");
    assert_eq!(outcome.report.result, ResultKind::Fail);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("step 1"), "{detail}");
    assert!(detail.contains("^later$"), "{detail}");
    assert!(detail.contains("exited"), "{detail}");
}

#[test]
fn send_writes_verbatim_without_a_newline() {
    let text = case_text(
        "[[step]]\nsend = \"abcde\"\ndelay_ms = 0\n\
                          [[expect]]\nregex = '^payload=abcde$'\n",
    );
    let outcome = execute_text(
        &text,
        "payload=$(dd bs=1 count=5 2>/dev/null); printf 'payload=%s\\n' \"$payload\"; exit 33",
    );
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
}

#[test]
fn a_delay_pauses_before_sending() {
    let text = case_text("[[step]]\nsend = \"x\\n\"\ndelay_ms = 200\n");
    let started = Instant::now();
    let outcome = execute_text(&text, "read line; exit 33");
    let elapsed = started.elapsed();
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert!(
        elapsed.as_millis() >= 150,
        "the send happened after {elapsed:?}"
    );
}

#[test]
fn expects_still_see_lines_consumed_by_steps() {
    let text = case_text(
        "[[step]]\nwait = '^hello$'\n\
         [[expect]]\nregex = '^hello$'\n",
    );
    let outcome = execute_text(&text, "printf 'hello\\n'; exit 33");
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert_eq!(outcome.report.matched, vec!["^hello$"]);
}

#[test]
fn steps_and_expects_mix_in_one_case() {
    let text = case_text(
        "[[expect]]\nregex = '^boot$'\n\
         [[step]]\nwait = '^login:$'\nsend = \"root\\n\"\ndelay_ms = 0\n\
         [[expect]]\nregex = '^welcome root$'\n\
         [[step]]\nwait = '^welcome root$'\n",
    );
    let outcome = execute_text(
        &text,
        "printf 'boot\\nlogin:\\n'; read user; printf 'welcome %s\\n' \"$user\"; exit 33",
    );
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert_eq!(outcome.report.matched.len(), 2);
}

#[test]
fn a_pending_send_at_process_exit_is_reported() {
    let text = case_text("[[step]]\nsend = \"x\\n\"\ndelay_ms = 300\n");
    let outcome = execute_text(&text, "exit 33");
    assert_eq!(outcome.report.result, ResultKind::Fail);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("step 1"), "{detail}");
    assert!(detail.contains("send"), "{detail}");
    assert!(detail.contains("exited"), "{detail}");
}

#[test]
fn a_skipped_case_never_starts_the_runner() {
    let text = "name = \"skipped\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
                expect_exit = \"success\"\nskip = \"waiting for S27\"\n";
    let outcome = execute_text(text, "printf 'must not run\\n'; exit 1");
    assert_eq!(outcome.report.result, ResultKind::Skipped);
    assert!(outcome.detail.unwrap().contains("S27"));
    assert!(outcome.lines.is_empty());
}
