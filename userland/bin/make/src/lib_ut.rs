// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the bounded make core.

use super::*;

struct TestFs {
    files: BTreeMap<String, (String, i64)>,
}

impl TestFs {
    fn new() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    fn file(&mut self, path: &str, text: &str, mtime: i64) {
        self.files
            .insert(path.to_string(), (text.to_string(), mtime));
    }
}

impl FileSystem for TestFs {
    fn exists(&mut self, path: &str) -> bool {
        self.files.contains_key(path)
    }

    fn mtime(&mut self, path: &str) -> Option<i64> {
        self.files.get(path).map(|(_, mtime)| *mtime)
    }

    fn read(&mut self, path: &str) -> Result<String, String> {
        self.files
            .get(path)
            .map(|(text, _)| text.clone())
            .ok_or_else(|| "not found".to_string())
    }
}

#[derive(Default)]
struct Recorder {
    commands: Vec<String>,
    fail: Option<String>,
}

impl Runner for Recorder {
    fn run(&mut self, line: &str) -> Result<(), String> {
        self.commands.push(line.to_string());
        if self.fail.as_deref() == Some(line) {
            Err("scripted failure".to_string())
        } else {
            Ok(())
        }
    }
}

fn run_text(
    fs: &mut TestFs,
    text: &str,
    args: &[&str],
) -> Result<(Vec<String>, Vec<String>), String> {
    fs.file("Makefile", text, 1);
    let args = args
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<_>>();
    let (config, goals) = parse_args(args)?;
    let mut runner = Recorder::default();
    let mut output = Vec::new();
    run(&config, &goals, fs, &mut runner, &mut |line| {
        output.push(line.to_string())
    })?;
    Ok((runner.commands, output))
}

#[test]
fn parses_the_required_options() {
    let (config, goals) = parse_args(
        ["-f", "Build.mk", "-n", "-s", "-k", "-j1", "one", "two"]
            .into_iter()
            .map(str::to_string),
    )
    .expect("options");
    assert_eq!(config.file.as_deref(), Some("Build.mk"));
    assert!(config.dry_run);
    assert!(config.silent);
    assert!(config.keep_going);
    assert_eq!(goals, ["one", "two"]);
    assert!(parse_args(["-j2"].into_iter().map(str::to_string)).is_err());
    assert!(parse_args(["-x"].into_iter().map(str::to_string)).is_err());
}

#[test]
fn expands_recursive_and_immediate_variables() {
    let mut fs = TestFs::new();
    let (commands, _) = run_text(
        &mut fs,
        "GREETING = hello\nNAME := world\nall:\n\t@echo $(GREETING) $(NAME)\n",
        &[],
    )
    .expect("build");
    assert_eq!(commands, ["echo hello world"]);
}

#[test]
fn applies_pattern_rules_and_automatic_variables() {
    let mut fs = TestFs::new();
    fs.file("x.c", "source", 10);
    let (commands, _) =
        run_text(&mut fs, "all: x.o\n%.o: %.c\n\tcc -c $< -o $@\n", &[]).expect("build");
    assert_eq!(commands, ["cc -c x.c -o x.o"]);
}

#[test]
fn applies_suffix_rules() {
    let mut fs = TestFs::new();
    fs.file("y.c", "source", 10);
    let (commands, _) =
        run_text(&mut fs, "all: y.o\n.c.o:\n\tcc -c $< -o $@\n", &[]).expect("build");
    assert_eq!(commands, ["cc -c y.c -o y.o"]);
}

#[test]
fn phony_targets_always_run() {
    let mut fs = TestFs::new();
    let (commands, _) =
        run_text(&mut fs, ".PHONY: clean\nclean:\n\trm -f out\n", &["clean"]).expect("build");
    assert_eq!(commands, ["rm -f out"]);
}

#[test]
fn includes_are_loaded_recursively() {
    let mut fs = TestFs::new();
    fs.file("inc.mk", "VALUE = included\n", 1);
    let (commands, _) =
        run_text(&mut fs, "include inc.mk\nall:\n\techo $(VALUE)\n", &[]).expect("build");
    assert_eq!(commands, ["echo included"]);
}

#[test]
fn recipe_failure_propagates() {
    let mut fs = TestFs::new();
    let mut runner = Recorder {
        fail: Some("false".to_string()),
        ..Recorder::default()
    };
    let mut output = Vec::new();
    fs.file("Makefile", "all:\n\tfalse\n", 1);
    let (config, goals) = parse_args(Vec::<String>::new()).expect("args");
    let error = run(&config, &goals, &mut fs, &mut runner, &mut |line| {
        output.push(line.to_string())
    })
    .expect_err("failure");
    assert!(error.contains("recipe for all failed"));
}

#[test]
fn keep_going_runs_later_goals() {
    let mut fs = TestFs::new();
    fs.file("Makefile", "one:\n\tfalse\ntwo:\n\techo two\n", 1);
    let (config, goals) =
        parse_args(["-k", "one", "two"].into_iter().map(str::to_string)).expect("args");
    let mut runner = Recorder {
        fail: Some("false".to_string()),
        ..Recorder::default()
    };
    let mut output = Vec::new();
    let error = run(&config, &goals, &mut fs, &mut runner, &mut |line| {
        output.push(line.to_string())
    })
    .expect_err("failure");
    assert!(error.contains("goals failed"));
    assert_eq!(runner.commands, ["false", "echo two"]);
}

#[test]
fn dry_run_prints_without_running() {
    let mut fs = TestFs::new();
    fs.file("Makefile", "all:\n\ttouch out\n", 1);
    let (config, goals) = parse_args(["-n"].into_iter().map(str::to_string)).expect("args");
    let mut runner = Recorder::default();
    let mut output = Vec::new();
    run(&config, &goals, &mut fs, &mut runner, &mut |line| {
        output.push(line.to_string())
    })
    .expect("dry run");
    assert!(runner.commands.is_empty());
    assert_eq!(output, ["touch out"]);
}

#[test]
fn newer_and_all_prerequisites_are_expanded() {
    let mut fs = TestFs::new();
    fs.file("out", "target", 5);
    fs.file("old", "old", 1);
    fs.file("new", "new", 10);
    let (commands, _) =
        run_text(&mut fs, "out: old new\n\techo $^ newer=$?\n", &["out"]).expect("build");
    assert_eq!(commands, ["echo old new newer=new"]);
}

#[test]
fn silent_suppresses_echo_but_runs() {
    let mut fs = TestFs::new();
    fs.file("Makefile", "all:\n\techo quiet\n", 1);
    let (config, goals) = parse_args(["-s"].into_iter().map(str::to_string)).expect("args");
    let mut runner = Recorder::default();
    let mut output = Vec::new();
    run(&config, &goals, &mut fs, &mut runner, &mut |line| {
        output.push(line.to_string())
    })
    .expect("silent");
    assert_eq!(runner.commands, ["echo quiet"]);
    assert!(output.is_empty());
}
