// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[derive(Clone, Debug, PartialEq)]
struct Number(u64);

impl Case for Number {
    type Obs = u64;

    fn shrink(&self) -> Vec<Self> {
        if self.0 == 0 {
            Vec::new()
        } else {
            vec![Self(self.0 / 2)]
        }
    }

    fn to_toml(&self) -> String {
        format!("n = {}", self.0)
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!("ret = {obs}")
    }
}

fn small_domain() -> Box<dyn Iterator<Item = Number>> {
    Box::new((0..4).map(Number))
}

fn large_domain() -> Box<dyn Iterator<Item = Number>> {
    Box::new((0..100).map(Number))
}

fn oracle(case: &Number) -> u64 {
    case.0
}

fn subject(case: &Number) -> u64 {
    case.0 + case.0 % 2
}

fn config(seeds: u64, cases: u64) -> Config {
    Config {
        seeds,
        cases,
        seed_base: 1,
        exhaustive: false,
        filter: None,
    }
}

#[test]
fn classes_run_in_order_and_counts_are_kept() {
    let fixed = Fixed(vec![Number(0), Number(1), Number(2)]);
    let random = Seeded::new(|rng| Number(rng.below(10)));
    let exhaustive = Exhaustive {
        small: true,
        make: small_domain,
    };
    let generators: [&dyn Generator<Number>; 3] = [&fixed, &random, &exhaustive];
    let report = run_diff("counts", &config(2, 5), &generators, oracle, subject);
    let classes: Vec<Class> = report.ran.keys().copied().collect();
    assert_eq!(
        classes,
        vec![Class::Fixed, Class::Random, Class::Exhaustive]
    );
    assert_eq!(report.ran[&Class::Fixed], 3);
    assert_eq!(report.ran[&Class::Random], 10);
    assert_eq!(report.ran[&Class::Exhaustive], 4);
}

#[test]
fn a_large_exhaustive_domain_is_skipped_unless_enabled() {
    let exhaustive = Exhaustive {
        small: false,
        make: large_domain,
    };
    let generators: [&dyn Generator<Number>; 1] = [&exhaustive];
    let report = run_diff("skip", &config(1, 1), &generators, oracle, subject);
    assert_eq!(report.ran[&Class::Exhaustive], 0);
    let mut enabled = config(1, 1);
    enabled.exhaustive = true;
    let report = run_diff("enabled", &enabled, &generators, oracle, subject);
    assert_eq!(report.ran[&Class::Exhaustive], 100);
}

#[test]
fn at_most_ten_mismatches_are_recorded_but_every_case_is_counted() {
    let random = Seeded::new(|rng| {
        let _ = rng.next_u64();
        Number(1)
    });
    let generators: [&dyn Generator<Number>; 1] = [&random];
    let report = run_diff("cap", &config(1, 100), &generators, oracle, subject);
    assert_eq!(report.ran[&Class::Random], 100);
    assert_eq!(report.mismatches.len(), 10);
    assert!(!report.is_clean());
}

#[test]
fn render_carries_the_header_and_the_two_observations() {
    let fixed = Fixed(vec![Number(1)]);
    let generators: [&dyn Generator<Number>; 1] = [&fixed];
    let report = run_diff("odd", &config(1, 1), &generators, oracle, subject);
    let text = report.render();
    assert!(
        text.starts_with(
            "[[case]]                        # raven-libc-oracle mismatch: fn=odd class=fixed seed=0 index=0"
        )
    );
    assert!(text.contains("n = 1"));
    assert!(text.contains("# oracle: ret = 1"));
    assert!(text.contains("# rust:   ret = 2"));
    assert!(report.killed_by().contains(&Class::Fixed));
}

#[test]
fn shrinking_is_applied_to_the_recorded_case() {
    let random = Seeded::new(|_rng| Number(100));
    let generators: [&dyn Generator<Number>; 1] = [&random];
    let report = run_diff("shrink", &config(1, 1), &generators, |case| case.0, |_| 99);
    assert_eq!(report.mismatches.len(), 1);
    assert_eq!(report.mismatches[0].case.0, 0);
}

#[test]
#[should_panic(expected = "[[case]]")]
fn assert_clean_panics_with_the_reproducer() {
    let fixed = Fixed(vec![Number(1)]);
    let generators: [&dyn Generator<Number>; 1] = [&fixed];
    let report = run_diff("panic", &config(1, 1), &generators, oracle, subject);
    report.assert_clean();
}

#[test]
fn a_clean_run_has_no_render_text() {
    let fixed = Fixed(vec![Number(0), Number(2), Number(4)]);
    let generators: [&dyn Generator<Number>; 1] = [&fixed];
    let report = run_diff("clean", &config(1, 1), &generators, oracle, subject);
    assert!(report.is_clean());
    assert!(report.render().is_empty());
    assert!(report.killed_by().is_empty());
}

#[test]
fn the_generator_index_salts_the_random_stream() {
    let first = Seeded::new(|rng| Number(rng.next_u64()));
    let second = Seeded::new(|rng| Number(rng.next_u64()));
    let cfg = config(1, 1);
    let a = first.cases_for(&cfg, 0).next().expect("case").1;
    let again = first.cases_for(&cfg, 0).next().expect("case").1;
    let b = second.cases_for(&cfg, 1).next().expect("case").1;
    assert_eq!(a, again);
    assert_ne!(a, b);
}
