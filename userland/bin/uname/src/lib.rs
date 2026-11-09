// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! POSIX-style `uname` option parsing and output formatting for RavenBSD.
//!
//! The library is shared by the `uname` utility and by `init`; it is `no_std`, allocation-free and
//! contains no `unsafe` code.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

use core::fmt;

use raven_abi::uname::{SYSNAME, Uname};

/// Selected `uname` fields.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Options {
    /// Print the system name (`-s`).
    pub sysname: bool,
    /// Print the nodename (`-n`).
    pub nodename: bool,
    /// Print the release (`-r`).
    pub release: bool,
    /// Print the version (`-v`).
    pub version: bool,
    /// Print the machine (`-m`).
    pub machine: bool,
    /// Print the processor (`-p`; the machine on RavenBSD).
    pub processor: bool,
    /// Print the operating system (`-o`; `RavenBSD`).
    pub os: bool,
}

/// The command line contained an unknown option or an operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UsageError;

impl fmt::Display for UsageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid uname option")
    }
}

/// Parse `uname` option arguments.
///
/// Options may be combined (`-snrvm`) and `-a` selects `-s -n -r -v -m` (but not `-p` or `-o`).
/// `--` ends option processing; any remaining argument is an error because `uname` has no
/// operands. With no options the default is `-s`.
pub fn parse_args<'a>(args: impl Iterator<Item = &'a str>) -> Result<Options, UsageError> {
    let mut options = Options::default();
    let mut options_done = false;
    for argument in args {
        if options_done {
            return Err(UsageError);
        }
        if argument == "--" {
            options_done = true;
            continue;
        }
        let Some(letters) = argument.strip_prefix('-') else {
            return Err(UsageError);
        };
        if letters.is_empty() {
            return Err(UsageError);
        }
        for letter in letters.chars() {
            match letter {
                'a' => {
                    options.sysname = true;
                    options.nodename = true;
                    options.release = true;
                    options.version = true;
                    options.machine = true;
                }
                's' => options.sysname = true,
                'n' => options.nodename = true,
                'r' => options.release = true,
                'v' => options.version = true,
                'm' => options.machine = true,
                'p' => options.processor = true,
                'o' => options.os = true,
                _ => return Err(UsageError),
            }
        }
    }
    if !(options.sysname
        || options.nodename
        || options.release
        || options.version
        || options.machine
        || options.processor
        || options.os)
    {
        options.sysname = true;
    }
    Ok(options)
}

/// Write the selected fields to `out` in the fixed order `s n r v m p o`.
///
/// Selected fields are separated by one space and a final newline is always written. A field that
/// is not valid UTF-8 is printed as an empty field.
pub fn format(options: &Options, uname: &Uname, out: &mut impl fmt::Write) -> fmt::Result {
    let mut first = true;
    if options.sysname {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.sysname).unwrap_or(""),
        )?;
    }
    if options.nodename {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.nodename).unwrap_or(""),
        )?;
    }
    if options.release {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.release).unwrap_or(""),
        )?;
    }
    if options.version {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.version).unwrap_or(""),
        )?;
    }
    if options.machine {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.machine).unwrap_or(""),
        )?;
    }
    if options.processor {
        field(
            out,
            &mut first,
            Uname::field_str(&uname.machine).unwrap_or(""),
        )?;
    }
    if options.os {
        field(out, &mut first, SYSNAME)?;
    }
    out.write_str("\n")
}

