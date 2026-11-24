// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Execute tests: fake-runner outcomes, case pre-commands, artifact cleanup and the
//! complete-log/live-QEMU success path.

use super::*;
use crate::testutil::TempDir;
use std::process::Stdio;
use std::time::{Duration, Instant};

fn case(timeout: u64, expect_exit: &str, expects: &str) -> Case {
    let text = format!(
        "name = \"unit\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = {timeout}\n\
         expect_exit = \"{expect_exit}\"\n{expects}"
    );
    Case::parse(&text).unwrap()
}

fn runner(script: &str) -> ExecuteOptions {
    let mut options = ExecuteOptions::new();
    options.runner = Some(vec!["sh".to_string(), "-c".to_string(), script.to_string()]);
    options
}

#[cfg(unix)]
fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

#[test]
fn runner_pass() {
    let case = case(5, "success", "[[expect]]\nregex = \"^hello$\"\n");
    let outcome = execute(&case, &runner("printf 'hello\\n'; exit 33"));
    assert_eq!(outcome.report.result, ResultKind::Pass);
    assert_eq!(outcome.report.exit_status, Some(33));
    assert_eq!(outcome.report.matched, vec!["^hello$"]);
    assert!(outcome.detail.is_none());
}

#[test]
fn unmatched_expectation_fails() {
    let case = case(5, "success", "[[expect]]\nregex = \"^missing$\"\n");
    let outcome = execute(&case, &runner("printf 'other\\n'; exit 33"));
    assert_eq!(outcome.report.result, ResultKind::Fail);
    assert_eq!(outcome.report.unmatched, vec!["^missing$"]);
    assert!(outcome.detail.unwrap().contains("unmatched"));
}

#[test]
fn expected_failure_passes() {
    let case = case(5, "failure", "");
    let outcome = execute(&case, &runner("exit 35"));
    assert_eq!(outcome.report.result, ResultKind::Pass);
    assert_eq!(outcome.report.exit_status, Some(35));
}

#[test]
fn expected_timeout_passes() {
    let case = case(1, "timeout", "");
    let outcome = execute(&case, &runner("sleep 5"));
    assert_eq!(outcome.report.result, ResultKind::Pass);
    assert!(outcome.report.duration_ms < 3000);
}

#[test]
fn unexpected_timeout_is_reported() {
    let case = case(1, "success", "");
    let outcome = execute(&case, &runner("sleep 5"));
    assert_eq!(outcome.report.result, ResultKind::Timeout);
    assert!(outcome.detail.unwrap().contains("timed out"));
}

#[test]
fn crash_is_reported() {
    let case = case(5, "success", "");
    let outcome = execute(&case, &runner("exit 7"));
    assert_eq!(outcome.report.result, ResultKind::Crash);
    assert_eq!(outcome.report.exit_status, Some(7));
}

#[test]
fn any_accepts_a_crash() {
    let case = case(5, "any", "");
    let outcome = execute(&case, &runner("exit 7"));
    assert_eq!(outcome.report.result, ResultKind::Pass);
}

#[test]
fn forbid_hit_fails() {
    let case = case(5, "success", "[[forbid]]\nregex = \"panic\"\n");
    let outcome = execute(&case, &runner("printf 'panic\\n'; exit 33"));
    assert_eq!(outcome.report.result, ResultKind::Fail);
    assert_eq!(outcome.report.forbidden_hits, vec!["panic"]);
}

#[test]
fn failing_one_shot_service_records_fail_not_timeout() {
    let case = case(
        5,
        "success",
        "[[expect]]\nregex = \"^done$\"\n[[forbid]]\nregex = \"^signal: fail\"\n",
    );
    let started = Instant::now();
    let outcome = execute(&case, &runner("printf 'signal: fail rows=2\\n'; sleep 30"));
    assert_eq!(outcome.report.result, ResultKind::Fail);
    assert_eq!(outcome.report.forbidden_hits, vec!["signal: fail rows=2"]);
    assert!(
        started.elapsed() < Duration::from_secs(3),
        "the runner waited for the 5 s timeout instead of the fail row: {:?}",
        started.elapsed()
    );
}

#[test]
fn missing_firmware_skips() {
    let temp = TempDir::new("execute-skip");
    let image = temp.path().join("dummy.img");
    std::fs::write(&image, b"image").unwrap();
    let case = Case::parse(
        "name = \"skip\"\narch = \"riscv64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n",
    )
    .unwrap();
    let mut options = ExecuteOptions::new();
    options.image = Some(image);
    let outcome = execute(&case, &options);
    assert_eq!(outcome.report.result, ResultKind::Skipped);
    assert!(outcome.detail.unwrap().contains("firmware"));
}

