// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `stdio-musl` differential: the memory streams (layer F).

#![cfg(feature = "oracle-stdio-musl")]

use raven_libc_oracle::config::Config;
use raven_libc_oracle::diff::{Fixed, Generator, run_diff};
use raven_libc_oracle::support_stdio::memcase::{
    MemCase, fmem_cases, mstream_cases, run_fmem_oracle, run_fmem_subject, run_mstream_oracle,
    run_mstream_subject,
};

fn oracle(case: &MemCase) -> raven_libc_oracle::support_stdio::memcase::MemObs {
    if case.kind == "fmem" {
        run_fmem_oracle(case)
    } else {
        run_mstream_oracle(case)
    }
}

fn subject(case: &MemCase) -> raven_libc_oracle::support_stdio::memcase::MemObs {
    if case.kind == "fmem" {
        run_fmem_subject(case)
    } else {
        run_mstream_subject(case)
    }
}

#[test]
fn diff_stdio_memory_streams() {
    let _guard = raven_libc_oracle::support_stdio::test_lock();
    let config = Config::from_env();
    if let Some(filter) = &config.filter
        && !"diff_stdio_memory_streams".contains(filter.as_str())
    {
        println!("diff_stdio_memory_streams: filtered out by RL_DIFF_FILTER");
        return;
    }
    let fmem = Fixed(fmem_cases());
    let mstream = Fixed(mstream_cases());
    let generators: [&dyn Generator<MemCase>; 2] = [&fmem, &mstream];
    let report = run_diff(
        "diff_stdio_memory_streams",
        &config,
        &generators,
        oracle,
        subject,
    );
    report.assert_clean();
}
