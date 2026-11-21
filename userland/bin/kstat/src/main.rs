// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The RavenBSD `kstat` utility.
//!
//! The tool reads the existing bounded `DTRACE_STATS` (0x94) per-CPU ring statistics, wraps them
//! in the typed `module:instance:name` model and prints human or NDJSON output. It also injects a
//! known `CLOCK_GET` count to prove the counter delta and reads the statistics while a worker
//! thread makes syscalls. The host build is a stub so clippy can check the manifest.

#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]
#![forbid(unsafe_code)]

#[cfg(target_os = "none")]
mod program {
    use core::fmt;
    use core::sync::atomic::{AtomicBool, Ordering};

    use raven_abi::types::{Timespec, clock};
    use raven_dtrace_user::Consumer;
    use raven_dtrace_user::abi::{
        CpuStats, ProbeInfo, name as probe_name, provider as probe_provider,
    };
    use raven_dtrace_user::consumer::TraceKernel;
    use raven_dtrace_user::sys::SyscallKernel;
    use raven_kstat::{
        ArgError, CLI_CAPACITY, DtraceProvider, Error, KernelStats, KstatName, MAX_CPUS, Snapshot,
        format_human, format_ndjson, parse_args,
    };
    use raven_rt::entry;

    entry!(main);

    const ACCURACY_CALLS: usize = 64;

    static BUSY: AtomicBool = AtomicBool::new(true);

    struct Writer;

