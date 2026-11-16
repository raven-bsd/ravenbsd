// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host end-to-end checks for the bounded m4 processor.

use std::collections::BTreeMap;

use raven_m4::{FileProvider, InitAction, Options, Processor, run};

#[derive(Default)]
struct Files {
    files: BTreeMap<String, String>,
}

impl FileProvider for Files {
    fn read_file(&mut self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }
}

#[test]
fn definitions_persist_across_sources() {
    let mut files = Files::default();
    let mut processor = Processor::new(&Options::default(), &mut files);
    processor
        .process("define(`name',`RavenBSD')dnl\n")
        .expect("definition source should expand");
    processor
        .process("hello name\n")
        .expect("use source should expand");
    assert_eq!(processor.finish(), "hello RavenBSD\n");
}

#[test]
fn include_defines_a_macro_for_later_input() {
    let mut files = Files::default();
    let options = Options {
        actions: Vec::new(),
        include_dirs: vec!["defs".to_string()],
    };
    files.files.insert(
        "defs/version.m4".to_string(),
        "define(`version',`22')dnl\n".to_string(),
    );
    let output = run("include(`version.m4')dnl\nversion\n", &options, &mut files)
        .expect("the included definitions should expand");
    assert_eq!(output, "22\n");
}

#[test]
fn command_line_actions_apply_before_input() {
    let options = Options {
        actions: vec![
            InitAction::Define("tool".to_string(), "m4".to_string()),
            InitAction::Undefine("gone".to_string()),
        ],
        include_dirs: Vec::new(),
    };
    let mut files = Files::default();
    let output = run("tool gone\n", &options, &mut files).expect("the input should expand");
    assert_eq!(output, "m4 gone\n");
}
