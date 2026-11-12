// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Run the performance matrix and write `tests/tcpip/PERFORMANCE.md`.
//!
//! smoltcp is retired from the tree, so its columns are frozen measurements taken before the
//! removal (see `REFERENCE`), not live runs.

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use raven_tcpip_bench::{Run, raven_transfer, raven_ttfb};

fn median(values: &mut [Duration]) -> Duration {
    values.sort_unstable();
    values[values.len() / 2]
}

fn spread(values: &[Duration]) -> Duration {
    let min = values.iter().min().copied().unwrap_or_default();
    let max = values.iter().max().copied().unwrap_or_default();
    max.saturating_sub(min)
}

fn five<F: FnMut() -> Run>(mut run: F) -> (Run, Duration, Duration) {
    let mut first = None;
    let mut walls = Vec::new();
    for _ in 0..5 {
        let result = run();
        if let Some(expected) = first {
            assert_eq!(
                result.virtual_ms, expected,
                "virtual metric changed between runs"
            );
        } else {
            first = Some(result.virtual_ms);
        }
        walls.push(result.wall);
    }
    let mut result = run();
    let median_wall = median(&mut walls.clone());
    let spread_wall = spread(&walls);
    result.wall = median_wall;
    (result, median_wall, spread_wall)
}

/// One frozen reference row: virtual milliseconds for the 1 MiB transfers.
struct Reference {
    /// smoltcp 0.14 as configured by the old `raven-netcore` default (no congestion control).
    no_cc_ms: i64,
    /// smoltcp 0.14 with the `socket-tcp-reno` feature, the fair congestion-controlled baseline.
    reno_ms: i64,
}

/// Measured on this host with the C30 harness immediately before smoltcp was removed
/// (C32). Virtual time is deterministic; the same scenarios and seeds are run natively below.
const REFERENCE: [(&str, Reference); 6] = [
    (
        "lossless 1 ms",
        Reference {
            no_cc_ms: 32,
            reno_ms: 54,
        },
    ),
    (
        "lossless 20 ms",
        Reference {
            no_cc_ms: 367,
            reno_ms: 801,
        },
    ),
    (
        "lossless 200 ms",
        Reference {
            no_cc_ms: 3517,
            reno_ms: 7911,
        },
    ),
    (
        "1% loss",
        Reference {
            no_cc_ms: 484,
            reno_ms: 10181,
        },
    ),
    (
        "5% loss",
        Reference {
            no_cc_ms: 5941,
            reno_ms: 70691,
        },
    ),
    (
        "10% reordering",
        Reference {
            no_cc_ms: 1567,
            reno_ms: 3151,
        },
    ),
];

/// Frozen CPU reference, wall ms per MiB (1 MiB, 20 ms RTT, median of five).
const CPU_NO_CC_MS: f64 = 4.4;
const CPU_RENO_MS: f64 = 4.6;

/// Frozen time-to-first-byte reference at 20 ms RTT (virtual ms; the handshake does not
/// depend on congestion control, so the Reno build was not measured separately).
const TTFB_REF_MS: i64 = 62;

fn mib_s(ms: i64) -> f64 {
    (1.0) / (ms as f64 / 1000.0)
}

fn reference(name: &str) -> &'static Reference {
    &REFERENCE
        .iter()
        .find(|(label, _)| *label == name)
        .expect("reference row")
        .1
}

/// Verdict against the fair (Reno) baseline: virtual time, lower is better, 10% tolerance.
fn verdict_ms(native_ms: i64, reference_ms: i64) -> &'static str {
    if native_ms * 100 < reference_ms * 90 {
        "better"
    } else if native_ms * 100 > reference_ms * 110 {
        "worse"
    } else {
        "equal"
    }
}

fn reference_cells(name: &str) -> (String, String) {
    let reference = reference(name);
    (
        format!(
            "{:.2} MiB/s ({} ms)",
            mib_s(reference.no_cc_ms),
            reference.no_cc_ms
        ),
        format!(
            "{:.2} MiB/s ({} ms)",
            mib_s(reference.reno_ms),
            reference.reno_ms
        ),
    )
}

fn push_row(
    lines: &mut Vec<String>,
    metric: &str,
    native: &str,
    key: &str,
    native_ms: i64,
    method: &str,
) {
    let (no_cc, reno) = reference_cells(key);
    let verdict = verdict_ms(native_ms, reference(key).reno_ms);
    lines.push(format!(
        "| {metric} | {native} | {no_cc} | {reno} | {verdict} | {method} |"
    ));
}

