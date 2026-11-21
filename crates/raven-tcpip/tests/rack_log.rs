// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public-surface integration test for the K329 RACK logging slice.

use raven_tcpip::{RackLog, RackLogCounter};

#[test]
fn public_logging_surface_records_timers_rtt_output_and_counters() {
    let mut log = RackLog::new();
    let nodes = log.rack_init_sysctls();
    assert_eq!(nodes.len(), 4);
    assert!(log.rack_log_to_start(1000, 2000, 30, 1));
    assert!(log.rack_log_rtt_sample(120));
    assert!(log.rack_log_output(1460, 9000, 0x18, 0, 1000, 0, 0, 1460));
    assert!(log.rack_log_ack(9000, 0, 0, &mut false, &mut 0));
    assert_eq!(log.counter(RackLogCounter::TotalBytes), 1460);
    assert_eq!(log.counter(RackLogCounter::OutBytes), 1460);
    assert_eq!(log.counter(RackLogCounter::Tlp), 1);
    assert_eq!(log.event_count(), 5);
    assert_eq!(log.sysctl_rack_clear(1).unwrap(), 2921);
    assert_eq!(log.counter(RackLogCounter::TotalBytes), 0);
    assert_eq!(log.rack_counter_destroy(), 4);
}
