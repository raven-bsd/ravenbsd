// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdio-musl` differential: the operation scripts (layer A), the boundary matrix (layer B)
//! and the exhaustive `ungetc`/`fseek`/`ftell` interleavings (layer D).

#![cfg(feature = "oracle-stdio-musl")]

use raven_libc_oracle::config::Config;
use raven_libc_oracle::diff::{Fixed, Generator, Seeded, run_diff};
use raven_libc_oracle::support_stdio::ScriptCase;
use raven_libc_oracle::support_stdio::case::{run_oracle, run_subject};
use raven_libc_oracle::support_stdio::generate;

#[test]
fn diff_stdio_scripts() {
    let _guard = raven_libc_oracle::support_stdio::test_lock();
    let config = Config::from_env();
    if let Some(filter) = &config.filter
        && !"diff_stdio_scripts".contains(filter.as_str())
    {
        println!("diff_stdio_scripts: filtered out by RL_DIFF_FILTER");
        return;
    }
    let fixed = Fixed(raven_libc_oracle::corpus::load::<ScriptCase>(
        "stdio/scripts/regress.toml",
    ));
    let random = Seeded::new(generate::random_case);
    let generators: [&dyn Generator<ScriptCase>; 2] = [&fixed, &random];
    let report = run_diff(
        "diff_stdio_scripts",
        &config,
        &generators,
        run_oracle,
        run_subject,
    );
    report.assert_clean();
}

#[test]
fn diff_stdio_boundaries() {
    let _guard = raven_libc_oracle::support_stdio::test_lock();
    let config = Config::from_env();
    if let Some(filter) = &config.filter
        && !"diff_stdio_boundaries".contains(filter.as_str())
    {
        println!("diff_stdio_boundaries: filtered out by RL_DIFF_FILTER");
        return;
    }
    let fixed = Fixed(generate::boundary_cases());
    let generators: [&dyn Generator<ScriptCase>; 1] = [&fixed];
    let report = run_diff(
        "diff_stdio_boundaries",
        &config,
        &generators,
        run_oracle,
        run_subject,
    );
    report.assert_clean();
}

#[test]
fn diff_stdio_ungetc() {
    let _guard = raven_libc_oracle::support_stdio::test_lock();
    let config = Config::from_env();
    if let Some(filter) = &config.filter
        && !"diff_stdio_ungetc".contains(filter.as_str())
    {
        println!("diff_stdio_ungetc: filtered out by RL_DIFF_FILTER");
        return;
    }
    let fixed = Fixed(generate::ungetc_cases());
    let generators: [&dyn Generator<ScriptCase>; 1] = [&fixed];
    let report = run_diff(
        "diff_stdio_ungetc",
        &config,
        &generators,
        run_oracle,
        run_subject,
    );
    report.assert_clean();
}
