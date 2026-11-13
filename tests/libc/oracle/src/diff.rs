// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential runner: generators, classes, mismatches, shrinking and the reproducer format.
//!
//! Classes run in the order `Fixed`, `Random`, `Exhaustive`. The runner never stops at the first
//! mismatch: it records at most ten per class, keeps counting the executed cases, shrinks each
//! recorded case greedily (at most 200 steps) and renders a pasteable TOML snippet per mismatch.

use std::collections::{BTreeMap, BTreeSet};
use std::marker::PhantomData;

use crate::config::Config;
use crate::rng::Rng;
use crate::shrink;

/// Input class of a generator.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Class {
    /// The fixed corpus.
    Fixed,
    /// Seeded pseudo-random cases.
    Random,
    /// Exhaustive small domains.
    Exhaustive,
}

impl Class {
    /// Lower-case name used in the reproducer header.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Fixed => "fixed",
            Self::Random => "random",
            Self::Exhaustive => "exhaustive",
        }
    }
}

/// One generated input and its two observations.
pub trait Case: Clone + core::fmt::Debug {
    /// The whole observation: return value, every output byte (guard bytes included), errno.
    type Obs: PartialEq + Clone + core::fmt::Debug;

    /// Smaller candidates in preference order; the default has none.
    fn shrink(&self) -> Vec<Self> {
        Vec::new()
    }

    /// The `[[case]]` body as TOML.
    fn to_toml(&self) -> String;

    /// The observation rendered for the `# oracle:` / `# rust:` lines.
    fn obs_toml(obs: &Self::Obs) -> String;
}

/// A source of cases.
pub trait Generator<C: Case> {
    /// The class this generator belongs to.
    fn class(&self) -> Class;

    /// Yield `(seed, case)` pairs; seed 0 for the fixed and exhaustive classes.
    fn cases<'a>(&'a self, cfg: &'a Config) -> Box<dyn Iterator<Item = (u64, C)> + 'a>;

    /// Whether this exhaustive domain is small enough to run without `RL_DIFF_EXHAUSTIVE=1`.
    fn small(&self) -> bool {
        false
    }

    /// Cases for a generator at position `index` in the run.
    ///
    /// The default ignores `index`; [`Seeded`] mixes it into the seed so two random generators do
    /// not share a stream. Not part of the ticket API; callers use [`Generator::cases`].
    #[doc(hidden)]
    fn cases_for<'a>(
        &'a self,
        cfg: &'a Config,
        index: u64,
    ) -> Box<dyn Iterator<Item = (u64, C)> + 'a> {
        let _ = index;
        self.cases(cfg)
    }
}

/// The fixed corpus.
pub struct Fixed<C>(
    /// Cases in corpus order.
    pub Vec<C>,
);

impl<C: Case> Generator<C> for Fixed<C> {
    fn class(&self) -> Class {
        Class::Fixed
    }

    fn cases<'a>(&'a self, _cfg: &'a Config) -> Box<dyn Iterator<Item = (u64, C)> + 'a> {
        Box::new(self.0.iter().cloned().map(|case| (0, case)))
    }
}

/// A seeded generator.
///
/// `salt` is mixed into the seed so two generators with the same seed do not share a stream;
/// [`Seeded::new`] uses salt 0 and [`Seeded::with_salt`] lets a caller pick another one.
pub struct Seeded<C, F: Fn(&mut Rng) -> C> {
    /// Case factory.
    pub make: F,
    salt: u64,
    marker: PhantomData<fn() -> C>,
}

impl<C, F: Fn(&mut Rng) -> C> Seeded<C, F> {
    /// A seeded generator with salt 0.
    pub fn new(make: F) -> Self {
        Self {
            make,
            salt: 0,
            marker: PhantomData,
        }
    }

    /// A seeded generator whose stream is mixed with `salt`.
    pub fn with_salt(make: F, salt: u64) -> Self {
        Self {
            make,
            salt,
            marker: PhantomData,
        }
    }
}

impl<C: Case, F: Fn(&mut Rng) -> C> Generator<C> for Seeded<C, F> {
    fn class(&self) -> Class {
        Class::Random
    }

    fn cases<'a>(&'a self, cfg: &'a Config) -> Box<dyn Iterator<Item = (u64, C)> + 'a> {
        self.cases_for(cfg, 0)
    }

    fn cases_for<'a>(
        &'a self,
        cfg: &'a Config,
        index: u64,
    ) -> Box<dyn Iterator<Item = (u64, C)> + 'a> {
        let start = cfg.seed_base;
        let end = cfg.seed_base.saturating_add(cfg.seeds);
        let cases = cfg.cases;
        let salt = self.salt ^ index;
        let make = &self.make;
        Box::new((start..end).flat_map(move |seed| {
            let mut rng = Rng::seed_from_u64(seed ^ salt);
            (0..cases).map(move |_| (seed, make(&mut rng)))
        }))
    }
}

/// An exhaustive domain.
pub struct Exhaustive<C> {
    /// Whether the domain runs without `RL_DIFF_EXHAUSTIVE=1`.
    pub small: bool,
    /// Domain factory.
    pub make: fn() -> Box<dyn Iterator<Item = C>>,
}

impl<C: Case> Generator<C> for Exhaustive<C> {
    fn class(&self) -> Class {
        Class::Exhaustive
    }

    fn cases<'a>(&'a self, _cfg: &'a Config) -> Box<dyn Iterator<Item = (u64, C)> + 'a> {
        Box::new((self.make)().map(|case| (0, case)))
    }

    fn small(&self) -> bool {
        self.small
    }
}

