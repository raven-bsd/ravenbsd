// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `getopt`/`getopt_long`: the OpenBSD golden scenarios and the D-3 permutation rules.

use super::*;

struct Src;

impl ArgSource for Src {
    type Arg = &'static [u8];

    fn text(&self, a: Self::Arg) -> &[u8] {
        a
    }
}

#[derive(Default)]
struct FakeDiag {
    lines: Vec<Vec<u8>>,
}

impl Diag for FakeDiag {
    fn warnx(&mut self, parts: &[&[u8]]) {
        let mut line = Vec::new();
        for part in parts {
            line.extend_from_slice(part);
        }
        self.lines.push(line);
    }
}

fn leak(text: &str) -> &'static [u8] {
    Box::leak(text.as_bytes().to_vec().into_boxed_slice())
}

fn longopts() -> Vec<LongOpt<'static>> {
    vec![
        LongOpt {
            name: b"force",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 0,
        },
        LongOpt {
            name: b"fast",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: i32::from(b'1'),
        },
        LongOpt {
            name: b"best",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: i32::from(b'9'),
        },
        LongOpt {
            name: b"input",
            has_arg: REQUIRED_ARGUMENT,
            flag_is_null: true,
            val: i32::from(b'i'),
        },
        LongOpt {
            name: b"illiterate",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 0,
        },
        LongOpt {
            name: b"drinking",
            has_arg: REQUIRED_ARGUMENT,
            flag_is_null: false,
            val: 42,
        },
        LongOpt {
            name: b"help",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: i32::from(b'h'),
        },
    ]
}

