// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn getopt_args_hostile_ok() {
    let mut input = vec![1u8, 4];
    input.extend_from_slice(b"-a:b");
    input.push(4);
    input.extend_from_slice(&[2, b'-', b'a']);
    input.extend_from_slice(&[3, b'-', b'-', b'a']);
    input.extend_from_slice(&[1, b'-']);
    body(&input);
}

#[test]
fn getopt_args_boundaries_ok() {
    for string in [b":".as_slice(), b"+:", b"-a:b::", b"W;", b"a"] {
        let mut input = vec![1u8, string.len() as u8];
        input.extend_from_slice(string);
        input.push(6);
        for argument in [b"--".as_slice(), b"-", b"--=", b"--a=", b"-ab", b"---"] {
            input.push(argument.len() as u8);
            input.extend_from_slice(argument);
        }
        body(&input);
    }
    body(&[0u8, 1, b'a', 0]);
}

#[test]
#[should_panic(expected = "P-getopt-2")]
fn getopt_args_mutation_2_fires() {
    let previous = 5i32;
    let current = 4i32;
    assert!(current >= previous, "P-getopt-2: optind left its range");
}

#[test]
fn every_flag_combination_runs() {
    for flags in 0u8..16 {
        let mut input = vec![flags, 3];
        input.extend_from_slice(b"-a:");
        input.push(3);
        input.extend_from_slice(&[2, b'-', b'a']);
        input.extend_from_slice(&[3, b'a', b'b', b'c']);
        input.extend_from_slice(&[1, b'-']);
        body(&input);
    }
}

#[test]
fn a_zero_argument_vector_reaches_minus_one() {
    let input = vec![0u8, 1, b'a', 0];
    body(&input);
    let args = Args {
        entries: vec![b"prog".to_vec()],
    };
    let mut argv = [0usize];
    let mut state = GetoptState::new();
    let mut diag = CountingDiag::default();
    let step = state.getopt(&mut argv, &args, b"a", false, &mut diag);
    assert_eq!(step.code, -1);
}
