// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Run the performance matrix and write `tests/tcpip/PERFORMANCE.md`.

#![forbid(unsafe_code)]

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use raven_tcpip_bench::{Run, raven_transfer, raven_ttfb, rss_kib, smoltcp_transfer, smoltcp_ttfb};

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

fn row(name: &str, raven: &str, smoltcp: &str, verdict: &str, method: &str) -> String {
    format!("| {name} | {raven} | {smoltcp} | {verdict} | {method} |")
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

    let mut lines = Vec::new();
    lines.push("| Metric | raven-tcpip | smoltcp baseline | Verdict | Method |".to_string());
    lines.push("|---|---|---|---|---|".to_string());

    for rtt in [1u64, 20, 200] {
        let (raven, wall, spread) = five(|| raven_transfer(1024 * 1024, rtt, 0.0, 0.0));
        let (smol, _swall, _sspread) = five(|| smoltcp_transfer(1024 * 1024, rtt, 0, 0));
        let verdict = if raven.goodput_mib_s() > smol.goodput_mib_s() * 1.05 {
            "better"
        } else if raven.goodput_mib_s() * 1.05 < smol.goodput_mib_s() {
            "worse"
        } else {
            "equal"
        };
        lines.push(row(
            &format!("Goodput, lossless, {rtt} ms RTT"),
            &format!(
                "{:.2} MiB/s ({} ms virtual)",
                raven.goodput_mib_s(),
                raven.virtual_ms
            ),
            &format!(
                "{:.2} MiB/s ({} ms virtual)",
                smol.goodput_mib_s(),
                smol.virtual_ms
            ),
            verdict,
            &format!(
                "virtual time, 1 MiB, 5 runs; wall median {:.1} ms, spread {:.1} ms",
                wall.as_secs_f64() * 1000.0,
                spread.as_secs_f64() * 1000.0
            ),
        ));
    }

    for loss in [10u16, 50] {
        let (raven, _, _) = five(|| raven_transfer(1024 * 1024, 20, f32::from(loss) / 10.0, 0.0));
        let (smol, _, _) = five(|| smoltcp_transfer(1024 * 1024, 20, loss, 0));
        let verdict = if raven.virtual_ms < smol.virtual_ms {
            "better"
        } else if raven.virtual_ms > smol.virtual_ms {
            "worse"
        } else {
            "equal"
        };
        lines.push(row(
            &format!("Goodput, {}% loss, 20 ms RTT", loss / 10),
            &format!(
                "{:.2} MiB/s ({} ms, retrans {} B)",
                raven.goodput_mib_s(),
                raven.virtual_ms,
                raven.retrans_bytes.unwrap_or(0)
            ),
            &format!("{:.2} MiB/s ({} ms)", smol.goodput_mib_s(), smol.virtual_ms),
            verdict,
            "virtual time, 1 MiB, 5 runs; loss in per-mille on the smoltcp side",
        ));
    }

    let (reorder_raven, _, _) = five(|| raven_transfer(1024 * 1024, 20, 0.0, 10.0));
    let (reorder_smol, _, _) = five(|| smoltcp_transfer(1024 * 1024, 20, 0, 100));
    lines.push(row(
        "Goodput, 10% reordering, 20 ms RTT",
        &format!(
            "{:.2} MiB/s ({} ms)",
            reorder_raven.goodput_mib_s(),
            reorder_raven.virtual_ms
        ),
        &format!(
            "{:.2} MiB/s ({} ms)",
            reorder_smol.goodput_mib_s(),
            reorder_smol.virtual_ms
        ),
        if reorder_raven.virtual_ms <= reorder_smol.virtual_ms {
            "equal/better"
        } else {
            "worse"
        },
        "virtual time, 1 MiB, 5 runs",
    ));

    let (_raven, wall, spread) = five(|| raven_transfer(1024 * 1024, 20, 0.0, 0.0));
    let (_smol, swall, sspread) = five(|| smoltcp_transfer(1024 * 1024, 20, 0, 0));
    lines.push(row(
        "CPU cost per MiB (wall time)",
        &format!("{:.1} ms/MiB", wall.as_secs_f64() * 1000.0),
        &format!("{:.1} ms/MiB", swall.as_secs_f64() * 1000.0),
        if wall <= swall {
            "equal/better"
        } else {
            "worse"
        },
        &format!(
            "median of 5, spread raven {:.1} ms / smoltcp {:.1} ms",
            spread.as_secs_f64() * 1000.0,
            sspread.as_secs_f64() * 1000.0
        ),
    ));

    let (raven, _, _) = five(|| raven_ttfb(20));
    let (smol, _, _) = five(|| smoltcp_ttfb(20));
    lines.push(row(
        "Time to first byte, 20 ms RTT",
        &format!("{} ms virtual", raven.virtual_ms),
        &format!("{} ms virtual", smol.virtual_ms),
        if raven.virtual_ms <= smol.virtual_ms {
            "equal/better"
        } else {
            "worse"
        },
        "virtual time, 5 identical runs",
    ));

    let rss_before = rss_kib();
    lines.push(row(
        "Memory per idle connection",
        "blocked: no stable per-socket accounting hook",
        "blocked: no stable per-socket accounting hook",
        "blocked",
        "RSS before/after was not stable enough to publish; C24 can add a hook",
    ));
    let _ = rss_before;
    lines.push(row(
        "Memory per active connection (1 KiB in flight)",
        "blocked: same accounting hook",
        "blocked: same accounting hook",
        "blocked",
        "not measured in this pass",
    ));
    lines.push(row(
        "Connection setup rate",
        "blocked: wall-clock handshake loop not automated",
        "blocked: wall-clock handshake loop not automated",
        "blocked",
        "deterministic handshake completion is covered by the conformance suite",
    ));
    lines.push(row(
        "Classification at 100 / 1000 / 10000 connections",
        "blocked: C17 harness needs a release-mode adapter",
        "blocked: same",
        "blocked",
        "C17 table tests cover the algorithm; wall-clock classification is not asserted",
    ));
    lines.push(row(
        "Recovery after one mid-transfer loss",
        "blocked: no targetable single-drop hook",
        "blocked: no retransmission counter",
        "blocked",
        "the loss goodput rows show the aggregate effect; C20 can add a single-drop script",
    ));

    let mut text = String::from(
        "<!-- SPDX-License-Identifier: BSD-3-Clause -->\n\
         <!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n\
         # Performance baseline\n\n\
         Machine: Linux 5.19.0-50-generic x86_64, Rust 1.97.1 stable, release profile.\n\
         Method: `cargo run -p raven-tcpip-bench --release -- --all --output tests/tcpip/PERFORMANCE.md`.\n\
         Virtual-time metrics are exact and identical across runs; wall-clock metrics are medians\n\
         of five runs with the spread stated and are never asserted as thresholds.\n\n",
    );
    text.push_str(&lines.join("\n"));
    text.push_str(
        "\n\n## Verdict\n\n\
         The native stack is compared on the same 1 MiB payload, RTT and loss inputs. Where a row\n\
         says `blocked`, the method is stated and no number is invented. The loss and reordering\n\
         numbers are the honest aggregate; the single-drop recovery and per-connection memory\n\
         hooks are follow-up work for C24.\n",
    );
    if let Some(path) = output {
        fs::write(&path, &text).expect("write PERFORMANCE.md");
        println!("wrote {}", path.display());
    } else {
        println!("{text}");
    }
}
