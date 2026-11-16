// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Runner tests: serial capture, dialogue writes, timeouts and process-group cleanup.

use super::*;
use crate::case::Case;
use std::time::Instant;

fn spec(script: &str, timeout: Duration) -> RunSpec {
    RunSpec {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), script.to_string()],
        timeout,
    }
}

fn dialogue(step_tables: &str, timeout: u64) -> Dialogue {
    let text = format!(
        "name = \"d\"\narch = \"x86_64\"\nprofile = \"base\"\ntimeout_secs = {timeout}\n\
         expect_exit = \"success\"\n{step_tables}"
    );
    let case = Case::parse(&text).expect("case");
    Dialogue::from_case(&case).expect("dialogue")
}

#[test]
fn strips_crlf_and_ansi_escapes() {
    let output = run(&spec(
        "printf 'hello\\r\\n\\033[31mred\\033[0m\\n'",
        Duration::from_secs(5),
    ))
    .unwrap();
    assert_eq!(output.lines, vec!["hello", "red"]);
    assert_eq!(output.status, Some(0));
    assert!(output.step_failure.is_none());
}

#[test]
fn flushes_a_partial_last_line() {
    let output = run(&spec("printf 'tail'", Duration::from_secs(5))).unwrap();
    assert_eq!(output.lines, vec!["tail"]);
}

#[test]
fn binary_garbage_does_not_panic() {
    let output = run(&spec("printf '\\377\\376\\n'", Duration::from_secs(5))).unwrap();
    assert_eq!(output.lines.len(), 1);
    assert!(output.lines[0].contains('\u{fffd}'));
}

#[test]
fn very_long_line_does_not_hang() {
    let output = run(&spec(
        "head -c 1048576 /dev/zero | tr '\\0' x; printf '\\n'",
        Duration::from_secs(10),
    ))
    .unwrap();
    assert_eq!(output.lines.len(), 1);
    assert_eq!(output.lines[0].len(), 1_048_576);
}

#[test]
fn exit_status_is_captured() {
    let output = run(&spec("exit 33", Duration::from_secs(5))).unwrap();
    assert_eq!(output.status, Some(33));
    assert!(!output.timed_out);
}

#[test]
fn timeout_kills_a_sleeping_process() {
    let started = Instant::now();
    let output = run(&spec("sleep 5", Duration::from_millis(500))).unwrap();
    assert!(output.timed_out);
    assert!(started.elapsed() < Duration::from_secs(3));
    assert_eq!(output.status, None);
}

#[test]
fn dialogue_writes_to_the_guest_stdin() {
    let mut dialogue = dialogue(
        "[[step]]\nwait = \"^ready$\"\nsend = \"hello\\n\"\ndelay_ms = 0\n",
        5,
    );
    let output = run_dialogue(
        &spec(
            "printf 'ready\\n'; read line; printf 'got: %s\\n' \"$line\"; exit 33",
            Duration::from_secs(5),
        ),
        &mut dialogue,
    )
    .unwrap();
    assert!(output.step_failure.is_none(), "{:?}", output.step_failure);
    assert_eq!(output.status, Some(33));
    assert!(
        output.lines.contains(&"got: hello".to_string()),
        "{:?}",
        output.lines
    );
    assert!(dialogue.is_complete());
}

#[test]
fn dialogue_matches_a_partial_prompt() {
    let mut dialogue = dialogue(
        "[[step]]\nwait = \"^raven# $\"\nsend = \"uname\\n\"\ndelay_ms = 0\n",
        5,
    );
    let output = run_dialogue(
        &spec(
            "printf 'raven# '; read line; printf 'got: %s\\n' \"$line\"; exit 33",
            Duration::from_secs(5),
        ),
        &mut dialogue,
    )
    .unwrap();
    assert!(output.step_failure.is_none(), "{:?}", output.step_failure);
    assert!(
        output.lines.iter().any(|line| line.contains("got: uname")),
        "{:?}",
        output.lines
    );
}

#[test]
fn a_step_timeout_reports_the_index_pattern_and_lines() {
    let mut dialogue = dialogue("[[step]]\nwait = \"^never$\"\ntimeout_secs = 1\n", 10);
    let started = Instant::now();
    let script = "i=0; while [ $i -lt 30 ]; do printf 'line %s\\n' $i; i=$((i+1)); done; sleep 5";
    let output = run_dialogue(&spec(script, Duration::from_secs(10)), &mut dialogue).unwrap();
    let failure = output.step_failure.expect("step failure");
    assert_eq!(failure.index, 0);
    assert_eq!(failure.pattern, "^never$");
    assert_eq!(failure.lines.len(), 20);
    assert!(failure.lines[19].contains("line 29"), "{:?}", failure.lines);
    assert!(started.elapsed() < Duration::from_secs(4));
}

#[test]
fn a_process_exit_before_a_step_is_a_failure() {
    let mut dialogue = dialogue("[[step]]\nwait = \"^missing$\"\n", 5);
    let output = run_dialogue(
        &spec("printf 'other\\n'; exit 33", Duration::from_secs(5)),
        &mut dialogue,
    )
    .unwrap();
    let failure = output.step_failure.expect("step failure");
    assert_eq!(failure.index, 0);
    assert_eq!(failure.pattern, "^missing$");
    assert!(failure.reason.contains("exited"), "{}", failure.reason);
}

#[test]
fn qmp_payloads_are_parsed_as_a_chord() {
    let (path, keys) =
        qmp_action(b"@qmp:build/qtest/usb-keyboard.qmp:h,i,shift,b,ret").expect("QMP action");
    assert_eq!(path, PathBuf::from("build/qtest/usb-keyboard.qmp"));
    assert_eq!(keys, ["h", "i", "shift", "b", "ret"]);
    assert!(qmp_action(b"plain serial bytes").is_none());
    assert!(qmp_action(b"@qmp:path:").is_none());
    assert!(qmp_action(b"@qmp::h").is_none());
}

#[cfg(unix)]
#[test]
fn timeout_kills_the_whole_process_group() {
    let temp = crate::testutil::TempDir::new("runner-group");
    let pid_file = temp.path().join("child.pid");
    let script = format!("sleep 30 & echo $! > {}; wait", pid_file.display());
    let output = run(&spec(&script, Duration::from_millis(500))).unwrap();
    assert!(output.timed_out);

    let bytes = std::fs::read(&pid_file).expect("pid file written before the timeout");
    let pid: u32 = String::from_utf8_lossy(&bytes).trim().parse().unwrap();
    let mut alive = true;
    for _ in 0..20 {
        let status = Command::new("kill")
            .arg("-0")
            .arg(pid.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        if !status.success() {
            alive = false;
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    assert!(
        !alive,
        "child process {pid} survived the process-group kill"
    );
}

#[cfg(unix)]
#[test]
fn timeout_leaves_no_process_matching_the_runner() {
    let marker = format!("raven-qtest-orphan-{}", std::process::id());
    let script = format!("# {marker}\nsleep 30 & wait");
    let output = run(&spec(&script, Duration::from_millis(500))).unwrap();
    assert!(output.timed_out);
    thread::sleep(Duration::from_millis(100));
    let pgrep = Command::new("pgrep")
        .arg("-f")
        .arg(&marker)
        .output()
        .expect("pgrep runs");
    assert!(
        !pgrep.status.success(),
        "process survived the timeout: {}",
        String::from_utf8_lossy(&pgrep.stdout)
    );
}
