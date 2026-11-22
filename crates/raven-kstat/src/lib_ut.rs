// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DTrace provider, filter and human/NDJSON formatting tests.

use super::*;

struct MockKernel {
    stats: [CpuStats; MAX_CPUS],
    count: usize,
    error: Option<Error>,
}

impl KernelStats for MockKernel {
    fn cpu_stats(&self, out: &mut [CpuStats]) -> Result<usize, Error> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let count = self.count.min(out.len()).min(self.stats.len());
        for (index, slot) in out.iter_mut().take(count).enumerate() {
            *slot = self.stats.get(index).copied().unwrap_or_default();
        }
        Ok(self.count)
    }
}

fn two_cpus() -> [CpuStats; MAX_CPUS] {
    let mut stats = [CpuStats::default(); MAX_CPUS];
    stats[0] = CpuStats {
        produced: 10,
        consumed: 3,
        dropped: 1,
        capacity: 4096,
    };
    stats[1] = CpuStats {
        produced: 20,
        consumed: 7,
        dropped: 2,
        capacity: 4096,
    };
    stats
}

fn provider() -> DtraceProvider<MockKernel> {
    DtraceProvider::new(MockKernel {
        stats: two_cpus(),
        count: 2,
        error: None,
    })
}

fn record<'a>(snapshot: &'a Snapshot<CLI_CAPACITY>, name: &str) -> &'a Kstat {
    let expected = KstatName::parse(name).expect("name");
    snapshot
        .iter()
        .find(|record| record.name() == &expected)
        .expect("record")
}

#[test]
fn dtrace_snapshot_has_per_cpu_and_global_records() {
    let mut snapshot = Snapshot::<CLI_CAPACITY>::new();
    provider().snapshot(&mut snapshot).expect("snapshot");
    assert_eq!(snapshot.len(), 2 * 4 + 5);
    assert_eq!(
        record(&snapshot, "dtrace:cpu0:produced").value(),
        Value::Uint64(10)
    );
    assert_eq!(
        record(&snapshot, "dtrace:cpu1:consumed").value(),
        Value::Uint64(7)
    );
    assert_eq!(
        record(&snapshot, "dtrace:global:produced").value(),
        Value::Uint64(30)
    );
    assert_eq!(
        record(&snapshot, "dtrace:global:consumed").value(),
        Value::Uint64(10)
    );
    assert_eq!(
        record(&snapshot, "dtrace:global:dropped").value(),
        Value::Uint64(3)
    );
    assert_eq!(
        record(&snapshot, "dtrace:global:capacity").value(),
        Value::Uint64(8192)
    );
    assert_eq!(
        record(&snapshot, "dtrace:global:cpus").value(),
        Value::Uint64(2)
    );
}

#[test]
fn cpus_are_capped_at_the_provider_limit() {
    let provider = DtraceProvider::new(MockKernel {
        stats: two_cpus(),
        count: 10,
        error: None,
    });
    let mut snapshot = Snapshot::<CLI_CAPACITY>::new();
    provider.snapshot(&mut snapshot).expect("snapshot");
    assert_eq!(snapshot.len(), MAX_CPUS * 4 + 5);
    assert_eq!(
        record(&snapshot, "dtrace:global:cpus").value(),
        Value::Uint64(MAX_CPUS as u64)
    );
}

#[test]
fn provider_reports_a_full_snapshot() {
    let mut snapshot = Snapshot::<2>::new();
    assert_eq!(provider().snapshot(&mut snapshot), Err(Error::Full));
}

#[test]
fn provider_propagates_the_kernel_error() {
    let provider = DtraceProvider::new(MockKernel {
        stats: [CpuStats::default(); MAX_CPUS],
        count: 0,
        error: Some(Error::Kernel),
    });
    let mut snapshot = Snapshot::<CLI_CAPACITY>::new();
    assert_eq!(provider.snapshot(&mut snapshot), Err(Error::Kernel));
}

#[test]
fn global_sums_reject_overflow() {
    let mut stats = [CpuStats::default(); MAX_CPUS];
    stats[0].produced = u64::MAX;
    stats[1].produced = 1;
    let provider = DtraceProvider::new(MockKernel {
        stats,
        count: 2,
        error: None,
    });
    let mut snapshot = Snapshot::<CLI_CAPACITY>::new();
    assert_eq!(provider.snapshot(&mut snapshot), Err(Error::Overflow));
}

#[test]
fn human_output_is_stable() {
    let name = KstatName::parse("dtrace:cpu0:produced").expect("name");
    let mut snapshot = Snapshot::<4>::new();
    snapshot
        .push(Kstat::new(name, Value::Uint64(10), KstatUnit::Count))
        .expect("push");
    let mut text = String::new();
    format_human(&snapshot, None, &mut text).expect("format");
    assert_eq!(
        text,
        "kstat: name=dtrace:cpu0:produced type=uint64 unit=count value=10\n"
    );
}

#[test]
fn ndjson_output_is_stable() {
    let name = KstatName::parse("dtrace:cpu0:produced").expect("name");
    let mut snapshot = Snapshot::<4>::new();
    snapshot
        .push(Kstat::new(name, Value::Uint64(10), KstatUnit::Count))
        .expect("push");
    let mut text = String::new();
    format_ndjson(&snapshot, None, &mut text).expect("format");
    assert_eq!(
        text,
        "{\"name\":\"dtrace:cpu0:produced\",\"type\":\"uint64\",\"unit\":\"count\",\"value\":10}\n"
    );
}

#[test]
fn filter_selects_one_record() {
    let first = KstatName::parse("dtrace:cpu0:produced").expect("first");
    let second = KstatName::parse("dtrace:cpu1:produced").expect("second");
    let mut snapshot = Snapshot::<4>::new();
    snapshot
        .push(Kstat::new(first, Value::Uint64(1), KstatUnit::Count))
        .expect("push");
    snapshot
        .push(Kstat::new(second, Value::Uint64(2), KstatUnit::Count))
        .expect("push");
    let mut text = String::new();
    format_human(&snapshot, Some(&first), &mut text).expect("format");
    assert_eq!(
        text,
        "kstat: name=dtrace:cpu0:produced type=uint64 unit=count value=1\n"
    );
}

#[test]
fn ndjson_text_values_are_escaped() {
    let name = KstatName::parse("text:global:value").expect("name");
    let value = Text::parse_value("a\"b\\c").expect("text");
    let mut snapshot = Snapshot::<4>::new();
    snapshot
        .push(Kstat::new(name, Value::Text(value), KstatUnit::Count))
        .expect("push");
    let mut text = String::new();
    format_ndjson(&snapshot, None, &mut text).expect("format");
    assert_eq!(
        text,
        "{\"name\":\"text:global:value\",\"type\":\"string\",\"unit\":\"count\",\"value\":\"a\\\"b\\\\c\"}\n"
    );
}

#[test]
fn malformed_args_are_rejected() {
    assert_eq!(parse_args(["Bad Name"]), Err(ArgError::InvalidName));
    assert_eq!(parse_args(["-x"]), Err(ArgError::Unknown));
    assert_eq!(
        parse_args(["dtrace:cpu0:produced", "dtrace:cpu1:produced"]),
        Err(ArgError::DuplicateName)
    );
}

#[test]
fn ndjson_and_filter_args_are_parsed() {
    let options = parse_args(["-n", "dtrace:cpu0:produced"]).expect("args");
    assert!(options.ndjson);
    assert!(!options.help);
    assert_eq!(
        options.filter,
        Some(KstatName::parse("dtrace:cpu0:produced").expect("name"))
    );
}