#[test]
fn missing_prebuilt_image_is_an_error() {
    let temp = TempDir::new("execute-nobuild");
    let case = Case::parse(
        "name = \"nobuild\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n",
    )
    .unwrap();
    let mut options = ExecuteOptions::new();
    options.out_dir = temp.path().to_path_buf();
    options.no_build = true;
    let outcome = execute(&case, &options);
    assert_eq!(outcome.report.result, ResultKind::Error);
    assert!(outcome.detail.unwrap().contains("--no-build"));
}

#[test]
fn missing_explicit_image_is_an_error() {
    let temp = TempDir::new("execute-image");
    let case = Case::parse(
        "name = \"image\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n",
    )
    .unwrap();
    let mut options = ExecuteOptions::new();
    options.image = Some(temp.path().join("missing.img"));
    let outcome = execute(&case, &options);
    assert_eq!(outcome.report.result, ResultKind::Error);
    assert!(outcome.detail.unwrap().contains("image not found"));
}

#[test]
fn arch_override_is_recorded() {
    let case = case(5, "success", "");
    let outcome = execute(&case, &runner("exit 33"));
    assert_eq!(outcome.report.arch, "x86_64");
    let mut options = runner("exit 33");
    options.arch = Some("aarch64".to_string());
    let outcome = execute(&case, &options);
    assert_eq!(outcome.report.arch, "aarch64");
    assert_eq!(outcome.report.result, ResultKind::Pass);
}

#[test]
fn a_skipped_case_does_not_run() {
    let text = "name = \"skipped\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                timeout_secs = 5\nexpect_exit = \"success\"\n\
                skip = \"needs S27 (DEBUG_READ)\"\n";
    let case = Case::parse(text).unwrap();
    let outcome = execute(&case, &runner("printf 'must not run\\n'; exit 1"));
    assert_eq!(outcome.report.result, ResultKind::Skipped);
    assert!(outcome.lines.is_empty());
    assert!(outcome.detail.unwrap().contains("S27"));
}

#[test]
fn steps_run_against_the_fake_runner() {
    let case = case(
        5,
        "success",
        "[[expect]]\nregex = \"^got: uname$\"\n\
         [[step]]\nwait = \"^ready$\"\nsend = \"uname\\n\"\ndelay_ms = 0\n",
    );
    let outcome = execute(
        &case,
        &runner("printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33"),
    );
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
}

#[test]
fn a_step_failure_fails_the_case_and_reports_the_step() {
    let case = case(
        5,
        "success",
        "[[step]]\nwait = \"^never$\"\ntimeout_secs = 1\n",
    );
    let outcome = execute(&case, &runner("printf 'started\\n'; sleep 5"));
    assert_eq!(outcome.report.result, ResultKind::Fail);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("step 1"), "{detail}");
    assert!(detail.contains("^never$"), "{detail}");
    assert!(detail.contains("started"), "{detail}");
}

#[test]
fn pre_commands_run_in_order() {
    let temp = TempDir::new("execute-pre");
    let marker = temp.path().join("order.txt");
    let text = format!(
        "name = \"pre\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         pre = [\"echo one >> {}\", \"echo two >> {}\"]\n",
        marker.display(),
        marker.display()
    );
    let case = Case::parse(&text).unwrap();
    let outcome = execute(&case, &runner("exit 33"));
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert_eq!(std::fs::read_to_string(&marker).unwrap(), "one\ntwo\n");
}

#[test]
fn a_failing_pre_command_is_an_error() {
    let temp = TempDir::new("execute-pre-fail");
    let marker = temp.path().join("ran.txt");
    let text = "name = \"pre-fail\"\narch = \"x86_64\"\nprofile = \"base\"\n\
                timeout_secs = 5\nexpect_exit = \"success\"\n\
                pre = [\"echo broken >&2; exit 7\"]\n";
    let case = Case::parse(text).unwrap();
    let outcome = execute(&case, &runner(&format!("touch {}", marker.display())));
    assert_eq!(outcome.report.result, ResultKind::Error);
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("pre[0]"), "{detail}");
    assert!(detail.contains("broken"), "{detail}");
    assert!(!marker.exists(), "the runner must not start");
}

#[test]
fn a_preflight_77_is_skipped_not_error() {
    let temp = TempDir::new("execute-preflight-skip");
    let marker = temp.path().join("ran.txt");
    let script = format!("{}/preflight-load.sh", env!("CARGO_MANIFEST_DIR"));
    let text = format!(
        "name = \"preflight-skip\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         pre = [\"sh {script} --load1 6.32 --max-load 5.0\"]\n"
    );
    let case = Case::parse(&text).unwrap();
    let outcome = execute(&case, &runner(&format!("touch {}", marker.display())));
    assert_eq!(
        outcome.report.result,
        ResultKind::Skipped,
        "{:?}",
        outcome.detail
    );
    let detail = outcome.detail.expect("detail");
    assert!(detail.contains("pre[0]"), "{detail}");
    assert!(detail.contains("waiting-for-window"), "{detail}");
    assert!(!marker.exists(), "the runner must not start under load");
}