fn optarg_text(step: &Step, argv: &[&'static [u8]]) -> Option<Vec<u8>> {
    match step.optarg {
        OptArg::None => None,
        OptArg::At { arg_index, offset } => argv
            .get(arg_index)
            .and_then(|arg| arg.get(offset..))
            .map(|text| text.to_vec()),
    }
}

fn run_case(args: &[&str], only: bool, posix: bool) -> String {
    let original: Vec<&'static [u8]> = core::iter::once(leak("prog"))
        .chain(args.iter().map(|arg| leak(arg)))
        .collect();
    let mut argv = original.clone();
    let opts = longopts();
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let mut goggles = 0i32;
    let mut output = String::new();
    output.push_str(if only {
        "getopt_long_only"
    } else {
        "getopt_long"
    });
    if posix {
        output.push_str(" (POSIXLY_CORRECT)");
    }
    output.push_str(": ");
    for arg in args {
        output.push_str(arg);
        output.push(' ');
    }
    output.push('\n');
    loop {
        let step = state.getopt_long(
            &mut argv,
            &Src,
            b"19bf:i:hW;-",
            &opts,
            only,
            posix,
            &mut diag,
        );
        if step.code == -1 {
            break;
        }
        if let Some(index) = step.flag_to_set
            && let Some(option) = opts.get(index)
        {
            goggles = option.val;
        }
        match step.code {
            0 | 49 | 57 | 104 | 98 | 45 => {
                if let Some(index) = step.long_index {
                    let name = opts[index].name;
                    if goggles == 42 {
                        let arg = optarg_text(&step, &argv).unwrap_or_default();
                        output.push_str(&format!(
                            "option {}, arg {}\n",
                            String::from_utf8_lossy(name),
                            String::from_utf8_lossy(&arg)
                        ));
                    } else {
                        output.push_str(&format!("option {}\n", String::from_utf8_lossy(name)));
                    }
                } else {
                    output.push_str(&format!("option {}\n", step.code as u8 as char));
                }
            }
            102 | 105 => {
                let arg = optarg_text(&step, &argv).unwrap_or_default();
                if let Some(index) = step.long_index {
                    output.push_str(&format!(
                        "option {}, arg {}\n",
                        String::from_utf8_lossy(opts[index].name),
                        String::from_utf8_lossy(&arg)
                    ));
                } else {
                    output.push_str(&format!(
                        "option {}, arg {}\n",
                        step.code as u8 as char,
                        String::from_utf8_lossy(&arg)
                    ));
                }
            }
            63 => {}
            _ => output.push_str("unexpected return value\n"),
        }
    }
    let remaining = state.optind().max(0) as usize;
    if remaining < argv.len() {
        output.push_str("remaining ARGV: ");
        for arg in &argv[remaining..] {
            output.push_str(&String::from_utf8_lossy(arg));
            output.push(' ');
        }
        output.push('\n');
    }
    output.push('\n');
    output
}

#[test]
fn golden_getopt_long_test_ok() {
    let cases: &[(&[&str], bool, bool, &str)] = &[
        (
            &["myfile", "--force", "-f", "infile", "-9"],
            false,
            false,
            "getopt_long: myfile --force -f infile -9 \noption force\noption f, arg infile\noption 9\nremaining ARGV: myfile \n\n",
        ),
        (
            &[
                "onefile",
                "twofile",
                "--best",
                "-Williterate",
                "-i",
                "foo.in",
                "threefile",
            ],
            false,
            false,
            "getopt_long: onefile twofile --best -Williterate -i foo.in threefile \noption best\noption illiterate\noption i, arg foo.in\nremaining ARGV: onefile twofile threefile \n\n",
        ),
        (
            &["-1bfast", "-"],
            false,
            false,
            "getopt_long: -1bfast - \noption 1\noption b\noption f, arg ast\noption -\n\n",
        ),
        (
            &["--fast", "--drinking=guiness", "-i", "foo.in", "somefile"],
            false,
            false,
            "getopt_long: --fast --drinking=guiness -i foo.in somefile \noption fast\noption drinking, arg guiness\noption i, arg foo.in\nremaining ARGV: somefile \n\n",
        ),
        (
            &["myfile", "--force", "-f", "infile", "-9"],
            false,
            true,
            "getopt_long (POSIXLY_CORRECT): myfile --force -f infile -9 \nremaining ARGV: myfile --force -f infile -9 \n\n",
        ),
        (
            &[
                "onefile",
                "twofile",
                "--best",
                "-Williterate",
                "-i",
                "foo.in",
                "threefile",
            ],
            false,
            true,
            "getopt_long (POSIXLY_CORRECT): onefile twofile --best -Williterate -i foo.in threefile \nremaining ARGV: onefile twofile --best -Williterate -i foo.in threefile \n\n",
        ),
        (
            &["-1bfast", "-"],
            false,
            true,
            "getopt_long (POSIXLY_CORRECT): -1bfast - \noption 1\noption b\noption f, arg ast\noption -\n\n",
        ),
        (
            &["--fast", "--drinking=guiness", "-i", "foo.in", "somefile"],
            false,
            true,
            "getopt_long (POSIXLY_CORRECT): --fast --drinking=guiness -i foo.in somefile \noption fast\noption drinking, arg guiness\noption i, arg foo.in\nremaining ARGV: somefile \n\n",
        ),
        (
            &["myfile", "-force", "-f", "infile", "-9"],
            true,
            false,
            "getopt_long_only: myfile -force -f infile -9 \noption force\noption f, arg infile\noption 9\nremaining ARGV: myfile \n\n",
        ),
        (
            &[
                "onefile",
                "twofile",
                "-best",
                "-Williterate",
                "-i",
                "foo.in",
                "threefile",
            ],
            true,
            false,
            "getopt_long_only: onefile twofile -best -Williterate -i foo.in threefile \noption best\noption illiterate\noption i, arg foo.in\nremaining ARGV: onefile twofile threefile \n\n",
        ),
        (
            &["-1bfast", "-"],
            true,
            false,
            "getopt_long_only: -1bfast - \noption 1\noption b\noption fast\noption -\n\n",
        ),
        (
            &["--fast", "-drinking=guiness", "-i", "foo.in", "somefile"],
            true,
            false,
            "getopt_long_only: --fast -drinking=guiness -i foo.in somefile \noption fast\noption drinking, arg guiness\noption i, arg foo.in\nremaining ARGV: somefile \n\n",
        ),
        (
            &["myfile", "-force", "-f", "infile", "-9"],
            true,
            true,
            "getopt_long_only (POSIXLY_CORRECT): myfile -force -f infile -9 \nremaining ARGV: myfile -force -f infile -9 \n\n",
        ),
        (
            &[
                "onefile",
                "twofile",
                "-best",
                "-Williterate",
                "-i",
                "foo.in",
                "threefile",
            ],
            true,
            true,
            "getopt_long_only (POSIXLY_CORRECT): onefile twofile -best -Williterate -i foo.in threefile \nremaining ARGV: onefile twofile -best -Williterate -i foo.in threefile \n\n",
        ),
        (
            &["-1bfast", "-"],
            true,
            true,
            "getopt_long_only (POSIXLY_CORRECT): -1bfast - \noption 1\noption b\noption fast\noption -\n\n",
        ),
        (
            &["--fast", "-drinking=guiness", "-i", "foo.in", "somefile"],
            true,
            true,
            "getopt_long_only (POSIXLY_CORRECT): --fast -drinking=guiness -i foo.in somefile \noption fast\noption drinking, arg guiness\noption i, arg foo.in\nremaining ARGV: somefile \n\n",
        ),
    ];
    for (args, only, posix, expected) in cases {
        assert_eq!(&run_case(args, *only, *posix), expected, "args {args:?}");
    }
}

#[test]
fn getopt_does_not_permute() {
    let mut argv = vec![leak("prog"), leak("file"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"v", false, &mut diag);
    assert_eq!(step.code, -1);
    assert_eq!(state.optind(), 1);
    assert_eq!(&argv[1..], &[leak("file"), leak("-v")]);
}

#[test]
fn getopt_long_permutes() {
    let mut argv = vec![leak("prog"), leak("file"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"v", &[], false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'v'));
    assert_eq!(state.optind(), 3);
    // The permutation happens when the scan reaches the end of argv.
    let step = state.getopt_long(&mut argv, &Src, b"v", &[], false, false, &mut diag);
    assert_eq!(step.code, -1);
    assert_eq!(state.optind(), 2);
    assert_eq!(argv[2], b"file");
}

#[test]
fn getopt_performes_flag_is_false() {
    assert!(!GETOPT_PERMUTES);
}

#[test]
fn posixly_correct_stops_at_the_first_non_option() {
    let mut argv = vec![leak("prog"), leak("file"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"v", &[], false, true, &mut diag);
    assert_eq!(step.code, -1);
    assert_eq!(state.optind(), 1);
}

#[test]
fn plus_prefix_stops_at_the_first_non_option() {
    let mut argv = vec![leak("prog"), leak("file"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"+v", &[], false, false, &mut diag);
    assert_eq!(step.code, -1);
}

#[test]
fn minus_prefix_returns_non_options_as_one() {
    let mut argv = vec![leak("prog"), leak("file"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"-v", false, &mut diag);
    assert_eq!(step.code, 1);
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 1,
            offset: 0
        }
    );
    let step = state.getopt(&mut argv, &Src, b"-v", false, &mut diag);
    assert_eq!(step.code, i32::from(b'v'));
}

#[test]
fn double_dash_terminates_and_permutes() {
    let mut argv = vec![leak("prog"), leak("file"), leak("--"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"v", &[], false, false, &mut diag);
    assert_eq!(step.code, -1);
    assert_eq!(state.optind(), 2);
    assert_eq!(argv[1], b"--");
    assert_eq!(argv[2], b"file");
    assert_eq!(argv[3], b"-v");
}

#[test]
fn optind_zero_restarts() {
    let mut argv = vec![leak("prog"), leak("-v")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"v", false, &mut diag).code,
        i32::from(b'v')
    );
    state.set_optind(0);
    assert_eq!(
        state.getopt(&mut argv, &Src, b"v", false, &mut diag).code,
        i32::from(b'v')
    );
}

#[test]
fn optreset_restarts_after_a_finished_scan() {
    let mut argv = vec![leak("prog"), leak("-v"), leak("-x")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"vx", false, &mut diag).code,
        i32::from(b'v')
    );
    assert_eq!(
        state.getopt(&mut argv, &Src, b"vx", false, &mut diag).code,
        i32::from(b'x')
    );
    assert_eq!(
        state.getopt(&mut argv, &Src, b"vx", false, &mut diag).code,
        -1
    );
    state.set_optreset(true);
    state.set_optind(1);
    assert_eq!(
        state.getopt(&mut argv, &Src, b"vx", false, &mut diag).code,
        i32::from(b'v')
    );
}

#[test]
fn missing_argument_and_colon_optstring() {
    let mut argv = vec![leak("prog"), leak("-f")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"f:", false, &mut diag);
    assert_eq!(step.code, i32::from(b'?'));
    assert_eq!(state.optopt(), i32::from(b'f'));
    assert_eq!(diag.lines.len(), 1);
    assert_eq!(diag.lines[0], b"option requires an argument -- f");

    let mut argv = vec![leak("prog"), leak("-f")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b":f:", false, &mut diag);
    assert_eq!(step.code, i32::from(b':'));
    assert!(diag.lines.is_empty());
}

#[test]
fn opterr_zero_is_silent() {
    let mut argv = vec![leak("prog"), leak("-z")];
    let mut state = GetoptState::new();
    state.set_opterr(0);
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"v", false, &mut diag).code,
        i32::from(b'?')
    );
    assert!(diag.lines.is_empty());
    assert_eq!(state.optopt(), i32::from(b'z'));
}

