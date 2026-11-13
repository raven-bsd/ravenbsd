// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::diff::Case;

#[test]
fn strlcpy_cases_parse_from_the_corpus_form() {
    let case: StrlcpyCase =
        toml::from_str("dsize = 4\nbuflen = 8\nfill = 170\nbytes_hex = \"\"\n").expect("case");
    assert_eq!(case.dsize, 4);
    assert_eq!(case.buflen, 8);
    assert_eq!(case.bytes, Vec::<u8>::new());
    let case: StrlcpyCase =
        toml::from_str("dsize = 2\nbuflen = 6\nfill = 0\nbytes = \"hi\"\n").expect("case");
    assert_eq!(case.bytes, b"hi");
}

#[test]
fn a_strlcpy_case_that_would_overflow_the_buffer_is_refused() {
    let error = toml::from_str::<StrlcpyCase>("dsize = 8\nbuflen = 4\nfill = 0\nbytes = \"\"\n")
        .unwrap_err();
    assert!(error.to_string().contains("smaller than dsize"));
}

#[test]
fn strlcpy_normalises_at_the_first_zero() {
    let case = StrlcpyCase {
        dsize: 4,
        buflen: 8,
        fill: 0,
        bytes: b"ab\0cd".to_vec(),
    };
    assert_eq!(case.normalised(), b"ab");
    assert_eq!(case.run_subject(crate::standin::strlcpy::strlcpy).ret, 2);
}

#[test]
fn strlcpy_exhaustive_domain_is_217_cases() {
    assert_eq!(StrlcpyCase::exhaustive().count(), 217);
}

#[test]
fn strlcpy_subject_observation_includes_the_guard_bytes() {
    let case = StrlcpyCase {
        dsize: 4,
        buflen: 7,
        fill: 0xAA,
        bytes: b"hello".to_vec(),
    };
    let obs = case.run_subject(crate::standin::strlcpy::strlcpy);
    assert_eq!(obs.ret, 5);
    assert_eq!(&obs.buf[..4], b"hel\0");
    assert_eq!(&obs.buf[4..], [0xAA, 0xAA, 0xAA]);
}

#[test]
fn strnlen_and_memchr_cases_parse_and_reject_unknown_keys() {
    let case: StrnlenCase = toml::from_str("maxlen = 3\nbytes = \"abc\"\n").expect("case");
    assert_eq!(case.maxlen, 3);
    let case: MemchrCase = toml::from_str("c = 128\nbytes_hex = \"7f80\"\n").expect("case");
    assert_eq!(case.c, 128);
    assert!(toml::from_str::<StrnlenCase>("maxlen = 3\nbytes = \"a\"\nother = 1\n").is_err());
    assert!(toml::from_str::<MemchrCase>("c = 1\nbytes = \"a\"\nother = 1\n").is_err());
}

#[test]
fn exhaustive_counts_match_the_documented_domains() {
    assert_eq!(StrnlenCase::exhaustive().count(), 1093 * 9);
    assert_eq!(MemchrCase::exhaustive().count(), 1093 * 10);
}

#[test]
fn random_cases_keep_the_buffer_invariant() {
    let mut rng = Rng::seed_from_u64(5);
    for _ in 0..1000 {
        let case = StrlcpyCase::random(&mut rng);
        assert!(case.buflen >= case.dsize);
    }
}

#[test]
fn shrink_candidates_are_smaller() {
    let case = StrlcpyCase {
        dsize: 5,
        buflen: 9,
        fill: 0,
        bytes: b"hello".to_vec(),
    };
    let candidates = case.shrink();
    assert_eq!(candidates[0].dsize, 4);
    assert_eq!(candidates[1].bytes, b"hell");
    assert_eq!(candidates[2].buflen, 8);
    let memchr = MemchrCase {
        c: 0x80,
        bytes: vec![0x61, 0x80],
    };
    let candidates = memchr.shrink();
    assert_eq!(candidates[0].bytes, vec![0x80]);
    assert_eq!(candidates[1].bytes, vec![0x61]);
    assert_eq!(candidates[2].c, 0);
}

#[test]
fn obs_toml_is_the_reproducer_observation() {
    let obs = StrlcpyObs {
        ret: 5,
        buf: b"hel\0".to_vec(),
    };
    assert_eq!(
        StrlcpyCase::obs_toml(&obs),
        "ret = 5, buf_hex = \"68656c00\""
    );
    assert_eq!(StrnlenCase::obs_toml(&3), "ret = 3");
    assert_eq!(MemchrCase::obs_toml(&Some(2)), "ret = 2");
    assert_eq!(MemchrCase::obs_toml(&None), "ret = -1");
}

#[test]
fn to_toml_round_trips_through_the_parser() {
    let case = StrlcpyCase {
        dsize: 3,
        buflen: 7,
        fill: 0xAA,
        bytes: vec![0x00, 0xff],
    };
    let text = case.to_toml();
    let parsed: StrlcpyCase = toml::from_str(&text).expect("parse");
    assert_eq!(parsed, case);
    let case = MemchrCase {
        c: -1,
        bytes: vec![0x80, 0x00],
    };
    let parsed: MemchrCase = toml::from_str(&case.to_toml()).expect("parse");
    assert_eq!(parsed, case);
}
