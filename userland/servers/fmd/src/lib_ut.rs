// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fmd policy engine.

use super::*;
use raven_proto_fm::Severity;

fn event(class: &str, resource: &str) -> Ereport {
    Ereport::new(class, Severity::Major, resource, &[1, 2, 3], 100).expect("event")
}

#[test]
fn declared_policy_drives_diagnosis() {
    let mut fmd = Fmd::new();
    assert!(fmd.add_policy("ereport.io.mmio.bad", Action::Retire));
    assert!(fmd.add_policy("ereport.io.ecc", Action::Degrade));
    let mmio = event("ereport.io.mmio.bad", "pci0:1:0");
    let diagnosis = fmd.diagnose(&mmio, 10);
    assert!(diagnosis.accepted);
    assert_eq!(diagnosis.action, Action::Retire);
    assert_eq!(diagnosis.case, 1);
    assert_eq!(diagnosis.resource.as_str(), Some("pci0:1:0"));
    let ecc = event("ereport.io.ecc", "dimm0");
    let diagnosis = fmd.diagnose(&ecc, 11);
    assert_eq!(diagnosis.action, Action::Degrade);
    assert_eq!(fmd.audit_count(), 2);
    assert_eq!(
        fmd.audit(0).map(|record| record.action),
        Some(Action::Retire)
    );
    assert_eq!(
        fmd.audit(1).map(|record| record.action),
        Some(Action::Degrade)
    );
}

#[test]
fn unknown_class_is_ignored_but_audited() {
    let mut fmd = Fmd::new();
    let diagnosis = fmd.diagnose(&event("ereport.io.unknown", "r"), 1);
    assert_eq!(diagnosis.action, Action::Ignore);
    assert!(diagnosis.accepted);
    assert_eq!(fmd.audit_count(), 1);
}

#[test]
fn malformed_rejection_is_audited() {
    let mut fmd = Fmd::new();
    assert!(fmd.reject(7));
    let record = fmd.audit(0).expect("record");
    assert!(!record.accepted);
    assert_eq!(record.action, Action::Ignore);
    assert_eq!(record.class.as_str(), Some("ereport.malformed"));
}

#[test]
fn policy_and_audit_are_bounded() {
    let mut fmd = Fmd::new();
    for index in 0..MAX_POLICY {
        assert!(
            fmd.add_policy("ereport.io.mmio.bad", Action::Retire),
            "{index}"
        );
    }
    assert!(!fmd.add_policy("ereport.io.other", Action::Ignore));
    for index in 0..MAX_AUDIT + 4 {
        fmd.diagnose(&event("ereport.io.mmio.bad", "r"), index as u64);
    }
    assert_eq!(fmd.audit_count(), MAX_AUDIT + 4);
    assert!(fmd.audit(MAX_AUDIT).is_none());
}