#[test]
fn optional_argument_forms() {
    let opts = [LongOpt {
        name: b"opt",
        has_arg: OPTIONAL_ARGUMENT,
        flag_is_null: true,
        val: i32::from(b'o'),
    }];
    let mut argv = vec![leak("prog"), leak("--opt"), leak("--opt=x"), leak("-o")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"o::", &opts, false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'o'));
    assert_eq!(step.optarg, OptArg::None);
    let step = state.getopt_long(&mut argv, &Src, b"o::", &opts, false, false, &mut diag);
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 2,
            offset: 6
        }
    );
    let step = state.getopt_long(&mut argv, &Src, b"o::", &opts, false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'o'));
    assert_eq!(step.optarg, OptArg::None);
}

#[test]
fn long_options_abbreviate_and_detect_ambiguity() {
    let opts = [
        LongOpt {
            name: b"verbose",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 1,
        },
        LongOpt {
            name: b"verbatim",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 2,
        },
        LongOpt {
            name: b"version",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 3,
        },
    ];
    let mut argv = vec![leak("prog"), leak("--verb")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'?'));
    assert_eq!(step.long_index, None);
    assert_eq!(diag.lines[0], b"ambiguous option -- verb");

    let mut argv = vec![leak("prog"), leak("--vers")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(step.code, 3);
    assert_eq!(step.long_index, Some(2));
}