fn main() {
    let mut output: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "--all" => {}
            "--output" => output = args.next().map(PathBuf::from),
            other => {
                eprintln!("unknown argument {other}");
                std::process::exit(2);
            }
        }
    }

    let mut lines = vec![
        "| Metric | raven-tcpip | smoltcp, no CC (frozen) | smoltcp + Reno (frozen, fair) | Verdict vs fair | Method |".to_string(),
        "|---|---|---|---|---|---|".to_string(),
    ];

    for (rtt, key) in [
        (1u64, "lossless 1 ms"),
        (20, "lossless 20 ms"),
        (200, "lossless 200 ms"),
    ] {
        let (run, wall, spread) = five(|| raven_transfer(1024 * 1024, rtt, 0.0, 0.0));
        push_row(
            &mut lines,
            &format!("Goodput, lossless, {rtt} ms RTT"),
            &format!("{:.2} MiB/s ({} ms)", run.goodput_mib_s(), run.virtual_ms),
            key,
            run.virtual_ms,
            &format!(
                "virtual time, 1 MiB, 5 runs; wall median {:.1} ms, spread {:.1} ms",
                wall.as_secs_f64() * 1000.0,
                spread.as_secs_f64() * 1000.0
            ),
        );
    }
    for (loss, key) in [(1.0f32, "1% loss"), (5.0, "5% loss")] {
        let (run, _, _) = five(|| raven_transfer(1024 * 1024, 20, loss, 0.0));
        push_row(
            &mut lines,
            &format!("Goodput, {loss}% loss, 20 ms RTT"),
            &format!(
                "{:.2} MiB/s ({} ms, retrans {} B)",
                run.goodput_mib_s(),
                run.virtual_ms,
                run.retrans_bytes.unwrap_or(0)
            ),
            key,
            run.virtual_ms,
            "virtual time, 1 MiB, 5 runs, seed 131",
        );
    }
    let (run, _, _) = five(|| raven_transfer(1024 * 1024, 20, 0.0, 10.0));
    push_row(
        &mut lines,
        "Goodput, 10% reordering, 20 ms RTT",
        &format!("{:.2} MiB/s ({} ms)", run.goodput_mib_s(), run.virtual_ms),
        "10% reordering",
        run.virtual_ms,
        "virtual time, 1 MiB, 5 runs",
    );

    let (_run, wall, spread) = five(|| raven_transfer(1024 * 1024, 20, 0.0, 0.0));
    let cpu = wall.as_secs_f64() * 1000.0;
    let cpu_verdict = if cpu < CPU_RENO_MS * 0.9 {
        "better"
    } else if cpu > CPU_RENO_MS * 1.1 {
        "worse"
    } else {
        "equal"
    };
    lines.push(format!(
        "| CPU cost per MiB (wall time) | {cpu:.1} ms/MiB | {CPU_NO_CC_MS:.1} ms/MiB | {CPU_RENO_MS:.1} ms/MiB | {cpu_verdict} | median of 5, spread {:.1} ms; references measured on this host before removal |",
        spread.as_secs_f64() * 1000.0
    ));

    let (ttfb, _, _) = five(|| raven_ttfb(20));
    let ttfb_verdict = verdict_ms(ttfb.virtual_ms, TTFB_REF_MS);
    lines.push(format!(
        "| Time to first byte, 20 ms RTT | {} ms virtual | {TTFB_REF_MS} ms virtual | {TTFB_REF_MS} ms virtual (not re-measured) | {ttfb_verdict} | virtual time, 5 identical runs |",
        ttfb.virtual_ms
    ));

    let mut text = String::from(
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n\
         <!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n\
         # Performance baseline\n\n\
         Machine: Linux 5.19.0-50-generic x86_64, Rust 1.97.1 stable, release profile.\n\
         Method: `cargo run -p raven-tcpip-bench --release -- --all --output tests/tcpip/PERFORMANCE.md`.\n\
         Virtual-time metrics are exact and identical across runs; wall-clock metrics are medians\n\
         of five runs with the spread stated and are never asserted as thresholds.\n\n\
         smoltcp was retired in C32. Its columns are frozen measurements of smoltcp 0.14 taken\n\
         with the same harness, scenarios and seeds immediately before removal: once as the old\n\
         `raven-netcore` default (no congestion control) and once with `socket-tcp-reno`, the fair\n\
         congestion-controlled baseline the verdict is computed against (10% tolerance).\n\n",
    );
    text.push_str(&lines.join("\n"));
    text.push_str("\n");
    if let Some(path) = output {
        fs::write(&path, &text).expect("write PERFORMANCE.md");
        println!("wrote {}", path.display());
    } else {
        println!("{text}");
    }
}
