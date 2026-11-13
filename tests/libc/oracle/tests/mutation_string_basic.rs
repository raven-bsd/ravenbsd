// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Mutation proof against the **C oracle**: the same generators that pass for the stand-ins must
//! kill all eight broken variants. Prints the mutant x class matrix for the report.

#![cfg(feature = "oracle-string-basic")]

use raven_libc_oracle::call::{MemchrCase, StrlcpyCase, StrnlenCase};
use raven_libc_oracle::config::Config;
use raven_libc_oracle::corpus;
use raven_libc_oracle::diff::{Class, Exhaustive, Fixed, Generator, Seeded, run_diff};
use raven_libc_oracle::mutant;
use raven_libc_oracle::standin;

fn selected(name: &str, config: &Config) -> bool {
    config
        .filter
        .as_ref()
        .is_none_or(|filter| name.contains(filter.as_str()))
}

fn print_matrix(function: &str, mutant: &str, killed: &std::collections::BTreeSet<Class>) {
    println!(
        "mutant {function}::{mutant}: fixed={} random={} exhaustive={}",
        killed.contains(&Class::Fixed),
        killed.contains(&Class::Random),
        killed.contains(&Class::Exhaustive)
    );
}

#[test]
fn strlcpy_standin_matches_the_c_oracle_and_four_mutants_die() {
    let config = Config::from_env();
    if !selected("strlcpy", &config) {
        println!("strlcpy mutation: filtered out");
        return;
    }
    let fixed = Fixed(corpus::load::<StrlcpyCase>("strlcpy/basic.toml"));
    let random = Seeded::new(StrlcpyCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: StrlcpyCase::exhaustive,
    };
    let generators: [&dyn Generator<StrlcpyCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "strlcpy stand-in vs C",
        &config,
        &generators,
        |case| case.run_oracle(),
        |case| case.run_subject(standin::strlcpy::strlcpy),
    );
    clean.assert_clean();
    for (name, mutant) in mutant::strlcpy::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_oracle(),
            |case| case.run_subject(mutant),
        );
        print_matrix("strlcpy", name, &report.killed_by());
        assert!(!report.is_clean(), "strlcpy::{name} survived the C oracle");
        assert!(report.killed_by().contains(&Class::Random));
        assert!(report.killed_by().contains(&Class::Exhaustive));
    }
}

#[test]
fn strnlen_standin_matches_the_c_oracle_and_two_mutants_die() {
    let config = Config::from_env();
    if !selected("strnlen", &config) {
        println!("strnlen mutation: filtered out");
        return;
    }
    let fixed = Fixed(corpus::load::<StrnlenCase>("strnlen/basic.toml"));
    let random = Seeded::new(StrnlenCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: StrnlenCase::exhaustive,
    };
    let generators: [&dyn Generator<StrnlenCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "strnlen stand-in vs C",
        &config,
        &generators,
        |case| case.run_oracle(),
        |case| case.run_subject(standin::strnlen::strnlen),
    );
    clean.assert_clean();
    for (name, mutant) in mutant::strnlen::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_oracle(),
            |case| case.run_subject(mutant),
        );
        print_matrix("strnlen", name, &report.killed_by());
        assert!(!report.is_clean(), "strnlen::{name} survived the C oracle");
        assert!(report.killed_by().contains(&Class::Random));
        assert!(report.killed_by().contains(&Class::Exhaustive));
    }
}

#[test]
fn memchr_standin_matches_the_c_oracle_and_two_mutants_die() {
    let config = Config::from_env();
    if !selected("memchr", &config) {
        println!("memchr mutation: filtered out");
        return;
    }
    let fixed = Fixed(corpus::load::<MemchrCase>("memchr/basic.toml"));
    let random = Seeded::new(MemchrCase::random);
    let exhaustive = Exhaustive {
        small: true,
        make: MemchrCase::exhaustive,
    };
    let generators: [&dyn Generator<MemchrCase>; 3] = [&fixed, &random, &exhaustive];
    let clean = run_diff(
        "memchr stand-in vs C",
        &config,
        &generators,
        |case| case.run_oracle(),
        |case| case.run_subject(standin::memchr::memchr),
    );
    clean.assert_clean();
    for (name, mutant) in mutant::memchr::ALL {
        let report = run_diff(
            name,
            &config,
            &generators,
            |case| case.run_oracle(),
            |case| case.run_subject(mutant),
        );
        print_matrix("memchr", name, &report.killed_by());
        assert!(!report.is_clean(), "memchr::{name} survived the C oracle");
        assert!(report.killed_by().contains(&Class::Random));
        assert!(report.killed_by().contains(&Class::Exhaustive));
    }
}