#[test]
fn no_argument_with_equals_is_an_error() {
    let opts = [LongOpt {
        name: b"force",
        has_arg: NO_ARGUMENT,
        flag_is_null: true,
        val: 0,
    }];
    let mut argv = vec![leak("prog"), leak("--force=1")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'?'));
    assert_eq!(state.optopt(), 0);
    assert_eq!(diag.lines[0], b"option doesn't take an argument -- force");
}

#[test]
fn flag_option_reports_the_index() {
    let opts = [LongOpt {
        name: b"flag",
        has_arg: NO_ARGUMENT,
        flag_is_null: false,
        val: 7,
    }];
    let mut argv = vec![leak("prog"), leak("--flag")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(step.code, 0);
    assert_eq!(step.flag_to_set, Some(0));
    assert_eq!(step.long_index, Some(0));
}

#[test]
fn w_semicolon_takes_a_long_option() {
    let opts = [LongOpt {
        name: b"verbose",
        has_arg: NO_ARGUMENT,
        flag_is_null: true,
        val: i32::from(b'v'),
    }];
    let mut argv = vec![leak("prog"), leak("-Wverbose")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"W;", &opts, false, false, &mut diag);
    assert_eq!(step.code, i32::from(b'v'));
    assert_eq!(step.long_index, Some(0));
}

#[test]
fn cluster_short_options_resume() {
    let mut argv = vec![leak("prog"), leak("-abc")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"abc", false, &mut diag).code,
        i32::from(b'a')
    );
    assert_eq!(
        state.getopt(&mut argv, &Src, b"abc", false, &mut diag).code,
        i32::from(b'b')
    );
    assert_eq!(
        state.getopt(&mut argv, &Src, b"abc", false, &mut diag).code,
        i32::from(b'c')
    );
    assert_eq!(
        state.getopt(&mut argv, &Src, b"abc", false, &mut diag).code,
        -1
    );
}

