// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Collect result JSON files into `results/summary.md`.

use std::fmt::Write as _;
use std::fs;

use anyhow::{Context, Result, bail};

use crate::result::{BenchResult, host_info, read_all_results, results_dir};

pub(crate) fn run() -> Result<()> {
    let results = read_all_results()?;
    if results.is_empty() {
        bail!("no result JSON files; run the measurements first");
    }
    let host = results
        .first()
        .map(|result| result.host.clone())
        .unwrap_or_else(host_info);

    let mut text = String::new();
    text.push_str("<!-- SPDX-License-Identifier: BSD-3-Clause -->\n");
    text.push_str("<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->\n\n");
    text.push_str("# bench-host results\n\n");
    let _ = writeln!(
        text,
        "Host: {} CPUs, {} (`{}`), kernel {}, QEMU {}, KVM {}, governor {}.\n",
        host.cpus, host.cpu, host.arch, host.kernel, host.qemu, host.kvm, host.governor
    );
    text.push_str("| Measurement | Unit | n | Median | p99 | Min | Max | Note |\n");
    text.push_str("|---|---|---:|---:|---:|---:|---:|---|\n");
    for result in &results {
        let _ = writeln!(
            text,
            "| {} | {} | {} | {} | {} | {} | {} | {} |",
            escape(&result.name),
            escape(&result.unit),
            result.n,
            value(result, result.median),
            value(result, result.p99),
            value(result, result.min),
            value(result, result.max),
            result.note.as_deref().map(escape).unwrap_or_default()
        );
    }
    text.push_str(
        "\nAll timings are wall-clock nanoseconds. `n=0` means the item was not measurable; \
         see the note and the report.\n",
    );

    let directory = results_dir();
    fs::create_dir_all(&directory)
        .with_context(|| format!("cannot create {}", directory.display()))?;
    let path = directory.join("summary.md");
    fs::write(&path, text).with_context(|| format!("cannot write {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}

fn value(result: &BenchResult, value: f64) -> String {
    if result.n == 0 {
        String::from("-")
    } else if result.unit == "bytes" {
        format!("{value:.0}")
    } else {
        format!("{value:.1}")
    }
}

fn escape(text: &str) -> String {
    text.replace('|', "\\|")
}
