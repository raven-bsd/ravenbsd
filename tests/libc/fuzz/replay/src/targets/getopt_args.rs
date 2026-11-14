// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 15: `getopt` and `getopt_long` (C41).
//!
//! The body checks the step bound, the `optind` monotonicity, every `OptArg::At` range, the
//! one-diagnostic-per-step rule and the argv permutation rule.

use raven_libc_stdlib::getopt::{
    ArgSource, Diag, GetoptState, LongOpt, NO_ARGUMENT, OPTIONAL_ARGUMENT, REQUIRED_ARGUMENT, Step,
};

use crate::{Cursor, alloc_probe};

struct Args {
    entries: Vec<Vec<u8>>,
}

impl ArgSource for Args {
    type Arg = usize;

    fn text(&self, a: Self::Arg) -> &[u8] {
        self.entries.get(a).map(Vec::as_slice).unwrap_or(b"")
    }
}

#[derive(Default)]
struct CountingDiag {
    messages: usize,
}

impl Diag for CountingDiag {
    fn warnx(&mut self, _parts: &[&[u8]]) {
        self.messages = self.messages.saturating_add(1);
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

fn argument(cursor: &mut Cursor<'_>) -> Vec<u8> {
    let length = usize::from(cursor.u8() % 12).min(cursor.remaining());
    let raw = cursor.take(length);
    raw.iter()
        .map(|byte| match byte % 8 {
            0 => b'a',
            1 => b'b',
            2 => b'c',
            3 => b'=',
            4 => b'-',
            5 => b'x',
            6 => b'0',
            _ => b'y',
        })
        .collect()
}

fn optstring(cursor: &mut Cursor<'_>) -> Vec<u8> {
    let length = usize::from(cursor.u8() % 16).min(cursor.remaining());
    let raw = cursor.take(length);
    let mut out: Vec<u8> = raw
        .iter()
        .map(|byte| match byte % 9 {
            0 => b'a',
            1 => b'b',
            2 => b'c',
            3 => b':',
            4 => b'+',
            5 => b'-',
            6 => b'W',
            7 => b';',
            _ => b'x',
        })
        .collect();
    if out.is_empty() {
        out.push(b'a');
    }
    out
}

/// Run target 15.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let flags = cursor.u8();
    let long = flags & 1 != 0;
    let only = flags & 2 != 0;
    let posixly = flags & 4 != 0;
    let string = optstring(&mut cursor);
    let count = usize::from(cursor.u8() % 13);
    let mut entries = vec![b"prog".to_vec()];
    for _ in 0..count {
        entries.push(argument(&mut cursor));
    }
    let source = Args { entries };
    let argc = source.entries.len();
    let mut argv: Vec<usize> = (0..argc).collect();
    let mut original = argv.clone();
    original.sort_unstable();
    let longopts = [
        LongOpt {
            name: b"alpha",
            has_arg: NO_ARGUMENT,
            flag_is_null: true,
            val: 1000,
        },
        LongOpt {
            name: b"beta",
            has_arg: REQUIRED_ARGUMENT,
            flag_is_null: true,
            val: 1001,
        },
        LongOpt {
            name: b"gamma",
            has_arg: OPTIONAL_ARGUMENT,
            flag_is_null: false,
            val: 1002,
        },
    ];
    let mut state = GetoptState::new();
    let mut diagnostics = CountingDiag::default();
    let mut previous = state.optind();
    let mut steps = 0usize;
    let limit = argc.saturating_mul(14).saturating_add(2);
    let mut finished = false;
    while steps < limit {
        diagnostics.messages = 0;
        let step: Step = if long {
            state.getopt_long(
                &mut argv,
                &source,
                &string,
                &longopts,
                only,
                posixly,
                &mut diagnostics,
            )
        } else {
            state.getopt(&mut argv, &source, &string, posixly, &mut diagnostics)
        };
        steps = steps.saturating_add(1);
        assert!(
            diagnostics.messages <= 1,
            "P-getopt-4: more than one diagnostic in one step"
        );
        if let Some(argument) = inside(&source, step.optarg) {
            assert!(
                argument,
                "P-getopt-3: OptArg::At is outside the argument vector"
            );
        }
        let current = state.optind();
        let permuting = !posixly && string.first() != Some(&b'+');
        assert!(
            current <= i32::try_from(argc).unwrap_or(i32::MAX),
            "P-getopt-2: optind left its range"
        );
        if !permuting {
            assert!(
                current >= previous,
                "P-getopt-2: optind decreased without permutation"
            );
        }
        previous = current;
        if step.code == -1 {
            finished = true;
            break;
        }
    }
    assert!(finished, "P-getopt-1: no -1 within argc * 14 + 2 steps");
    let mut sorted = argv.clone();
    sorted.sort_unstable();
    if !posixly && string.first() != Some(&b'+') {
        assert!(
            sorted == original,
            "P-getopt-5: the argument vector is not a permutation"
        );
    }

    alloc_probe::note(hash(&[
        u64::from(flags),
        steps as u64,
        u64::from(state.optind() as u8),
        u64::from(string.len() as u8),
    ]));
}

fn inside(source: &Args, optarg: raven_libc_stdlib::getopt::OptArg) -> Option<bool> {
    match optarg {
        raven_libc_stdlib::getopt::OptArg::None => None,
        raven_libc_stdlib::getopt::OptArg::At { arg_index, offset } => {
            let text = source.text(arg_index);
            Some(arg_index < source.entries.len() && offset <= text.len())
        }
    }
}

#[cfg(test)]
#[path = "getopt_args_ut.rs"]
mod tests;