#[test]
fn attached_argument_is_reported_with_offset() {
    let mut argv = vec![leak("prog"), leak("-ovalue")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"o:", false, &mut diag);
    assert_eq!(step.code, i32::from(b'o'));
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 1,
            offset: 2
        }
    );
}

#[test]
fn bare_dash_is_an_option_when_the_optstring_has_it() {
    let mut argv = vec![leak("prog"), leak("-")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"x-", false, &mut diag);
    assert_eq!(step.code, i32::from(b'-'));
}

#[test]
fn getopt_short_no_argument() {
    let mut argv = vec![leak("prog"), leak("-a")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"a", false, &mut diag);
    assert_eq!(step.code, i32::from(b'a'));
    assert_eq!(step.optarg, OptArg::None);
}

#[test]
fn getopt_required_argument_attached() {
    let mut argv = vec![leak("prog"), leak("-avalue")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"a:", false, &mut diag);
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 1,
            offset: 2
        }
    );
}

#[test]
fn getopt_required_argument_separate() {
    let mut argv = vec![leak("prog"), leak("-a"), leak("value")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"a:", false, &mut diag);
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 2,
            offset: 0
        }
    );
    assert_eq!(state.optind(), 3);
}

#[test]
fn getopt_optional_argument_absent() {
    let mut argv = vec![leak("prog"), leak("-a")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt(&mut argv, &Src, b"a::", false, &mut diag);
    assert_eq!(step.optarg, OptArg::None);
}

#[test]
fn getopt_unknown_option_sets_optopt() {
    let mut argv = vec![leak("prog"), leak("-z")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"a", false, &mut diag).code,
        i32::from(b'?')
    );
    assert_eq!(state.optopt(), i32::from(b'z'));
}

#[test]
fn getopt_colon_missing_argument() {
    let mut argv = vec![leak("prog"), leak("-a")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b":a:", false, &mut diag).code,
        i32::from(b':')
    );
}

#[test]
fn long_exact_match_returns_val() {
    let opts = [LongOpt {
        name: b"verbose",
        has_arg: NO_ARGUMENT,
        flag_is_null: true,
        val: 7,
    }];
    let mut argv = vec![leak("prog"), leak("--verbose")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(step.code, 7);
    assert_eq!(step.long_index, Some(0));
}

#[test]
fn long_argument_with_equals() {
    let opts = [LongOpt {
        name: b"input",
        has_arg: REQUIRED_ARGUMENT,
        flag_is_null: true,
        val: 1,
    }];
    let mut argv = vec![leak("prog"), leak("--input=x")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag);
    assert_eq!(
        step.optarg,
        OptArg::At {
            arg_index: 1,
            offset: 8
        }
    );
}

#[test]
fn long_unknown_is_badch() {
    let opts = [LongOpt {
        name: b"verbose",
        has_arg: NO_ARGUMENT,
        flag_is_null: true,
        val: 1,
    }];
    let mut argv = vec![leak("prog"), leak("--nope")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state
            .getopt_long(&mut argv, &Src, b"", &opts, false, false, &mut diag)
            .code,
        i32::from(b'?')
    );
}

#[test]
fn double_dash_without_permutation() {
    let mut argv = vec![leak("prog"), leak("--"), leak("-a")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    assert_eq!(
        state.getopt(&mut argv, &Src, b"a", false, &mut diag).code,
        -1
    );
}

#[test]
fn optind_and_opterr_accessors() {
    let mut state = GetoptState::new();
    state.set_optind(3);
    state.set_opterr(0);
    assert_eq!(state.optind(), 3);
    assert_eq!(state.optopt(), i32::from(b'?'));
}

#[test]
fn long_only_single_dash() {
    let opts = [LongOpt {
        name: b"abc",
        has_arg: NO_ARGUMENT,
        flag_is_null: true,
        val: 5,
    }];
    let mut argv = vec![leak("prog"), leak("-abc")];
    let mut state = GetoptState::new();
    let mut diag = FakeDiag::default();
    let step = state.getopt_long_only(&mut argv, &Src, b"", &opts, false, &mut diag);
    assert_eq!(step.code, 5);
}
