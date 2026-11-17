// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX behaviour cases for `echo`.

use core::fmt;

use raven_echo::write_echo;

fn render(args: &[&str]) -> String {
    let mut out = String::new();
    write_echo(args.iter().copied(), &mut out).expect("echo writes to String");
    out
}

#[test]
fn dash_n_is_recognised_only_as_the_first_operand() {
    assert_eq!(render(&["-n"]), "");
    assert_eq!(render(&["-n", "a", "b"]), "a b");
    assert_eq!(render(&["a", "-n"]), "a -n\n");
    assert_eq!(render(&["-n", "-n"]), "-n");
}

#[test]
fn empty_operands_and_double_dash_are_preserved() {
    assert_eq!(render(&[]), "\n");
    assert_eq!(render(&[""]), "\n");
    assert_eq!(render(&["", "x", ""]), " x \n");
    assert_eq!(render(&["--", "-e", "a b"]), "-- -e a b\n");
}

#[test]
fn backslashes_are_literal_in_the_implementation_defined_posix_form() {
    assert_eq!(
        render(&["a\\nb", "\\t", "\\\\", "\\0123"]),
        "a\\nb \\t \\\\ \\0123\n"
    );
}

#[test]
fn status_is_zero_on_success_and_one_on_a_write_error() {
    assert!(write_echo(["ok"].into_iter(), &mut String::new()).is_ok());

    struct Failing;
    impl fmt::Write for Failing {
        fn write_str(&mut self, _text: &str) -> fmt::Result {
            Err(fmt::Error)
        }
    }
    assert!(write_echo(["ok"].into_iter(), &mut Failing).is_err());
}
