// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the line discipline.

use alloc::vec::Vec;

use super::*;

fn feed(discipline: &mut Discipline, input: &[u8]) -> Output {
    let mut out = Output::default();
    discipline.feed(input, &mut out);
    out
}

fn lines(output: &Output) -> Vec<&[u8]> {
    output.lines.iter().map(Vec::as_slice).collect()
}

#[test]
fn new_discipline_is_canonical_and_echoes() {
    let discipline = Discipline::new();
    assert_eq!(discipline.settings().mode, Mode::Canonical);
    assert!(discipline.settings().echo);
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn newline_completes_a_line() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"ls\n");
    assert_eq!(lines(&output), vec![&b"ls\n"[..]]);
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn carriage_return_completes_a_line() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"ls\r");
    assert_eq!(lines(&output), vec![&b"ls\n"[..]]);
}

#[test]
fn the_terminator_is_normalised_to_newline() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"abc\r");
    assert!(output.lines[0].ends_with(&[NEWLINE]));
    assert_eq!(output.lines[0].len(), 4);
}

#[test]
fn an_empty_line_is_delivered_as_a_newline() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\n");
    assert_eq!(lines(&output), vec![&b"\n"[..]]);
}

#[test]
fn carriage_return_newline_completes_two_lines() {
    let mut discipline = Discipline::new();
    let first = feed(&mut discipline, b"a\r");
    let second = feed(&mut discipline, b"\n");
    assert_eq!(lines(&first), vec![&b"a\n"[..]]);
    assert_eq!(lines(&second), vec![&b"\n"[..]]);
}

#[test]
fn input_is_echoed_in_order() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"abc\n");
    assert_eq!(output.echo, b"abc\r\n");
}

#[test]
fn backspace_erases_and_echoes() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"ab");
    let output = feed(&mut discipline, b"\x7f");
    assert_eq!(output.echo, b"\x08 \x08");
    assert_eq!(discipline.pending(), 1);
}

#[test]
fn backspace_accepts_both_erase_bytes() {
    for erase in [0x7f, 0x08] {
        let mut discipline = Discipline::new();
        feed(&mut discipline, b"ab");
        let output = feed(&mut discipline, &[erase]);
        assert_eq!(output.echo, b"\x08 \x08", "erase {erase:#x}");
        assert_eq!(discipline.pending(), 1);
    }
}

#[test]
fn backspace_on_an_empty_line_does_nothing() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x7f");
    assert!(output.echo.is_empty());
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn a_line_edited_with_backspace_is_delivered_edited() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abc");
    feed(&mut discipline, b"\x7fd");
    let output = feed(&mut discipline, b"\n");
    assert_eq!(lines(&output), vec![&b"abd\n"[..]]);
}

#[test]
fn kill_line_erases_the_whole_line() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abc");
    let output = feed(&mut discipline, b"\x15");
    assert_eq!(output.echo, b"\x08 \x08\x08 \x08\x08 \x08");
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn kill_line_on_an_empty_line_does_nothing() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x15");
    assert!(output.echo.is_empty());
}

#[test]
fn kill_line_then_input_starts_a_new_line() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"junk");
    feed(&mut discipline, b"\x15");
    feed(&mut discipline, b"ok");
    let output = feed(&mut discipline, b"\n");
    assert_eq!(lines(&output), vec![&b"ok\n"[..]]);
}

#[test]
fn interrupt_discards_the_line() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"half");
    let output = feed(&mut discipline, b"\x03");
    assert_eq!(discipline.pending(), 0);
    assert_eq!(output.events, vec![Event::Interrupt]);
    assert!(output.lines.is_empty());
}

#[test]
fn interrupt_echoes_caret_c_and_a_newline() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x03");
    assert_eq!(output.echo, b"^C\r\n");
}

#[test]
fn interrupt_after_a_partial_line_raises_one_event() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"ab");
    let output = feed(&mut discipline, b"\x03");
    assert_eq!(output.events.len(), 1);
    let next = feed(&mut discipline, b"x\n");
    assert_eq!(lines(&next), vec![&b"x\n"[..]]);
}

#[test]
fn eof_on_an_empty_line_raises_end_of_file() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x04");
    assert_eq!(output.events, vec![Event::EndOfFile]);
    assert!(output.lines.is_empty());
}

#[test]
fn eof_on_a_non_empty_line_completes_it() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"bye");
    let output = feed(&mut discipline, b"\x04");
    assert_eq!(lines(&output), vec![&b"bye\n"[..]]);
    assert!(output.events.is_empty());
}

#[test]
fn eof_echoes_nothing() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x04");
    assert!(output.echo.is_empty());
}

#[test]
fn ctrl_l_is_passed_through() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\x0c");
    assert_eq!(output.echo, b"\x0c");
    assert_eq!(discipline.pending(), 1);
}

#[test]
fn the_line_limit_rings_the_bell_and_ignores_input() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, &[b'a'; MAX_LINE + 3]);
    assert_eq!(discipline.pending(), MAX_LINE);
    assert_eq!(output.echo.len(), MAX_LINE + 3);
    assert_eq!(&output.echo[MAX_LINE..], &[BELL, BELL, BELL]);
}

#[test]
fn the_line_limit_accepts_exactly_max_bytes() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, &[b'a'; MAX_LINE]);
    let output = feed(&mut discipline, b"\n");
    assert_eq!(output.lines[0].len(), MAX_LINE + 1);
}

#[test]
fn echo_can_be_disabled() {
    let mut discipline = Discipline::new();
    assert_eq!(
        discipline.set_settings(Settings {
            mode: Mode::Canonical,
            echo: false,
        }),
        0
    );
    let output = feed(&mut discipline, b"abc\n");
    assert!(output.echo.is_empty());
    assert_eq!(lines(&output), vec![&b"abc\n"[..]]);
}