/// One recorded mismatch, after shrinking.
pub struct Mismatch<C: Case> {
    /// Input class.
    pub class: Class,
    /// Seed (0 for fixed and exhaustive cases).
    pub seed: u64,
    /// Case index within the seed.
    pub index: u64,
    /// The shrunk input.
    pub case: C,
    /// Oracle observation.
    pub want: C::Obs,
    /// Subject observation.
    pub got: C::Obs,
}

/// The result of one differential run.
pub struct Report<C: Case> {
    /// Function or test name.
    pub name: String,
    /// Executed cases per class.
    pub ran: BTreeMap<Class, u64>,
    /// Recorded mismatches (at most ten per class).
    pub mismatches: Vec<Mismatch<C>>,
}

impl<C: Case> Report<C> {
    /// Whether every case agreed.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.mismatches.is_empty()
    }

    /// The classes that found at least one mismatch.
    #[must_use]
    pub fn killed_by(&self) -> BTreeSet<Class> {
        self.mismatches
            .iter()
            .map(|mismatch| mismatch.class)
            .collect()
    }

    /// Panic with the reproducer text when the run is not clean.
    ///
    /// # Panics
    ///
    /// Panics with [`Report::render`] when at least one mismatch was recorded.
    pub fn assert_clean(&self) {
        if !self.is_clean() {
            panic!("{}", self.render());
        }
    }

    /// The pasteable reproducer text, empty when the run is clean.
    #[must_use]
    pub fn render(&self) -> String {
        let mut text = String::new();
        for mismatch in &self.mismatches {
            text.push_str(&format!(
                "[[case]]                        # raven-libc-oracle mismatch: fn={} class={} seed={} index={}\n",
                self.name,
                mismatch.class.name(),
                mismatch.seed,
                mismatch.index
            ));
            text.push_str(&mismatch.case.to_toml());
            if !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&format!(
                "# oracle: {}\n# rust:   {}\n\n",
                C::obs_toml(&mismatch.want),
                C::obs_toml(&mismatch.got)
            ));
        }
        text
    }
}

/// Run every generator against the oracle and the subject.
pub fn run_diff<C: Case>(
    name: &str,
    cfg: &Config,
    gens: &[&dyn Generator<C>],
    oracle: impl Fn(&C) -> C::Obs,
    subject: impl Fn(&C) -> C::Obs,
) -> Report<C> {
    let mut report = Report {
        name: name.to_string(),
        ran: BTreeMap::new(),
        mismatches: Vec::new(),
    };
    for class in [Class::Fixed, Class::Random, Class::Exhaustive] {
        let mut ran = 0u64;
        let mut recorded = 0usize;
        for (generator_index, generator) in gens
            .iter()
            .enumerate()
            .filter(|(_, generator)| generator.class() == class)
        {
            if class == Class::Exhaustive && !cfg.exhaustive && !generator.small() {
                continue;
            }
            let mut last_seed = None;
            let mut index = 0u64;
            for (seed, case) in generator.cases_for(cfg, generator_index as u64) {
                if last_seed == Some(seed) {
                    index += 1;
                } else {
                    last_seed = Some(seed);
                    index = 0;
                }
                ran += 1;
                let want = oracle(&case);
                let got = subject(&case);
                if want != got && recorded < 10 {
                    let shrunk = shrink::greedy(
                        &case,
                        |candidate| oracle(candidate) != subject(candidate),
                        200,
                    );
                    let want = oracle(&shrunk);
                    let got = subject(&shrunk);
                    report.mismatches.push(Mismatch {
                        class,
                        seed,
                        index,
                        case: shrunk,
                        want,
                        got,
                    });
                    recorded += 1;
                }
            }
        }
        report.ran.insert(class, ran);
    }
    let fixed = report.ran.get(&Class::Fixed).copied().unwrap_or(0);
    let random = report.ran.get(&Class::Random).copied().unwrap_or(0);
    let exhaustive = report.ran.get(&Class::Exhaustive).copied().unwrap_or(0);
    println!(
        "{}: fixed {fixed}, random {random}, exhaustive {exhaustive} cases, {} mismatches",
        report.name,
        report.mismatches.len()
    );
    report
}

/// Declare a differential test.
///
/// ```ignore
/// diff_case! {
///     name: strlcpy,
///     case: StrlcpyCase,
///     fixed: "strlcpy/basic.toml",
///     random: |rng| StrlcpyCase::random(rng),
///     exhaustive: (small, StrlcpyCase::exhaustive),
///     oracle: |c| c.run_oracle(),
///     subject: |c| c.run_subject(standin::strlcpy::strlcpy),
/// }
/// ```
#[macro_export]
macro_rules! diff_case {
    (
        name: $name:ident,
        case: $case:ty,
        fixed: $fixed:expr,
        random: $random:expr,
        exhaustive: (small, $exhaustive:expr),
        oracle: $oracle:expr,
        subject: $subject:expr $(,)?
    ) => {
        #[test]
        fn $name() {
            let config = $crate::config::Config::from_env();
            let full = concat!("diff_", stringify!($name));
            if let Some(filter) = &config.filter {
                if !full.contains(filter.as_str()) && !stringify!($name).contains(filter.as_str()) {
                    println!("{full}: filtered out by RL_DIFF_FILTER");
                    return;
                }
            }
            let fixed = $crate::diff::Fixed($crate::corpus::load::<$case>($fixed));
            let random = $crate::diff::Seeded::new($random);
            let exhaustive = $crate::diff::Exhaustive {
                small: true,
                make: $exhaustive,
            };
            let generators: [&dyn $crate::diff::Generator<$case>; 3] =
                [&fixed, &random, &exhaustive];
            let report = $crate::diff::run_diff(full, &config, &generators, $oracle, $subject);
            report.assert_clean();
        }
    };
}

#[cfg(test)]
#[path = "diff_ut.rs"]
mod tests;
