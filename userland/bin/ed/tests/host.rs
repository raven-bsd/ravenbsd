// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host filesystem tests for ed file commands.

use std::fs;
use std::path::PathBuf;

use ed::{Editor, Error, Io, parse_args};

struct StdIo;

impl Io for StdIo {
    fn read(&mut self, path: &str) -> Result<String, Error> {
        fs::read_to_string(path).map_err(|_| Error::Io)
    }
    fn write(&mut self, path: &str, data: &str) -> Result<(), Error> {
        fs::write(path, data).map_err(|_| Error::Io)
    }
}

fn temp(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("raven-ed-{label}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn edits_a_real_file_and_writes_it_back() {
    let root = temp("file");
    let input = root.join("in.txt");
    let output = root.join("out.txt");
    fs::write(&input, "one\ntwo\n").unwrap();
    let mut editor = Editor::new();
    let mut io = StdIo;
    let script = format!(
        "e {}\n1s/one/ONE/\nw {}\nq\n",
        input.display(),
        output.display()
    );
    editor.execute(&script, &mut io).unwrap();
    assert_eq!(fs::read_to_string(&output).unwrap(), "ONE\ntwo\n");
    let _ = fs::remove_dir_all(root);
}

#[test]
fn parses_silent_and_file() {
    let options = parse_args(["-s".into(), "file".into()]).unwrap();
    assert!(options.silent);
    assert_eq!(options.file.as_deref(), Some("file"));
}
