// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::dispositions::{DispTable, Disposition, SA_RESTART};
use crate::host_ut::HostSigKernel;
use crate::mask;
use raven_libc_abi::generated::signal;

#[test]
fn signal_installs_restart_and_never_resets() {
    let table = DispTable::new();
    let first = signal(&table, 2, 0x1234).unwrap();
    assert_eq!(first, 0);
    let stored = table.read(2);
    assert_eq!(stored.handler, 0x1234);
    assert_eq!(stored.flags, SA_RESTART);
    let previous = signal(&table, 2, 0x5678).unwrap();
    assert_eq!(previous, 0x1234);
    assert_eq!(table.read(2).handler, 0x5678);
}

#[test]
fn signal_rejects_internal_and_uncatchable() {
    let table = DispTable::new();
    assert_eq!(signal(&table, 32, 0x1), Err(EINVAL_ERR));
    assert_eq!(signal(&table, 9, 0x1), Err(EINVAL_ERR));
}

#[test]
fn sigignore_installs_sig_ign() {
    let table = DispTable::new();
    sigignore(&table, 3).unwrap();
    assert_eq!(table.read(3), Disposition::IGNORE);
}

#[test]
fn sighold_and_sigrelse_toggle_the_mask() {
    let kernel = HostSigKernel::new();
    mask::set_mask(&kernel, 0);
    sighold(&kernel, 10).unwrap();
    assert!(mask::is_blocked(mask::current_mask(&kernel), 10));
    sigrelse(&kernel, 10).unwrap();
    assert!(!mask::is_blocked(mask::current_mask(&kernel), 10));
}

#[test]
fn sigset_hold_and_restore() {
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    signal(&table, 4, 0x1111).unwrap();
    assert_eq!(sigset(&kernel, &table, 4, SIG_HOLD).unwrap(), 0x1111);
    assert!(mask::is_blocked(mask::current_mask(&kernel), 4));
    assert_eq!(sigset(&kernel, &table, 4, 0x2222).unwrap(), SIG_HOLD);
    assert!(!mask::is_blocked(mask::current_mask(&kernel), 4));
    assert_eq!(table.read(4).handler, 0x2222);
}

#[test]
fn siginterrupt_toggles_sa_restart() {
    let table = DispTable::new();
    signal(&table, 5, 0x10).unwrap();
    siginterrupt(&table, 5, 1).unwrap();
    assert_eq!(table.read(5).flags & SA_RESTART, 0);
    siginterrupt(&table, 5, 0).unwrap();
    assert_ne!(table.read(5).flags & SA_RESTART, 0);
}

#[test]
fn number_calls_and_realtime_bounds() {
    let kernel = HostSigKernel::new();
    let table = DispTable::new();
    mask::set_mask(&kernel, 0);
    assert_eq!(
        pthread_sigmask_number(&kernel, signal::SIG_BLOCK, None, None),
        0
    );
    assert_eq!(sigpause(&kernel, &table, 32), EINVAL_ERR);
    assert_eq!(sigrtmin(), 35);
    assert_eq!(sigrtmax(), 64);
}
