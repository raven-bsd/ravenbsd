// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Harness configuration from the environment.
//!
//! | Variable | Default | Meaning |
//! |---|---|---|
//! | `RL_DIFF_SEEDS` | 20 | random seeds per function |
//! | `RL_DIFF_CASES` | 10000 | cases per seed |
//! | `RL_DIFF_SEED_BASE` | 1 | first seed |
//! | `RL_DIFF_EXHAUSTIVE` | 0 | run large exhaustive domains too |
//! | `RL_DIFF_FILTER` | unset | only run tests whose name contains the value |
//!
//! Parsing takes an explicit lookup function so tests need no process-global state.

/// Harness configuration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// Random seeds per function.
    pub seeds: u64,
    /// Cases per seed.
    pub cases: u64,
    /// First seed.
    pub seed_base: u64,
    /// Also run exhaustive domains that are not marked small.
    pub exhaustive: bool,
    /// Test-name filter.
    pub filter: Option<String>,
}

impl Config {
    /// The documented defaults: 20 seeds, 10,000 cases, seed base 1, no large exhaustive domains.
    #[must_use]
    pub fn defaults() -> Self {
        Self {
            seeds: 20,
            cases: 10_000,
            seed_base: 1,
            exhaustive: false,
            filter: None,
        }
    }

    /// Read the environment, printing a warning for every malformed value.
    #[must_use]
    pub fn from_env() -> Self {
        let (config, warnings) = Self::from_lookup(&|name| std::env::var(name).ok());
        for warning in warnings {
            eprintln!("raven-libc-oracle: {warning}");
        }
        config
    }

    /// Read the configuration through `lookup`; returns the config and warning strings.
    #[must_use]
    pub fn from_lookup(lookup: &dyn Fn(&str) -> Option<String>) -> (Self, Vec<String>) {
        let mut config = Self::defaults();
        let mut warnings = Vec::new();
        if let Some(value) = lookup("RL_DIFF_SEEDS") {
            match parse_u64(&value) {
                Some(seeds) if seeds > 0 => config.seeds = seeds,
                _ => warnings.push(format!(
                    "RL_DIFF_SEEDS=`{value}` is not a positive integer; using {}",
                    config.seeds
                )),
            }
        }
        if let Some(value) = lookup("RL_DIFF_CASES") {
            match parse_u64(&value) {
                Some(cases) if cases > 0 => config.cases = cases,
                _ => warnings.push(format!(
                    "RL_DIFF_CASES=`{value}` is not a positive integer; using {}",
                    config.cases
                )),
            }
        }
        if let Some(value) = lookup("RL_DIFF_SEED_BASE") {
            match parse_u64(&value) {
                Some(base) => config.seed_base = base,
                None => warnings.push(format!(
                    "RL_DIFF_SEED_BASE=`{value}` is not an integer; using {}",
                    config.seed_base
                )),
            }
        }
        if let Some(value) = lookup("RL_DIFF_EXHAUSTIVE") {
            match value.as_str() {
                "1" | "true" | "yes" => config.exhaustive = true,
                "0" | "false" | "no" => config.exhaustive = false,
                _ => warnings.push(format!(
                    "RL_DIFF_EXHAUSTIVE=`{value}` is not a boolean; using {}",
                    config.exhaustive
                )),
            }
        }
        if let Some(value) = lookup("RL_DIFF_FILTER")
            && !value.is_empty()
        {
            config.filter = Some(value);
        }
        (config, warnings)
    }
}

fn parse_u64(value: &str) -> Option<u64> {
    value.parse::<u64>().ok()
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