    impl fmt::Write for Writer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            raven_rt::print!("{text}");
            Ok(())
        }
    }

    struct FixedBuffer {
        bytes: [u8; 256],
        len: usize,
    }

    impl FixedBuffer {
        const fn new() -> Self {
            Self {
                bytes: [0; 256],
                len: 0,
            }
        }

        fn as_str(&self) -> &str {
            let bytes = self.bytes.get(..self.len).unwrap_or(&[]);
            core::str::from_utf8(bytes).unwrap_or("")
        }
    }

    impl fmt::Write for FixedBuffer {
        fn write_str(&mut self, text: &str) -> fmt::Result {
            let end = self.len.checked_add(text.len()).ok_or(fmt::Error)?;
            if end > self.bytes.len() {
                return Err(fmt::Error);
            }
            let target = self.bytes.get_mut(self.len..end).ok_or(fmt::Error)?;
            target.copy_from_slice(text.as_bytes());
            self.len = end;
            Ok(())
        }
    }

    struct Dtrace;

    impl KernelStats for Dtrace {
        fn cpu_stats(&self, out: &mut [CpuStats]) -> Result<usize, Error> {
            SyscallKernel.stats(out).map_err(|_| Error::Kernel)
        }
    }

    fn worker(_argument: usize) {
        while BUSY.load(Ordering::Relaxed) {
            let _ = raven_rt::clock_get(clock::MONOTONIC);
        }
    }

    fn usage() {
        raven_rt::println!("usage: kstat [-n|--ndjson] [-h|--help] [module:instance:name]");
    }

    fn find_probe(consumer: &Consumer<SyscallKernel>) -> Option<u32> {
        let mut probes = [ProbeInfo::default(); 8];
        let count = consumer.list_probes(&mut probes).ok()?;
        let count = count.min(probes.len());
        let mut first = None;
        for info in probes.iter().take(count) {
            if first.is_none() {
                first = Some(info.id);
            }
            if probe_provider(info) == "syscall" && probe_name(info) == "entry" {
                return Some(info.id);
            }
        }
        first
    }

    fn timespec_ns(time: Timespec) -> u64 {
        let seconds = u64::try_from(time.tv_sec).unwrap_or(0);
        let nanoseconds = u64::try_from(time.tv_nsec).unwrap_or(0);
        seconds
            .saturating_mul(1_000_000_000)
            .saturating_add(nanoseconds)
    }

    fn main(args: raven_rt::Args) -> i32 {
        let options = match parse_args(args.iter().skip(1)) {
            Ok(options) => options,
            Err(error) => {
                match error {
                    ArgError::Unknown => raven_rt::println!("kstat: unknown option"),
                    ArgError::DuplicateName => raven_rt::println!("kstat: duplicate name"),
                    ArgError::InvalidName => raven_rt::println!("kstat: invalid name"),
                }
                usage();
                return 2;
            }
        };
        if options.help {
            usage();
            return 0;
        }

        let consumer = Consumer::new(SyscallKernel);
        let Some(probe) = find_probe(&consumer) else {
            raven_rt::println!("kstat: fail no-probe");
            return 1;
        };
        if consumer.enable(probe).is_err() {
            raven_rt::println!("kstat: fail enable");
            return 1;
        }

        let mut baseline = [CpuStats::default(); MAX_CPUS];
        let base_cpus = match Dtrace.cpu_stats(&mut baseline) {
            Ok(cpus) => cpus,
            Err(_) => {
                raven_rt::println!("kstat: fail baseline");
                let _ = consumer.disable(probe);
                return 1;
            }
        };

        for _ in 0..ACCURACY_CALLS {
            let _ = raven_rt::clock_get(clock::MONOTONIC);
        }

        let mut final_stats = [CpuStats::default(); MAX_CPUS];
        let final_cpus = match Dtrace.cpu_stats(&mut final_stats) {
            Ok(cpus) => cpus,
            Err(_) => {
                raven_rt::println!("kstat: fail final");
                let _ = consumer.disable(probe);
                return 1;
            }
        };
        let cpus = base_cpus.min(final_cpus).min(MAX_CPUS);
        let mut delta = 0u64;
        for index in 0..cpus {
            let base = baseline.get(index).map(|entry| entry.produced).unwrap_or(0);
            let final_value = final_stats
                .get(index)
                .map(|entry| entry.produced)
                .unwrap_or(0);
            delta = delta.saturating_add(final_value.saturating_sub(base));
        }
        if delta < ACCURACY_CALLS as u64 {
            raven_rt::println!("kstat: fail accuracy delta={delta} min={ACCURACY_CALLS}");
            let _ = consumer.disable(probe);
            return 1;
        }
        raven_rt::println!("kstat: accuracy delta={delta} min={ACCURACY_CALLS} ok");

        match KstatName::parse("Bad Name") {
            Err(_) => raven_rt::println!("kstat: bad-name rejected"),
            Ok(_) => {
                raven_rt::println!("kstat: fail bad-name accepted");
                let _ = consumer.disable(probe);
                return 1;
            }
        }

        let start = raven_rt::clock_get(clock::MONOTONIC).unwrap_or_default();
        let provider = DtraceProvider::new(Dtrace);
        let mut snapshot = Snapshot::<CLI_CAPACITY>::new();
        if provider.snapshot(&mut snapshot).is_err() {
            raven_rt::println!("kstat: fail read");
            let _ = consumer.disable(probe);
            return 1;
        }
        let end = raven_rt::clock_get(clock::MONOTONIC).unwrap_or_default();
        let read_ns = timespec_ns(end).saturating_sub(timespec_ns(start));
        raven_rt::println!("kstat: read_ns={read_ns}");

        let mut writer = Writer;
        let formatted = if options.ndjson {
            format_ndjson(&snapshot, options.filter.as_ref(), &mut writer)
        } else {
            format_human(&snapshot, options.filter.as_ref(), &mut writer)
        };
        if formatted.is_err() {
            raven_rt::println!("kstat: fail format");
            let _ = consumer.disable(probe);
            return 1;
        }

        if let Some(first) = snapshot.iter().next() {
            let mut buffer = FixedBuffer::new();
            if format_ndjson(&snapshot, Some(first.name()), &mut buffer).is_ok() {
                let text = buffer.as_str().trim_end_matches('\n');
                raven_rt::println!("kstat: ndjson {text}");
            }
        }

        BUSY.store(true, Ordering::Relaxed);
        match raven_rt::thread::spawn(worker, 0, 8192) {
            Ok(thread) => {
                for _ in 0..3 {
                    let mut busy = Snapshot::<CLI_CAPACITY>::new();
                    let _ = provider.snapshot(&mut busy);
                }
                BUSY.store(false, Ordering::Relaxed);
                let _ = thread.join();
                raven_rt::println!("kstat: busy ok");
            }
            Err(_) => {
                BUSY.store(false, Ordering::Relaxed);
                raven_rt::println!("kstat: fail busy-spawn");
                let _ = consumer.disable(probe);
                return 1;
            }
        }

        let _ = consumer.disable(probe);
        raven_rt::println!("kstat: ok");
        0
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    eprintln!("kstat: this utility runs on RavenBSD");
}
