// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host unit tests for the bounded xargs core.

use std::collections::VecDeque;

use super::*;

#[derive(Default)]
struct Recorder {
    commands: Vec<Vec<String>>,
    traces: Vec<Vec<String>>,
    statuses: VecDeque<ChildStatus>,
    failures: VecDeque<RunError>,
}

impl Runner for Recorder {
    fn trace(&mut self, command: &[String]) {
        self.traces.push(command.to_vec());
    }

    fn run(&mut self, command: &[String]) -> Result<ChildStatus, RunError> {
        self.commands.push(command.to_vec());
        if let Some(failure) = self.failures.pop_front() {
            return Err(failure);
        }
        Ok(self.statuses.pop_front().unwrap_or(ChildStatus::Code(0)))
    }
}

fn command_options(command: &[&str]) -> Options {
    Options {
        command: command.iter().map(|part| (*part).to_string()).collect(),
        ..Options::default()
    }
}

#[test]
fn parses_options_and_default_command() {
    let options = parse_args(["-0", "-n2", "-I{}", "-r", "-t", "-x", "-s", "100"]).unwrap();
    assert!(options.null);
    assert_eq!(options.max_args, Some(2));
    assert_eq!(options.replace.as_deref(), Some("{}"));
    assert!(options.no_run_if_empty);
    assert!(options.trace);
    assert!(options.exit_if_size);
    assert_eq!(options.max_size, 100);
    assert_eq!(options.command, vec!["echo"]);

    let options = parse_args(["--", "printf", "%s\\n"]).unwrap();
    assert_eq!(options.command, vec!["printf", "%s\\n"]);

    assert_eq!(parse_args(["-n0", "echo"]), Err(XargsError::Usage));
    assert_eq!(parse_args(["-s0", "echo"]), Err(XargsError::Usage));
    assert_eq!(parse_args(["-I", "", "echo"]), Err(XargsError::Usage));
    assert_eq!(parse_args(["-Q", "echo"]), Err(XargsError::Usage));
}

#[test]
fn batches_items() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.max_args = Some(2);
    let outcome = run("one two three four five\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![
            vec!["echo", "one", "two"],
            vec!["echo", "three", "four"],
            vec!["echo", "five"],
        ]
    );
    assert_eq!(outcome.invocations, 3);
    assert_eq!(outcome.exit_code, 0);
}

#[test]
fn handles_quotes_and_backslashes() {
    let mut recorder = Recorder::default();
    let options = command_options(&["echo"]);
    run("'one two' three\\ four\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![vec!["echo", "one two", "three four"]]
    );

    assert_eq!(
        run("'unterminated", &options, &mut Recorder::default()),
        Err(XargsError::UnmatchedQuote)
    );
    assert_eq!(
        run("trailing\\", &options, &mut Recorder::default()),
        Err(XargsError::TrailingBackslash)
    );
}

#[test]
fn handles_nul_items() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.null = true;
    run("one two\0three\0", &options, &mut recorder).unwrap();
    assert_eq!(recorder.commands, vec![vec!["echo", "one two", "three"]]);
}

#[test]
fn replaces_in_every_command_argument() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo", "pre-{}-post"]);
    options.replace = Some("{}".to_string());
    options.exit_if_size = true;
    run("a\nb c\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![vec!["echo", "pre-a-post"], vec!["echo", "pre-b c-post"],]
    );

    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo", "fixed"]);
    options.replace = Some("{}".to_string());
    run("a\nb\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![vec!["echo", "fixed"], vec!["echo", "fixed"]]
    );
}

#[test]
fn handles_empty_input() {
    let mut recorder = Recorder::default();
    let options = command_options(&["echo"]);
    let outcome = run("", &options, &mut recorder).unwrap();
    assert_eq!(recorder.commands, vec![vec!["echo"]]);
    assert_eq!(outcome.invocations, 1);

    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.no_run_if_empty = true;
    let outcome = run("", &options, &mut recorder).unwrap();
    assert!(recorder.commands.is_empty());
    assert_eq!(outcome.invocations, 0);

    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo", "{}"]);
    options.replace = Some("{}".to_string());
    let outcome = run("", &options, &mut recorder).unwrap();
    assert!(recorder.commands.is_empty());
    assert_eq!(outcome.invocations, 0);
}

#[test]
fn batches_by_size() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.max_size = 12;
    run("aaa bbb\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![vec!["echo", "aaa"], vec!["echo", "bbb"]]
    );
}

#[test]
fn x_rejects_an_n_group_that_exceeds_size() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.max_args = Some(3);
    options.max_size = 12;
    run("aaa bbb ccc\n", &options, &mut recorder).unwrap();
    assert_eq!(
        recorder.commands,
        vec![
            vec!["echo", "aaa"],
            vec!["echo", "bbb"],
            vec!["echo", "ccc"],
        ]
    );

    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.max_args = Some(3);
    options.max_size = 12;
    options.exit_if_size = true;
    assert_eq!(
        run("aaa bbb ccc\n", &options, &mut recorder),
        Err(XargsError::SizeExceeded)
    );
    assert!(recorder.commands.is_empty());
}

#[test]
fn oversized_item_is_an_error() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.max_size = 8;
    assert_eq!(
        run("overflow\n", &options, &mut recorder),
        Err(XargsError::SizeExceeded)
    );
    assert!(recorder.commands.is_empty());
}

#[test]
fn aggregates_child_status() {
    let mut recorder = Recorder::default();
    recorder.statuses.push_back(ChildStatus::Code(7));
    let outcome = run("x\n", &command_options(&["false"]), &mut recorder).unwrap();
    assert_eq!(outcome.exit_code, 123);

    let mut recorder = Recorder::default();
    recorder.statuses.push_back(ChildStatus::Code(255));
    let outcome = run("x\n", &command_options(&["sh"]), &mut recorder).unwrap();
    assert_eq!(outcome.exit_code, 124);

    let mut recorder = Recorder::default();
    recorder.statuses.push_back(ChildStatus::Signal(9));
    let outcome = run("x\n", &command_options(&["sh"]), &mut recorder).unwrap();
    assert_eq!(outcome.exit_code, 125);

    let mut recorder = Recorder::default();
    recorder.statuses.push_back(ChildStatus::Code(7));
    recorder.statuses.push_back(ChildStatus::Code(255));
    let mut options = command_options(&["sh"]);
    options.max_args = Some(1);
    let outcome = run("a b\n", &options, &mut recorder).unwrap();
    assert_eq!(outcome.exit_code, 124);
}

#[test]
fn reports_run_errors() {
    let mut recorder = Recorder::default();
    recorder.failures.push_back(RunError::NotFound);
    assert_eq!(
        run("x\n", &command_options(&["missing"]), &mut recorder),
        Err(XargsError::CommandNotFound)
    );

    let mut recorder = Recorder::default();
    recorder.failures.push_back(RunError::CannotInvoke);
    assert_eq!(
        run("x\n", &command_options(&["denied"]), &mut recorder),
        Err(XargsError::CommandNotInvokable)
    );
}

#[test]
fn traces_commands() {
    let mut recorder = Recorder::default();
    let mut options = command_options(&["echo"]);
    options.trace = true;
    run("one\n", &options, &mut recorder).unwrap();
    assert_eq!(recorder.traces, recorder.commands);
}
