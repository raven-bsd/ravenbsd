// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn all_thirty_one_names_and_numbers() {
    let expected = [
        "HUP", "INT", "QUIT", "ILL", "TRAP", "ABRT", "BUS", "FPE", "KILL", "USR1", "SEGV", "USR2",
        "PIPE", "ALRM", "TERM", "STKFLT", "CHLD", "CONT", "STOP", "TSTP", "TTIN", "TTOU", "URG",
        "XCPU", "XFSZ", "VTALRM", "PROF", "WINCH", "IO", "PWR", "SYS",
    ];
    assert_eq!(TABLE.len(), 31);
    for (index, name) in expected.iter().enumerate() {
        let number = (index as i32) + 1;
        assert_eq!(name_of(number), Some(*name), "signal {number}");
        assert_eq!(number_of(name), Some(number), "name {name}");
    }
}

#[test]
fn internal_and_rt_numbers_have_no_name() {
    for sig in [0, -1, 32, 33, 34, 65, 100] {
        assert_eq!(name_of(sig), None);
    }
}

#[test]
fn aliases_and_prefixes() {
    assert_eq!(number_of("IOT"), Some(6));
    assert_eq!(number_of("POLL"), Some(29));
    assert_eq!(number_of("SIGABRT"), Some(6));
    assert_eq!(number_of("sigabrt"), Some(6));
    assert_eq!(number_of("SigUsr1"), Some(10));
    assert_eq!(number_of("SIG"), None);
    assert_eq!(number_of(""), None);
    assert_eq!(number_of("NOTASIGNAL"), None);
}

#[test]
fn strip_prefix_only_once() {
    assert_eq!(strip_sig_prefix("SIGINT"), "INT");
    assert_eq!(strip_sig_prefix("SIGSIGINT"), "SIGINT");
    assert_eq!(strip_sig_prefix("INT"), "INT");
    assert_eq!(strip_sig_prefix("SI"), "SI");
}