#[test]
fn without_echo_editing_still_happens() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Canonical,
        echo: false,
    });
    feed(&mut discipline, b"ab");
    feed(&mut discipline, b"\x7f");
    let output = feed(&mut discipline, b"\n");
    assert_eq!(lines(&output), vec![&b"a\n"[..]]);
    assert!(output.echo.is_empty());
}

#[test]
fn raw_mode_passes_bytes_through() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: true,
    });
    let output = feed(&mut discipline, b"ab\r\n");
    assert_eq!(output.data, b"ab\r\n");
    assert!(output.lines.is_empty());
}

#[test]
fn raw_mode_does_not_echo() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: true,
    });
    let output = feed(&mut discipline, b"x");
    assert!(output.echo.is_empty());
}

#[test]
fn raw_mode_does_not_edit() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    let output = feed(&mut discipline, b"a\x7f\x15\x03");
    assert_eq!(output.data, b"a\x7f\x15\x03");
    assert!(output.events.is_empty());
}

#[test]
fn raw_mode_has_no_events() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    let output = feed(&mut discipline, b"\x03\x04");
    assert!(output.events.is_empty());
    assert_eq!(output.data, b"\x03\x04");
}

#[test]
fn a_mode_switch_discards_the_pending_line() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abc");
    let discarded = discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    assert_eq!(discarded, 3);
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn the_same_mode_switch_does_not_discard() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abc");
    let discarded = discipline.set_settings(Settings {
        mode: Mode::Canonical,
        echo: false,
    });
    assert_eq!(discarded, 0);
    assert_eq!(discipline.pending(), 3);
}

#[test]
fn flush_reports_the_discarded_bytes() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abcd");
    assert_eq!(discipline.flush(), 4);
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn flush_on_an_empty_line_reports_zero() {
    let mut discipline = Discipline::new();
    assert_eq!(discipline.flush(), 0);
}

#[test]
fn several_lines_in_one_chunk_are_delivered_in_order() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"one\ntwo\nthree\n");
    assert_eq!(
        lines(&output),
        vec![&b"one\n"[..], &b"two\n"[..], &b"three\n"[..]]
    );
}

#[test]
fn a_partial_line_survives_between_chunks() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"par");
    let output = feed(&mut discipline, b"tial\n");
    assert_eq!(lines(&output), vec![&b"partial\n"[..]]);
}

#[test]
fn a_tab_is_inserted_verbatim() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"\t\n");
    assert_eq!(lines(&output), vec![&b"\t\n"[..]]);
    assert_eq!(output.echo, b"\t\r\n");
}

#[test]
fn high_bytes_are_inserted_verbatim() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, &[0x80, 0xff, b'\n']);
    assert_eq!(lines(&output), vec![&[0x80, 0xff, b'\n'][..]]);
}

#[test]
fn the_bell_byte_is_inserted_like_any_other() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, &[BELL, b'\n']);
    assert_eq!(lines(&output), vec![&[BELL, b'\n'][..]]);
}

#[test]
fn settings_round_trip() {
    let mut discipline = Discipline::new();
    let raw = Settings {
        mode: Mode::Raw,
        echo: false,
    };
    discipline.set_settings(raw);
    assert_eq!(discipline.settings(), raw);
    discipline.set_settings(Settings::default());
    assert_eq!(discipline.settings(), Settings::default());
}

#[test]
fn an_empty_chunk_produces_nothing() {
    let mut discipline = Discipline::new();
    let output = feed(&mut discipline, b"");
    assert_eq!(output, Output::default());
}

#[test]
fn output_starts_empty() {
    let output = Output::default();
    assert!(output.echo.is_empty());
    assert!(output.lines.is_empty());
    assert!(output.data.is_empty());
    assert!(output.events.is_empty());
}

#[test]
fn eof_then_more_input_still_completes_lines() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"\x04");
    let output = feed(&mut discipline, b"again\n");
    assert_eq!(lines(&output), vec![&b"again\n"[..]]);
}

#[test]
fn kill_then_backspace_does_nothing() {
    let mut discipline = Discipline::new();
    feed(&mut discipline, b"abc");
    feed(&mut discipline, b"\x15");
    let output = feed(&mut discipline, b"\x7f");
    assert!(output.echo.is_empty());
    assert_eq!(discipline.pending(), 0);
}

#[test]
fn raw_then_canonical_round_trip() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });
    let raw = feed(&mut discipline, b"raw");
    assert_eq!(raw.data, b"raw");
    discipline.set_settings(Settings::default());
    let canonical = feed(&mut discipline, b"line\n");
    assert_eq!(lines(&canonical), vec![&b"line\n"[..]]);
    assert_eq!(canonical.echo, b"line\r\n");
}

#[test]
fn many_lines_in_one_chunk_all_arrive() {
    let mut discipline = Discipline::new();
    let input: Vec<u8> = (0..50u8)
        .flat_map(|index| [b'a' + index % 26, b'\n'])
        .collect();
    let output = feed(&mut discipline, &input);
    assert_eq!(output.lines.len(), 50);
    assert_eq!(output.lines[49].len(), 2);
}

#[test]
fn interrupt_does_not_echo_when_echo_is_off() {
    let mut discipline = Discipline::new();
    discipline.set_settings(Settings {
        mode: Mode::Canonical,
        echo: false,
    });
    let output = feed(&mut discipline, b"\x03");
    assert!(output.echo.is_empty());
    assert_eq!(output.events, vec![Event::Interrupt]);
}

#[test]
fn the_default_impl_matches_new() {
    assert_eq!(
        Discipline::default().settings(),
        Discipline::new().settings()
    );
    assert_eq!(Discipline::default().pending(), 0);
}
