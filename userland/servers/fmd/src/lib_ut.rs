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
fn declared_policy_file_is_the_policy() {
    let fmd = Fmd::from_policy(include_str!("../policy.conf")).expect("policy file");
    assert_eq!(fmd.policy_count(), 2);
}

#[test]
fn malformed_policy_documents_are_rejected() {
    assert_eq!(
        Fmd::from_policy("ereport.io.ecc").err(),
        Some(PolicyError::BadLine)
    );
    assert_eq!(
        Fmd::from_policy("ereport.io.ecc retire extra").err(),
        Some(PolicyError::BadLine)
    );
    assert_eq!(
        Fmd::from_policy("ereport.io.ecc replace").err(),
        Some(PolicyError::BadAction)
    );
    assert_eq!(
        Fmd::from_policy("bad\u{7f}class retire").err(),
        Some(PolicyError::BadClass)
    );
    let too_many = concat!(
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
        "ereport.io.ecc degrade\n",
    );
    assert_eq!(Fmd::from_policy(too_many).err(), Some(PolicyError::TooMany));
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
fn audit_after_wrap_returns_oldest_first() {
    let mut fmd = Fmd::new();
    for index in 0..MAX_AUDIT + 4 {
        fmd.diagnose(&event("ereport.io.unknown", "r"), index as u64);
    }
    assert_eq!(fmd.audit_count(), MAX_AUDIT + 4);
    for index in 0..MAX_AUDIT {
        assert_eq!(
            fmd.audit(index).map(|record| record.timestamp),
            Some(index as u64 + 4),
            "retained audit slot {index}"
        );
    }
    assert!(fmd.audit(MAX_AUDIT).is_none());
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
