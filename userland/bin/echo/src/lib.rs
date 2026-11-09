// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX-style `echo` output formatting.
//!
//! No backslash escape processing is performed. `-n` is recognised only as the first argument;
//! every other argument, including `--`, is printed literally.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use core::fmt;

/// Write `args` joined by single spaces to `out`.
///
/// A newline is appended unless the first argument is exactly `-n`. With no arguments only the
/// newline is written (unless it is suppressed, in which case nothing is written).
pub fn write_echo<'a>(
    args: impl Iterator<Item = &'a str>,
    out: &mut impl fmt::Write,
) -> fmt::Result {
    let mut args = args.peekable();
    let mut newline = true;
    if args.peek().copied() == Some("-n") {
        args.next();
        newline = false;
    }
    let mut first = true;
    for argument in args {
        if !first {
            out.write_str(" ")?;
        }
        first = false;
        out.write_str(argument)?;
    }
    if newline {
        out.write_str("\n")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(args: &[&str]) -> String {
        let mut out = String::new();
        write_echo(args.iter().copied(), &mut out).unwrap();
        out
    }

    #[test]
    fn no_arguments_prints_a_newline() {
        assert_eq!(render(&[]), "\n");
    }

    #[test]
    fn joins_arguments_with_single_spaces() {
        assert_eq!(render(&["one", "two", "three"]), "one two three\n");
    }

    #[test]
    fn empty_arguments_are_preserved() {
        assert_eq!(render(&["", "x", ""]), " x \n");
    }

    #[test]
    fn dash_n_suppresses_the_newline_only_when_first() {
        assert_eq!(render(&["-n"]), "");
        assert_eq!(render(&["-n", "a", "b"]), "a b");
        assert_eq!(render(&["a", "-n"]), "a -n\n");
    }

    #[test]
    fn double_dash_is_printed_literally() {
        assert_eq!(render(&["--", "a"]), "-- a\n");
    }

    #[test]
    fn other_options_are_literal() {
        assert_eq!(render(&["-e", "a\\nb"]), "-e a\\nb\n");
        assert_eq!(render(&["-E", "-e"]), "-E -e\n");
    }

    #[test]
    fn dash_n_after_dash_n_is_literal() {
        assert_eq!(render(&["-n", "-n"]), "-n");
    }

    #[test]
    fn write_errors_are_propagated() {
        struct Failing;
        impl fmt::Write for Failing {
            fn write_str(&mut self, _text: &str) -> fmt::Result {
                Err(fmt::Error)
            }
        }
        let mut out = Failing;
        assert!(write_echo(["a"].into_iter(), &mut out).is_err());
    }
}
