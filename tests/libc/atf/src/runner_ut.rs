// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::*;
use crate::TpCase;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

static TEST_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn work(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "raven-atf-run-test-{tag}-{}-{}",
        std::process::id(),
        TEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("work dir");
    path
}

fn case(ident: &str) -> TpCase {
    TpCase {
        ident: ident.to_string(),
        meta: BTreeMap::new(),
    }
}

fn options(timeout: u64) -> RunOptions {
    RunOptions {
        timeout_secs: timeout,
        vars: BTreeMap::new(),
        keep: false,
    }
}

#[test]
fn lists_the_golden_three_cases() {
    let cases = list(&fixture("tp-mixed.sh"), None, 5).expect("list");
    let idents: Vec<&str> = cases.iter().map(|case| case.ident.as_str()).collect();
    assert_eq!(idents, ["ok_case", "needs_root", "sleeper"]);
    assert_eq!(
        cases[1].meta.get("require.user").map(String::as_str),
        Some("root")
    );
    assert_eq!(cases[2].meta.get("timeout").map(String::as_str), Some("1"));
}

#[test]
fn passing_case_is_pass() {
    let root = work("pass");
    let observation = run_case(
        &fixture("tp-pass.sh"),
        &case("first"),
        Some(&fixture(".")),
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(
        (observation.verdict, observation.reason.as_str()),
        (Verdict::Pass, "")
    );
    assert!(fs::read_dir(&root).expect("read").next().is_none());
}

#[test]
fn failing_case_keeps_the_atf_reason() {
    let root = work("fail");
    let observation = run_case(
        &fixture("tp-fail.sh"),
        &case("failing"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "strchr returned NULL");
}

#[test]
fn skip_is_skip() {
    let root = work("skip");
    let observation = run_case(
        &fixture("tp-skip.sh"),
        &case("skipped"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(
        (observation.verdict, observation.reason.as_str()),
        (Verdict::Skip, "no network")
    );
}

#[test]
fn expected_failure_is_a_pass() {
    let root = work("xfail");
    let observation = run_case(
        &fixture("tp-xfail.sh"),
        &case("xfail"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
    assert_eq!(observation.reason, "xfail:PR 4711");
}

#[test]
fn expected_exit_is_a_pass() {
    let root = work("exit");
    let observation = run_case(
        &fixture("tp-exit.sh"),
        &case("exits_three"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn expected_exit_mismatch_is_a_fail() {
    let root = work("exit-mismatch");
    let observation = run_case(
        &fixture("tp-exit-mismatch.sh"),
        &case("exits_zero"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    assert!(observation.reason.contains("expected exit 3, got 0"));
}

#[test]
fn expected_signal_is_a_pass() {
    let root = work("signal");
    let observation = run_case(
        &fixture("tp-signal.sh"),
        &case("segv"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn expected_death_is_a_pass() {
    let root = work("death");
    let observation = run_case(
        &fixture("tp-death.sh"),
        &case("dies"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn expected_timeout_is_a_pass_when_the_group_is_killed() {
    let root = work("timeout");
    let observation = run_case(
        &fixture("tp-timeout.sh"),
        &case("hangs"),
        None,
        &root,
        &options(1),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn an_unexpected_timeout_is_a_fail() {
    let root = work("sleeper");
    let started = Instant::now();
    let observation = run_case(
        &fixture("tp-sleeper.sh"),
        &case("sleeps"),
        None,
        &root,
        &options(1),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "timeout after 1s");
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn no_result_file_is_a_fail() {
    let root = work("noresult");
    let observation = run_case(
        &fixture("tp-noresult.sh"),
        &case("silent"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    assert_eq!(observation.reason, "no result file (exit 0)");
}

#[test]
fn garbage_result_file_is_a_fail() {
    let root = work("garbage");
    let observation = run_case(
        &fixture("tp-garbage.sh"),
        &case("garbage"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    assert!(observation.reason.starts_with("unparsable result:"));
}

#[test]
fn environment_is_cleared_and_set() {
    let root = work("env");
    let observation = run_case(
        &fixture("tp-env.sh"),
        &case("env"),
        None,
        &root,
        &options(5),
    )
    .expect("run");
    assert_eq!(
        (observation.verdict, observation.reason.as_str()),
        (Verdict::Pass, "")
    );
}

#[test]
fn one_mibibyte_of_output_does_not_deadlock() {
    let root = work("1mib");
    let observation = run_case(
        &fixture("tp-1mib.sh"),
        &case("noisy"),
        None,
        &root,
        &options(10),
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Pass);
}

#[test]
fn a_timed_out_case_kills_its_grandchild() {
    let root = work("group");
    let options = RunOptions {
        timeout_secs: 1,
        vars: BTreeMap::new(),
        keep: true,
    };
    let observation = run_case(
        &fixture("tp-child.sh"),
        &case("forks"),
        None,
        &root,
        &options,
    )
    .expect("run");
    assert_eq!(observation.verdict, Verdict::Fail);
    let entry = fs::read_dir(&root)
        .expect("read work")
        .next()
        .expect("one work dir")
        .expect("entry");
    let pid_file = entry.path().join("child.pid");
    let pid = fs::read_to_string(pid_file).expect("pid file");
    let alive = std::process::Command::new("kill")
        .arg("-0")
        .arg(pid.trim())
        .status()
        .is_ok_and(|status| status.success());
    assert!(!alive, "grandchild {} survived the group kill", pid.trim());
}

#[test]
fn a_second_run_clears_stale_files_from_the_work_directory() {
    let root = work("fresh");
    let options = RunOptions {
        timeout_secs: 5,
        vars: BTreeMap::new(),
        keep: true,
    };
    run_case(
        &fixture("tp-pass.sh"),
        &case("first"),
        None,
        &root,
        &options,
    )
    .expect("first");
    let entry = fs::read_dir(&root)
        .expect("read")
        .next()
        .expect("work dir")
        .expect("entry");
    fs::write(entry.path().join("stale-marker"), "old").expect("marker");
    run_case(
        &fixture("tp-pass.sh"),
        &case("first"),
        None,
        &root,
        &options,
    )
    .expect("second");
    let entry = fs::read_dir(&root)
        .expect("read")
        .next()
        .expect("work dir")
        .expect("entry");
    assert!(!entry.path().join("stale-marker").exists());
}

#[test]
fn require_root_skips_for_a_normal_user() {
    let mut test = case("needs_root");
    test.meta
        .insert("require.user".to_string(), "root".to_string());
    let requirement = requirement_skip(&test, &options(5));
    if running_as_root() {
        assert!(requirement.is_none());
    } else {
        assert_eq!(requirement.as_deref(), Some("needs-root"));
    }
}

#[test]
fn require_progs_skips_a_missing_program() {
    let mut test = case("needs_prog");
    test.meta.insert(
        "require.progs".to_string(),
        "definitely-not-a-real-program-c71".to_string(),
    );
    assert_eq!(
        requirement_skip(&test, &options(5)).as_deref(),
        Some("needs-program:definitely-not-a-real-program-c71")
    );
}

#[test]
fn require_config_skips_an_absent_variable() {
    let mut test = case("needs_config");
    test.meta
        .insert("require.config".to_string(), "need_this".to_string());
    assert_eq!(
        requirement_skip(&test, &options(5)).as_deref(),
        Some("needs-config:need_this")
    );
}

#[test]
fn metadata_timeout_is_capped() {
    let mut test = case("capped");
    test.meta.insert("timeout".to_string(), "900".to_string());
    assert_eq!(case_timeout(&test, 120), 120);
    test.meta.insert("timeout".to_string(), "5".to_string());
    assert_eq!(case_timeout(&test, 120), 5);
    test.meta.remove("timeout");
    assert_eq!(case_timeout(&test, 120), 120);
}

#[test]
fn listing_a_missing_program_is_an_error() {
    assert!(list(Path::new("/nonexistent/raven-atf-run"), None, 1).is_err());
}
