// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `dtrace` consumer utility.
//!
//! `-l` lists the registered probes, `-e/-d ID` enable or disable one, `-r N` prints up to `N`
//! records, `-s` prints per-CPU ring statistics, `-c` clears the rings and `-u` lists the USDT
//! marker sites. The host build is a stub so `cargo test`/clippy can check the manifest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;

    use raven_dtrace_user::abi::{name, provider};
    use raven_dtrace_user::sys::SyscallKernel;
    use raven_dtrace_user::{Consumer, CpuStats, ProbeInfo, Record, format_record, parse_args};
    use raven_rt::entry;

    entry!(main);

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    fn usage() {
        raven_rt::println!("usage: dtrace [-l] [-e id] [-d id] [-r count] [-s] [-c] [-u] [-h]");
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(_) => {
                usage();
                return 2;
            }
        };
        if options.help {
            usage();
            return 0;
        }
        let consumer = Consumer::new(SyscallKernel);
        let mut status = 0;
        if options.list || options.is_empty() {
            status |= list(&consumer);
        }
        if let Some(probe) = options.enable {
            if consumer.enable(probe).is_ok() {
                raven_rt::println!("dtrace: probe {probe} enabled");
            } else {
                raven_rt::println!("dtrace: cannot enable probe {probe}");
                status = 1;
            }
        }
        if let Some(count) = options.read {
            status |= read(&consumer, count);
        }
        if options.stats {
            status |= stats(&consumer);
        }
        if let Some(probe) = options.disable {
            if consumer.disable(probe).is_ok() {
                raven_rt::println!("dtrace: probe {probe} disabled");
            } else {
                raven_rt::println!("dtrace: cannot disable probe {probe}");
                status = 1;
            }
        }
        if options.clear {
            if consumer.clear().is_ok() {
                raven_rt::println!("dtrace: rings cleared");
            } else {
                raven_rt::println!("dtrace: cannot clear the rings");
                status = 1;
            }
        }
        if options.usdt {
            status |= usdt();
        }
        status
    }

    fn list(consumer: &Consumer<SyscallKernel>) -> i32 {
        let mut probes = [ProbeInfo::default(); 8];
        match consumer.list_probes(&mut probes) {
            Ok(count) => {
                for info in probes.iter().take(count.min(probes.len())) {
                    raven_rt::println!("{} {} {}", info.id, provider(info), name(info));
                }
                0
            }
            Err(error) => {
                raven_rt::println!("dtrace: {}", error.description());
                1
            }
        }
    }

    fn read(consumer: &Consumer<SyscallKernel>, count: usize) -> i32 {
        let mut records = [Record::default(); 32];
        let mut remaining = count;
        while remaining > 0 {
            let read = match consumer.read(&mut records) {
                Ok(read) => read,
                Err(error) => {
                    raven_rt::println!("dtrace: {}", error.description());
                    return 1;
                }
            };
            if read == 0 {
                break;
            }
            for record in records.iter().take(read) {
                let mut writer = Writer;
                if format_record(record, &mut writer).is_err() {
                    return 1;
                }
                raven_rt::println!();
            }
            remaining = remaining.saturating_sub(read);
        }
        0
    }

    fn stats(consumer: &Consumer<SyscallKernel>) -> i32 {
        let mut entries = [CpuStats::default(); 8];
        match consumer.stats(&mut entries) {
            Ok(cpus) => {
                for (index, entry) in entries.iter().take(cpus.min(entries.len())).enumerate() {
                    raven_rt::println!(
                        "cpu {index}: produced={} consumed={} dropped={} capacity={}",
                        entry.produced,
                        entry.consumed,
                        entry.dropped,
                        entry.capacity
                    );
                }
                0
            }
            Err(error) => {
                raven_rt::println!("dtrace: {}", error.description());
                1
            }
        }
    }

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    fn usdt() -> i32 {
        for probe in raven_dtrace_user::USDT_PROBES {
            raven_rt::println!(
                "{} {} args={:?} site=0x{:x} opcode=0x{:x}",
                probe.provider,
                probe.name,
                probe.args,
                probe.site_address(),
                raven_dtrace_user::site_opcode()
            );
        }
        0
    }

    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    fn usdt() -> i32 {
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {}
