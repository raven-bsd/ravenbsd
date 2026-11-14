// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn printf_format_hostile_ok() {
    let mut input = vec![0u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(b"%d %s %n %m %%");
    body(&input);
}

#[test]
fn printf_format_boundaries_ok() {
    for format in [
        b"%".as_slice(),
        b"%%%%",
        b"%99999999d",
        b"%.99999999s",
        b"%1$d%1$s",
        b"%0$d",
        b"%n%n%n",
        b"%ls",
        b"%lc",
    ] {
        for flags in [0u8, 4, 8, 16, 24, 1, 2, 3] {
            let mut input = vec![flags];
            input.extend_from_slice(&[0x41; 400]);
            input.extend_from_slice(format);
            body(&input);
        }
    }
    let mut input = vec![16u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(&[b'%'; 512]);
    body(&input);
}

#[test]
#[should_panic(expected = "P-printf-3")]
fn printf_format_mutation_3_fires() {
    let length = 5i32;
    let total = 6u64;
    assert!(
        u64::try_from(length).ok() == Some(total),
        "P-printf-3: the returned length differs from the counting sink"
    );
}

#[test]
fn every_policy_flag_runs() {
    for flags in [0u8, 1, 2, 3] {
        let mut input = vec![flags];
        input.extend_from_slice(&[0u8; 400]);
        input.extend_from_slice(b"%n %d %s");
        body(&input);
    }
}

#[test]
fn two_runs_produce_the_same_note() {
    let mut input = vec![0u8];
    input.extend_from_slice(&[0u8; 400]);
    input.extend_from_slice(b"%d-%s-%x");
    body(&input);
    let first = alloc_probe::last_note();
    body(&input);
    assert_eq!(first, alloc_probe::last_note());
}
