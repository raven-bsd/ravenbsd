// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Timer state-machine tests.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use raven_libc_abi::time::{CLOCK_MONOTONIC, CLOCK_REALTIME};

fn spec(seconds: i64, nanos: i64) -> ITimerSpec {
    ITimerSpec {
        interval: Timespec::ZERO,
        value: Timespec {
            tv_sec: seconds,
            tv_nsec: nanos,
        },
    }
}

fn now(mono_ns: u64) -> Now {
    Now {
        clock: Timespec::ZERO,
        mono_ns,
    }
}

#[test]
fn create_returns_the_kernel_op_and_id() {
    let mut table = TimerTable::new(32);
    let (id, op) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    assert_eq!(
        op,
        KernelOp::Create {
            clock: ClockId::Realtime
        }
    );
    assert_eq!(id.0 & 0xffff_ffff, 0);
    assert_eq!(table.len(), 1);
}

#[test]
fn create_rejects_cpu_and_unknown_clocks() {
    let mut table = TimerTable::new(32);
    assert_eq!(
        table.create(2, Notify::None),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(
        table.create(99, Notify::None),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
}

#[test]
fn create_validates_the_signal_number() {
    let mut table = TimerTable::new(32);
    assert_eq!(
        table.create(CLOCK_REALTIME, Notify::Signal { signo: 0, value: 0 }),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(
        table.create(
            CLOCK_REALTIME,
            Notify::Signal {
                signo: 65,
                value: 0
            }
        ),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
    assert!(
        table
            .create(
                CLOCK_REALTIME,
                Notify::Signal {
                    signo: 64,
                    value: 0
                }
            )
            .is_ok()
    );
}

#[test]
fn a_full_table_is_eagain() {
    let mut table = TimerTable::new(1);
    table.create(CLOCK_REALTIME, Notify::None).expect("first");
    assert_eq!(
        table.create(CLOCK_REALTIME, Notify::None),
        Err(errno(raven_libc_abi::errno::EAGAIN))
    );
}

#[test]
fn a_relative_one_shot_counts_down() {
    let mut table = TimerTable::new(32);
    let (id, _) = table
        .create(
            CLOCK_REALTIME,
            Notify::Signal {
                signo: 14,
                value: 7,
            },
        )
        .expect("create");
    let (old, op) = table
        .settime(id, false, spec(1, 500_000_000), now(1_000))
        .expect("settime");
    assert_eq!(old, ITimerSpec::default());
    assert_eq!(
        op,
        KernelOp::Set {
            initial_ns: 1_500_000_000,
            interval_ns: 0
        }
    );
    assert_eq!(table.gettime(id, 1_000).expect("get").value.tv_sec, 1);
    assert_eq!(
        table.gettime(id, 1_000_000_000).expect("get").value.tv_nsec,
        500_001_000
    );
    assert_eq!(
        table.gettime(id, 3_000_000_000).expect("get").value,
        Timespec::ZERO
    );
    assert_eq!(
        table.on_event(id.0, 3_000_000_000),
        Some(Action::Signal {
            signo: 14,
            value: 7
        })
    );
    assert_eq!(
        table.gettime(id, 3_000_000_000).expect("get").value,
        Timespec::ZERO
    );
}

#[test]
fn a_periodic_timer_reports_the_interval_value() {
    let mut table = TimerTable::new(32);
    let (id, _) = table
        .create(CLOCK_MONOTONIC, Notify::Thread)
        .expect("create");
    let mut value = spec(1, 0);
    value.interval = Timespec {
        tv_sec: 0,
        tv_nsec: 500_000_000,
    };
    table.settime(id, false, value, now(0)).expect("settime");
    assert_eq!(
        table.gettime(id, 500_000_000).expect("get").value.tv_nsec,
        500_000_000
    );
    assert_eq!(
        table.gettime(id, 1_000_000_000).expect("get").value.tv_nsec,
        500_000_000
    );
    assert_eq!(
        table.gettime(id, 2_500_000_000).expect("get").value.tv_nsec,
        500_000_000
    );
    assert_eq!(
        table.on_event(id.0, 1_000_000_000),
        Some(Action::StartThread)
    );
    assert!(table.gettime(id, 1_000_000_000).expect("get").value.tv_nsec > 0);
}

#[test]
fn absolute_arming_in_the_past_becomes_one_nanosecond() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    let mut past = now(5_000_000_000);
    past.clock = Timespec {
        tv_sec: 5,
        tv_nsec: 0,
    };
    let (_, op) = table.settime(id, true, spec(1, 0), past).expect("settime");
    assert_eq!(
        op,
        KernelOp::Set {
            initial_ns: 1,
            interval_ns: 0
        }
    );
}

#[test]
fn absolute_arming_in_the_future_uses_the_difference() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    let mut now = now(1_000_000_000);
    now.clock = Timespec {
        tv_sec: 1,
        tv_nsec: 0,
    };
    let (_, op) = table.settime(id, true, spec(3, 0), now).expect("settime");
    assert_eq!(
        op,
        KernelOp::Set {
            initial_ns: 2_000_000_000,
            interval_ns: 0
        }
    );
}

#[test]
fn zero_disarms() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    table.settime(id, false, spec(1, 0), now(0)).expect("arm");
    let (old, op) = table
        .settime(id, false, spec(0, 0), now(0))
        .expect("disarm");
    assert_eq!(old.value.tv_sec, 1);
    assert_eq!(
        op,
        KernelOp::Set {
            initial_ns: 0,
            interval_ns: 0
        }
    );
    assert_eq!(table.gettime(id, 2).expect("get"), ITimerSpec::default());
}

#[test]
fn bad_specs_are_rejected() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    assert_eq!(
        table.settime(id, false, spec(0, 1_000_000_000), now(0)),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(
        table.settime(id, false, spec(-1, 0), now(0)),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
}

#[test]
fn delete_invalidates_the_generation() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    assert_eq!(table.delete(id), Ok(KernelOp::Close));
    assert_eq!(
        table.gettime(id, 0),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
    assert_eq!(table.on_event(id.0, 0), None);
    let (reused, _) = table.create(CLOCK_REALTIME, Notify::None).expect("reuse");
    assert_ne!(reused.0, id.0);
}

#[test]
fn an_unknown_id_is_rejected() {
    let table = TimerTable::new(32);
    assert_eq!(
        table.gettime(TimerId(7), 0),
        Err(errno(raven_libc_abi::errno::EINVAL))
    );
}

#[test]
fn reset_after_fork_empties_the_table() {
    let mut table = TimerTable::new(32);
    table.create(CLOCK_REALTIME, Notify::None).expect("create");
    table.reset_after_fork();
    assert!(table.is_empty());
}

#[test]
fn overrun_stays_zero_until_pg19() {
    let mut table = TimerTable::new(32);
    let (id, _) = table.create(CLOCK_REALTIME, Notify::None).expect("create");
    assert_eq!(table.getoverrun(id), 0);
}
