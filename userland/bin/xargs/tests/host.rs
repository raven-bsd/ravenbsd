// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks with real child processes.

use std::process::{Command, Stdio};

use raven_xargs::{ChildStatus, Options, RunError, Runner, XargsError, parse_args, run};

#[derive(Default)]
struct ProcessRunner {
    outputs: Vec<Vec<u8>>,
}

impl Runner for ProcessRunner {
    fn trace(&mut self, _command: &[String]) {}

    fn run(&mut self, command: &[String]) -> Result<ChildStatus, RunError> {
        let mut process = Command::new(&command[0]);
        process.args(&command[1..]).stdin(Stdio::null());
        match process.output() {
            Ok(output) => {
                self.outputs.push(output.stdout);
                Ok(ChildStatus::Code(output.status.code().unwrap_or(255)))
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Err(RunError::NotFound),
            Err(_) => Err(RunError::CannotInvoke),
        }
    }
}

fn command_options(command: &[&str]) -> Options {
    parse_args(command).unwrap()
}

#[test]
fn real_echo_batches() {
    let mut runner = ProcessRunner::default();
    let mut options = command_options(&["echo"]);
    options.max_args = Some(2);
    let outcome = run("a b c\n", &options, &mut runner).unwrap();
    assert_eq!(outcome.invocations, 2);
    assert_eq!(outcome.exit_code, 0);
    assert_eq!(runner.outputs, vec![b"a b\n".to_vec(), b"c\n".to_vec()]);
}

#[test]
fn real_echo_replaces_items() {
    let mut runner = ProcessRunner::default();
    let mut options = command_options(&["echo", "pre-{}-post"]);
    options.replace = Some("{}".to_string());
    run("a\nb\n", &options, &mut runner).unwrap();
    assert_eq!(
        runner.outputs,
        vec![b"pre-a-post\n".to_vec(), b"pre-b-post\n".to_vec()]
    );
}

#[test]
fn real_failure_maps_to_123() {
    let mut runner = ProcessRunner::default();
    let options = command_options(&["false"]);
    let outcome = run("x\n", &options, &mut runner).unwrap();
    assert_eq!(outcome.exit_code, 123);
}

#[test]
fn missing_command_is_reported() {
    let mut runner = ProcessRunner::default();
    let options = command_options(&["definitely-not-a-command-xyz"]);
    assert_eq!(
        run("x\n", &options, &mut runner),
        Err(XargsError::CommandNotFound)
    );
}

#[test]
fn empty_input_runs_echo_once_unless_r() {
    let mut runner = ProcessRunner::default();
    let options = command_options(&["echo"]);
    let outcome = run("", &options, &mut runner).unwrap();
    assert_eq!(outcome.invocations, 1);
    assert_eq!(runner.outputs, vec![b"\n".to_vec()]);

    let mut runner = ProcessRunner::default();
    let mut options = command_options(&["echo"]);
    options.no_run_if_empty = true;
    let outcome = run("", &options, &mut runner).unwrap();
    assert_eq!(outcome.invocations, 0);
    assert!(runner.outputs.is_empty());
}
