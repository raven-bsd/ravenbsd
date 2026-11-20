// SPDX-License-Identifier: MIT
// Copyright (c) 2024 reuben olinsky

//! Host tests for accepting history autosuggestions in the Vt line editor.

use super::*;

fn new_editor() -> LineEditor<'static> {
    let mut editor = LineEditor::new("$ ", String::new(), Style::Vt);
    editor.buffer.set(String::from("echo "), 5);
    editor
}

#[test]
fn right_accepts_the_whole_hint() {
    let history = [String::from("echo hello")];
    let mut editor = new_editor();
    editor.update_hint(history.as_slice(), true);
    let mut kills = KillBuffer::default();
    let mut out = Vec::new();
    let line = editor
        .perform(
            &InputFunction::ForwardChar,
            &mut kills,
            history.as_slice(),
            &mut out,
            80,
        )
        .unwrap();
    assert_eq!(line, None);
    assert_eq!(editor.buffer.text(), "echo hello");
    assert_eq!(editor.buffer.cursor(), "echo hello".len());
}

#[test]
fn end_accepts_and_a_plain_key_keeps_the_typed_text() {
    let history = [String::from("echo hello")];
    let mut editor = new_editor();
    editor.update_hint(history.as_slice(), true);
    let mut kills = KillBuffer::default();
    let mut out = Vec::new();
    editor
        .perform(
            &InputFunction::EndOfLine,
            &mut kills,
            history.as_slice(),
            &mut out,
            80,
        )
        .unwrap();
    assert_eq!(editor.buffer.text(), "echo hello");

    let mut editor = new_editor();
    editor.update_hint(history.as_slice(), true);
    editor.update_hint(history.as_slice(), false);
    assert_eq!(editor.hint, None);
    editor
        .perform(
            &InputFunction::ForwardChar,
            &mut kills,
            history.as_slice(),
            &mut out,
            80,
        )
        .unwrap();
    assert_eq!(editor.buffer.text(), "echo ");
}
