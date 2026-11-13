// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Self-oracle mutation drill: the stand-in is the oracle, so no C is needed.

use super::memchr as mutant_memchr;
use super::strlcpy as mutant_strlcpy;
use super::strnlen as mutant_strnlen;
use crate::call::{MemchrCase, StrlcpyCase, StrnlenCase};
use crate::config::Config;
use crate::corpus;
use crate::diff::{Class, Exhaustive, Fixed, Generator, Seeded, run_diff};
use crate::standin;

#[test]
fn strlcpy_standin_is_clean_and_all_four_mutants_die() {
    let config = Config::defaults();
    let fixed = Fixed(corpus::load::<StrlcpyCase>("strlcpy/basic.toml"));
    let random = Seeded::new(StrlcpyCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: StrlcpyCase::exhaustive,
    };
    let generators: [&dyn Generator<StrlcpyCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "strlcpy stand-in",
        &config,
        &generators,
        |case| case.run_subject(standin::strlcpy::strlcpy),
        |case| case.run_subject(standin::strlcpy::strlcpy),
    );
    clean.assert_clean();
    for (name, mutant) in mutant_strlcpy::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_subject(standin::strlcpy::strlcpy),
            |case| case.run_subject(mutant),
        );
        println!("mutant strlcpy::{name}: killed_by={:?}", report.killed_by());
        assert!(!report.is_clean(), "strlcpy::{name} survived");
        assert!(report.killed_by().contains(&Class::Random));
    }
}

#[test]
fn strnlen_standin_is_clean_and_both_mutants_die() {
    let config = Config::defaults();
    let fixed = Fixed(corpus::load::<StrnlenCase>("strnlen/basic.toml"));
    let random = Seeded::new(StrnlenCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: StrnlenCase::exhaustive,
    };
    let generators: [&dyn Generator<StrnlenCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "strnlen stand-in",
        &config,
        &generators,
        |case| case.run_subject(standin::strnlen::strnlen),
        |case| case.run_subject(standin::strnlen::strnlen),
    );
    clean.assert_clean();
    for (name, mutant) in mutant_strnlen::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_subject(standin::strnlen::strnlen),
            |case| case.run_subject(mutant),
        );
        println!("mutant strnlen::{name}: killed_by={:?}", report.killed_by());
        assert!(!report.is_clean(), "strnlen::{name} survived");
        assert!(report.killed_by().contains(&Class::Random));
        assert!(report.killed_by().contains(&Class::Exhaustive));
    }
}

#[test]
fn memchr_standin_is_clean_and_both_mutants_die() {
    let config = Config::defaults();
    let fixed = Fixed(corpus::load::<MemchrCase>("memchr/basic.toml"));
    let random = Seeded::new(MemchrCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: MemchrCase::exhaustive,
    };
    let generators: [&dyn Generator<MemchrCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "memchr stand-in",
        &config,
        &generators,
        |case| case.run_subject(standin::memchr::memchr),
        |case| case.run_subject(standin::memchr::memchr),
    );
    clean.assert_clean();
    for (name, mutant) in mutant_memchr::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_subject(standin::memchr::memchr),
            |case| case.run_subject(mutant),
        );
        println!("mutant memchr::{name}: killed_by={:?}", report.killed_by());
        assert!(!report.is_clean(), "memchr::{name} survived");
        assert!(report.killed_by().contains(&Class::Random));
        assert!(report.killed_by().contains(&Class::Exhaustive));
    }
}

#[test]
fn the_fixed_corpus_alone_kills_at_least_six_mutants() {
    let mut killed = 0usize;
    let strlcpy_fixed = Fixed(corpus::load::<StrlcpyCase>("strlcpy/basic.toml"));
    let strlcpy_generators: [&dyn Generator<StrlcpyCase>; 1] = [&strlcpy_fixed];
    for (name, mutant) in mutant_strlcpy::ALL {
        let report = run_diff(
            name,
            &Config::defaults(),
            &strlcpy_generators,
            |case| case.run_subject(standin::strlcpy::strlcpy),
            |case| case.run_subject(mutant),
        );
        if !report.is_clean() {
            killed += 1;
            println!("fixed corpus kills strlcpy::{name}");
        }
    }
    let strnlen_fixed = Fixed(corpus::load::<StrnlenCase>("strnlen/basic.toml"));
    let strnlen_generators: [&dyn Generator<StrnlenCase>; 1] = [&strnlen_fixed];
    for (name, mutant) in mutant_strnlen::ALL {
        let report = run_diff(
            name,
            &Config::defaults(),
            &strnlen_generators,
            |case| case.run_subject(standin::strnlen::strnlen),
            |case| case.run_subject(mutant),
        );
        if !report.is_clean() {
            killed += 1;
            println!("fixed corpus kills strnlen::{name}");
        }
    }
    let memchr_fixed = Fixed(corpus::load::<MemchrCase>("memchr/basic.toml"));
    let memchr_generators: [&dyn Generator<MemchrCase>; 1] = [&memchr_fixed];
    for (name, mutant) in mutant_memchr::ALL {
        let report = run_diff(
            name,
            &Config::defaults(),
            &memchr_generators,
            |case| case.run_subject(standin::memchr::memchr),
            |case| case.run_subject(mutant),
        );
        if !report.is_clean() {
            killed += 1;
            println!("fixed corpus kills memchr::{name}");
        }
    }
    println!("fixed corpus killed {killed} of 8 mutants");
    assert!(killed >= 6, "fixed corpus killed only {killed} of 8");
}

#[test]
fn shrinking_reaches_a_short_memchr_case() {
    let case = MemchrCase {
        c: 0x41,
        bytes: b"xxxxxxxxxA".to_vec(),
    };
    let fixed = Fixed(vec![case]);
    let generators: [&dyn Generator<MemchrCase>; 1] = [&fixed];
    let report = run_diff(
        "shrink-lastbyte",
        &Config::defaults(),
        &generators,
        |case| case.run_subject(standin::memchr::memchr),
        |case| case.run_subject(mutant_memchr::lastbyte),
    );
    assert!(!report.is_clean());
    assert!(report.mismatches[0].case.bytes.len() <= 2);
}