fn field(out: &mut impl fmt::Write, first: &mut bool, value: &str) -> fmt::Result {
    if !*first {
        out.write_str(" ")?;
    }
    *first = false;
    out.write_str(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixed_uname() -> Uname {
        let mut uname = Uname::zeroed();
        Uname::set_field(&mut uname.sysname, "RavenBSD");
        Uname::set_field(&mut uname.nodename, "raven");
        Uname::set_field(&mut uname.release, "0.1");
        Uname::set_field(&mut uname.version, "RavenBSD 0.1 (BASE) #1: build-id");
        Uname::set_field(&mut uname.machine, "x86_64");
        uname
    }

    fn render(options: &Options) -> String {
        let mut out = String::new();
        format(options, &fixed_uname(), &mut out).unwrap();
        out
    }

    fn parsed(args: &[&str]) -> Options {
        parse_args(args.iter().copied()).unwrap()
    }

    #[test]
    fn each_option_selects_only_its_field() {
        assert!(parsed(&["-s"]).sysname);
        assert!(parsed(&["-n"]).nodename);
        assert!(parsed(&["-r"]).release);
        assert!(parsed(&["-v"]).version);
        assert!(parsed(&["-m"]).machine);
        assert!(parsed(&["-p"]).processor);
        assert!(parsed(&["-o"]).os);
    }

    #[test]
    fn empty_arguments_default_to_sysname() {
        let options = parse_args(core::iter::empty()).unwrap();
        assert!(options.sysname);
        assert!(!options.nodename);
        assert_eq!(render(&options), "RavenBSD\n");
    }

    #[test]
    fn double_dash_with_no_options_defaults_to_sysname() {
        let options = parsed(&["--"]);
        assert!(options.sysname);
    }

    #[test]
    fn options_after_double_dash_are_operands() {
        assert!(parse_args(["--", "-a"].into_iter()).is_err());
        assert!(parse_args(["-a", "--", "x"].into_iter()).is_err());
    }

    #[test]
    fn all_sets_first_five_only() {
        let options = parsed(&["-a"]);
        assert!(options.sysname);
        assert!(options.nodename);
        assert!(options.release);
        assert!(options.version);
        assert!(options.machine);
        assert!(!options.processor);
        assert!(!options.os);
    }

    #[test]
    fn combined_letters_match_all() {
        assert_eq!(parsed(&["-a"]), parsed(&["-snrvm"]));
        assert_eq!(parsed(&["-sn", "-rvm"]), parsed(&["-a"]));
    }

    #[test]
    fn repeated_letters_are_idempotent() {
        assert_eq!(parsed(&["-ss", "-nn"]), parsed(&["-sn"]));
    }

    #[test]
    fn p_and_o_combine_with_all() {
        let options = parsed(&["-a", "-p", "-o"]);
        assert!(options.sysname);
        assert!(options.processor);
        assert!(options.os);
    }

    #[test]
    fn unknown_options_are_errors() {
        for argument in ["-x", "--help", "-", "", "-snz", "-abc", "--kvm"] {
            assert!(parse_args([argument].into_iter()).is_err(), "{argument}");
        }
    }

    #[test]
    fn bare_operand_is_an_error() {
        assert!(parse_args(["foo"].into_iter()).is_err());
    }

    #[test]
    fn format_default_is_sysname_only() {
        assert_eq!(render(&parsed(&[])), "RavenBSD\n");
    }

    #[test]
    fn format_all_matches_contract() {
        assert_eq!(
            render(&parsed(&["-a"])),
            "RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: build-id x86_64\n"
        );
    }

    #[test]
    fn format_all_with_p_and_o_appends_after_machine() {
        assert_eq!(
            render(&parsed(&["-a", "-p", "-o"])),
            "RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: build-id x86_64 x86_64 RavenBSD\n"
        );
    }

    #[test]
    fn format_p_prints_machine() {
        assert_eq!(render(&parsed(&["-p"])), "x86_64\n");
    }

    #[test]
    fn format_o_prints_operating_system() {
        assert_eq!(render(&parsed(&["-o"])), "RavenBSD\n");
    }

    #[test]
    fn format_each_single_field() {
        assert_eq!(render(&parsed(&["-s"])), "RavenBSD\n");
        assert_eq!(render(&parsed(&["-n"])), "raven\n");
        assert_eq!(render(&parsed(&["-r"])), "0.1\n");
        assert_eq!(
            render(&parsed(&["-v"])),
            "RavenBSD 0.1 (BASE) #1: build-id\n"
        );
        assert_eq!(render(&parsed(&["-m"])), "x86_64\n");
    }

    #[test]
    fn field_order_is_fixed_regardless_of_option_order() {
        assert_eq!(
            render(&parsed(&["-o", "-p", "-m"])),
            "x86_64 x86_64 RavenBSD\n"
        );
    }

    #[test]
    fn options_with_every_flag_set_render_in_order() {
        let options = parsed(&["-snrvmpo"]);
        assert_eq!(
            render(&options),
            "RavenBSD raven 0.1 RavenBSD 0.1 (BASE) #1: build-id x86_64 x86_64 RavenBSD\n"
        );
    }

    #[test]
    fn no_selected_fields_prints_only_a_newline() {
        assert_eq!(render(&Options::default()), "\n");
    }

    #[test]
    fn empty_fields_are_rendered_as_empty_strings() {
        let uname = Uname::zeroed();
        let options = Options {
            sysname: true,
            nodename: true,
            ..Options::default()
        };
        let mut out = String::new();
        format(&options, &uname, &mut out).unwrap();
        assert_eq!(out, " \n");
    }

    #[test]
    fn truncated_fields_are_rendered_without_a_nul() {
        let mut uname = Uname::zeroed();
        let long = "x".repeat(64);
        Uname::set_field(&mut uname.sysname, &long);
        let mut out = String::new();
        format(
            &Options {
                sysname: true,
                ..Options::default()
            },
            &uname,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, format!("{long}\n"));
    }

    #[test]
    fn invalid_utf8_field_is_empty() {
        let mut uname = Uname::zeroed();
        uname.sysname[0] = 0xff;
        let mut out = String::new();
        format(
            &Options {
                sysname: true,
                ..Options::default()
            },
            &uname,
            &mut out,
        )
        .unwrap();
        assert_eq!(out, "\n");
    }

    #[test]
    fn format_never_panics_on_all_fields() {
        let uname = fixed_uname();
        let options = Options {
            sysname: true,
            nodename: true,
            release: true,
            version: true,
            machine: true,
            processor: true,
            os: true,
        };
        let mut out = String::new();
        assert!(format(&options, &uname, &mut out).is_ok());
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
        assert!(format(&Options::default(), &fixed_uname(), &mut out).is_err());
    }

    #[test]
    fn usage_error_has_a_description() {
        assert_eq!(format!("{}", UsageError), "invalid uname option");
    }
}
