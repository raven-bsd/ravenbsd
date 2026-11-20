// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ed command model.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};

use super::{Editor, Error, Io};

#[derive(Default)]
struct MemoryIo {
    files: BTreeMap<String, String>,
}

impl Io for MemoryIo {
    fn read(&mut self, path: &str) -> Result<String, Error> {
        self.files.get(path).cloned().ok_or(Error::Io)
    }
    fn write(&mut self, path: &str, data: &str) -> Result<(), Error> {
        self.files.insert(path.to_string(), data.to_string());
        Ok(())
    }
}

fn run(script: &str) -> (Editor, String) {
    let mut editor = Editor::new();
    let mut io = MemoryIo::default();
    editor.execute(script, &mut io).unwrap();
    let output = editor.take_output();
    (editor, output)
}

#[test]
fn append_print_number_and_addresses() {
    let (editor, output) = run("a\none\ntwo\nthree\n.\n1,$p\n1,2n\n$=\n");
    assert_eq!(output, "one\ntwo\nthree\n1\tone\n2\ttwo\n3\n");
    assert_eq!(editor.lines(), ["one", "two", "three"]);
}

#[test]
fn insert_change_delete_and_list() {
    let (_, output) = run("a\na\nb\nc\n.\n1i\nzero\n.\n2,3c\nx\ny\n.\n$d\n1,$l\n");
    assert_eq!(output, "zero$\nx$\ny$\n");
}

#[test]
fn substitution_global_and_numeric_flags() {
    let (_, output) = run("a\nfoo foo\nbar\n.\n1,$s/o/0/g\n1,$p\n2s/a/A/\n2p\n");
    assert_eq!(output, "f00 f00\nbar\nbAr\n");
}

#[test]
fn global_and_inverse_global() {
    let (editor, output) = run("a\nkeep\nremove\nkeep2\n.\ng/remove/d\n1,$p\n");
    assert_eq!(output, "keep\nkeep2\n");
    assert_eq!(editor.lines(), ["keep", "keep2"]);
    let (_, output) = run("a\none\ntwo\nthree\n.\nv/o/d\n1,$p\n");
    assert_eq!(output, "one\ntwo\n");
}

#[test]
fn move_copy_join_marks_and_undo() {
    let (_, output) = run("a\na\nb\nc\n.\n3m1\n1,$p\nu\n1,2j\n1p\n3ka\n'a=\n");
    assert_eq!(output, "a\nc\nb\na b\n3\n");
}

#[test]
fn read_write_edit_and_quit() {
    let mut editor = Editor::new();
    let mut io = MemoryIo::default();
    io.files.insert("in".into(), "one\ntwo\n".into());
    editor
        .execute("e in\n1,$p\n1s/one/ONE/\nw out\nq\n1,$p\n", &mut io)
        .unwrap();
    assert_eq!(editor.take_output(), "one\ntwo\n");
    assert_eq!(io.files.get("out").map(String::as_str), Some("ONE\ntwo\n"));
    assert_eq!(editor.lines(), ["ONE", "two"]);
    editor.execute("e in\n1p\n", &mut io).unwrap();
    assert_eq!(editor.take_output(), "one\n");
}

#[test]
fn feed_line_processes_blocks_incrementally_and_quits() {
    let mut editor = Editor::new();
    let mut io = MemoryIo::default();
    for line in [
        "a", "one", "two", ".", "2i", "middle", ".", "1,2c", "changed", ".", "1p", "q",
    ] {
        editor.feed_line(line, &mut io).unwrap();
    }
    assert_eq!(editor.lines(), ["changed", "two"]);
    assert_eq!(editor.take_output(), "changed\n");
    assert!(editor.quit());
    assert!(!editor.input_pending());
}

#[test]
fn feed_line_keeps_an_unterminated_block_open() {
    let mut editor = Editor::new();
    let mut io = MemoryIo::default();
    editor.feed_line("a", &mut io).unwrap();
    editor.feed_line("line", &mut io).unwrap();
    assert!(editor.input_pending());
    assert!(editor.lines().is_empty());
    editor.feed_line(".", &mut io).unwrap();
    assert!(!editor.input_pending());
    assert_eq!(editor.lines(), ["line"]);
}

#[test]
fn invalid_commands_and_shell_escape_are_rejected() {
    let mut editor = Editor::new();
    let mut io = MemoryIo::default();
    assert_eq!(editor.execute("!ls\n", &mut io), Err(Error::Unsupported));
    assert_eq!(editor.execute("p\n", &mut io), Err(Error::NoCurrent));
    assert_eq!(
        editor.execute("a\nx\n", &mut io),
        Err(Error::MissingTerminator)
    );
}
