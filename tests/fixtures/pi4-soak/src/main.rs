// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `/init` for the `pi4-soak` qtest fixture.
//!
//! The guest runs the same deterministic model loop as the host test and prints the counters the
//! qtest and the board evidence script consume. No MMIO, DMA or board resource is touched.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use raven_rt::entry;
    use raven_rt::println;

    entry!(main);

    fn main(_args: raven_rt::Args) -> i32 {
        println!(
            "pi4-soak: start iterations={}",
            pi4_soak::DEFAULT_ITERATIONS
        );
        let report = match pi4_soak::run(pi4_soak::DEFAULT_ITERATIONS) {
            Ok(report) => report,
            Err(error) => {
                println!("pi4-soak: failed {:?}", error);
                return 1;
            }
        };
        println!(
            "pi4-soak: genet rx={} tx={} owned={} rx_errors={} tx_errors={} link_events={} irq={}",
            report.rx_completed,
            report.tx_completed,
            report.owned_checks,
            report.rx_errors,
            report.tx_errors,
            report.link_events,
            report.irq_checks
        );
        println!(
            "pi4-soak: recovery resets={} faults={}",
            report.recovery_resets,
            if report.recovery_fault { 1 } else { 0 }
        );
        println!(
            "pi4-soak: audio periods={} frames={} underruns={} writes={} bytes={}",
            report.audio_periods,
            report.audio_frames,
            report.audio_underruns,
            report.audio_writes,
            report.audio_bytes
        );
        println!("pi4-soak: pass");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
