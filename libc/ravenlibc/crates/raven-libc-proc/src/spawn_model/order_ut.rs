// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the attribute application order.

use super::*;
use crate::consts::{POSIX_SPAWN_CLOEXEC_DEFAULT, POSIX_SPAWN_USEVFORK};

#[test]
fn an_empty_attribute_has_no_steps() {
    assert!(attr_steps(&SpawnAttr::new()).is_empty());
}

#[test]
fn setsid_is_first() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETSID).expect("flags");
    assert_eq!(attr_steps(&attr), vec![AttrStep::SetSid]);
}

#[test]
fn setpgroup_carries_its_value() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETPGROUP).expect("flags");
    attr.set_pgroup(7);
    assert_eq!(attr_steps(&attr), vec![AttrStep::SetPgroup(7)]);
}

#[test]
fn setsid_precedes_setpgroup() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETSID | POSIX_SPAWN_SETPGROUP)
        .expect("flags");
    assert_eq!(
        attr_steps(&attr),
        vec![AttrStep::SetSid, AttrStep::SetPgroup(0)]
    );
}

#[test]
fn scheduler_overrides_schedparam() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETSCHEDULER | POSIX_SPAWN_SETSCHEDPARAM)
        .expect("flags");
    attr.set_sched_policy(1);
    attr.set_sched_priority(9);
    assert_eq!(
        attr_steps(&attr),
        vec![AttrStep::SetScheduler { policy: 1, prio: 9 }]
    );
}

#[test]
fn schedparam_alone_is_kept() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_SETSCHEDPARAM).expect("flags");
    attr.set_sched_priority(9);
    assert_eq!(attr_steps(&attr), vec![AttrStep::SetSchedParam(9)]);
}

#[test]
fn resetids_signal_mask_and_defaults_follow() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_RESETIDS | POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_SETSIGDEF)
        .expect("flags");
    attr.set_sigmask([1; 128]);
    attr.set_sigdefault([2; 128]);
    assert_eq!(
        attr_steps(&attr),
        vec![
            AttrStep::ResetIds,
            AttrStep::SigMask([1; 128]),
            AttrStep::SigDefault([2; 128])
        ]
    );
}

#[test]
fn usevfork_and_cloexec_default_are_not_attr_steps() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_USEVFORK | POSIX_SPAWN_CLOEXEC_DEFAULT)
        .expect("flags");
    assert!(attr_steps(&attr).is_empty());
}

#[test]
fn the_full_order_is_sid_pgroup_sched_ids_mask_default() {
    let mut attr = SpawnAttr::new();
    attr.set_flags(
        POSIX_SPAWN_SETSID
            | POSIX_SPAWN_SETPGROUP
            | POSIX_SPAWN_SETSCHEDULER
            | POSIX_SPAWN_RESETIDS
            | POSIX_SPAWN_SETSIGMASK
            | POSIX_SPAWN_SETSIGDEF,
    )
    .expect("flags");
    let steps = attr_steps(&attr);
    assert!(matches!(steps[0], AttrStep::SetSid));
    assert!(matches!(steps[1], AttrStep::SetPgroup(_)));
    assert!(matches!(steps[2], AttrStep::SetScheduler { .. }));
    assert!(matches!(steps[3], AttrStep::ResetIds));
    assert!(matches!(steps[4], AttrStep::SigMask(_)));
    assert!(matches!(steps[5], AttrStep::SigDefault(_)));
    assert_eq!(steps.len(), 6);
}