#[test]
fn a_preflight_safe_value_continues() {
    let script = format!("{}/preflight-load.sh", env!("CARGO_MANIFEST_DIR"));
    let text = format!(
        "name = \"preflight-safe\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         pre = [\"sh {script} --load1 4.59 --max-load 5.0\"]\n"
    );
    let case = Case::parse(&text).unwrap();
    let outcome = execute(&case, &runner("exit 33"));
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
}

#[test]
fn both_timer_cases_carry_the_load_preflight() {
    for name in ["kernel-timer.toml", "kernel-timer-aarch64.toml"] {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("cases")
            .join(name);
        let case =
            Case::load(&path).unwrap_or_else(|error| panic!("{}: {error:#}", path.display()));
        let preflight: Vec<&str> = case
            .spec
            .pre
            .iter()
            .map(String::as_str)
            .filter(|command| command.contains("tests/qtest/preflight-load.sh"))
            .collect();
        assert_eq!(
            preflight.len(),
            1,
            "{name}: expected exactly one preflight pre step, got {:?}",
            case.spec.pre
        );
        assert!(
            preflight[0].contains("--max-load 5.0"),
            "{name}: the preflight must keep the measured 5.0 load bound: {}",
            preflight[0]
        );
    }
}

#[test]
fn a_skipped_case_does_not_run_pre_commands() {
    let temp = TempDir::new("execute-skip-pre");
    let marker = temp.path().join("ran.txt");
    let text = format!(
        "name = \"skip-pre\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\nskip = \"later\"\npre = [\"touch {}\"]\n",
        marker.display()
    );
    let case = Case::parse(&text).unwrap();
    let outcome = execute(&case, &runner("exit 33"));
    assert_eq!(outcome.report.result, ResultKind::Skipped);
    assert!(!marker.exists(), "a skipped case must not run pre commands");
}

/// The S137 regression: a complete guest log followed by a live process must not hold the case
/// until its timeout. The harness reaps it after the grace and passes on the complete expectation
/// set.
#[cfg(unix)]
#[test]
fn a_complete_log_with_a_live_runner_is_reaped_and_passes() {
    let temp = TempDir::new("execute-complete");
    let pid_file = temp.path().join("runner.pid");
    let case = case(
        30,
        "success",
        "[[expect]]\nregex = \"^hello$\"\n\
         [[expect]]\nregex = \"^world$\"\n",
    );
    let script = format!(
        "echo $$ > {}; printf 'hello\\n'; printf 'world\\n'; sleep 30",
        pid_file.display()
    );
    let started = Instant::now();
    let outcome = execute(&case, &runner(&script));
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the harness waited for the process timeout: {:?}",
        started.elapsed()
    );
    assert_eq!(outcome.lines, vec!["hello", "world"]);

    let pid: u32 = std::fs::read_to_string(&pid_file)
        .expect("runner pid file")
        .trim()
        .parse()
        .expect("runner pid");
    assert!(
        !process_alive(pid),
        "runner {pid} survived the completion reap"
    );
}

#[test]
fn run_with_no_pre_does_not_execute_pre() {
    let temp = TempDir::new("execute-no-pre");
    let marker = temp.path().join("pre-ran.txt");
    let text = format!(
        "name = \"no-pre\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = 5\n\
         expect_exit = \"success\"\n\
         pre = [\"echo ran > {}\"]\n",
        marker.display()
    );
    let case = Case::parse(&text).unwrap();
    let mut options = runner("exit 33");
    options.no_pre = true;
    let outcome = execute(&case, &options);
    assert_eq!(
        outcome.report.result,
        ResultKind::Pass,
        "{:?}",
        outcome.detail
    );
    assert!(
        !marker.exists(),
        "--no-pre executed the pre command and created {}",
        marker.display()
    );
    let detail = outcome.detail.expect("pre-skip detail");
    assert!(detail.contains("pre skipped: --no-pre"), "{detail}");
}

#[test]
fn run_artifacts_are_removed() {
    let temp = TempDir::new("execute-cleanup");
    let vars = temp.path().join("case.vars.fd");
    let qmp = temp.path().join("case.qmp");
    std::fs::write(&vars, b"vars").unwrap();
    std::fs::write(&qmp, b"socket").unwrap();
    remove_run_artifacts(&[vars.clone(), qmp.clone()]);
    assert!(!vars.exists());
    assert!(!qmp.exists());
}
