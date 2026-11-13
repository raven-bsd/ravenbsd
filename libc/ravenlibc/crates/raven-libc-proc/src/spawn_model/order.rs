// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Attribute application order.

use alloc::vec::Vec;

use crate::consts::{
    POSIX_SPAWN_RESETIDS, POSIX_SPAWN_SETPGROUP, POSIX_SPAWN_SETSCHEDPARAM,
    POSIX_SPAWN_SETSCHEDULER, POSIX_SPAWN_SETSID, POSIX_SPAWN_SETSIGDEF, POSIX_SPAWN_SETSIGMASK,
};
use crate::spawn_model::api::SpawnAttr;

/// One ordered attribute step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AttrStep {
    /// `setsid`.
    SetSid,
    /// `setpgid`.
    SetPgroup(i32),
    /// `sched_setscheduler`.
    SetScheduler {
        /// Scheduler policy.
        policy: i32,
        /// Scheduler priority.
        prio: i32,
    },
    /// `sched_setparam`.
    SetSchedParam(i32),
    /// Reset the effective ids.
    ResetIds,
    /// Set the child signal mask.
    SigMask([u8; 128]),
    /// Reset selected signals to their defaults.
    SigDefault([u8; 128]),
}

/// Build the ordered attribute steps for `attr`.
#[must_use]
pub fn attr_steps(attr: &SpawnAttr) -> Vec<AttrStep> {
    let mut steps = Vec::new();
    if attr.flags & POSIX_SPAWN_SETSID != 0 {
        steps.push(AttrStep::SetSid);
    }
    if attr.flags & POSIX_SPAWN_SETPGROUP != 0 {
        steps.push(AttrStep::SetPgroup(attr.pgroup));
    }
    if attr.flags & POSIX_SPAWN_SETSCHEDULER != 0 {
        steps.push(AttrStep::SetScheduler {
            policy: attr.sched_policy,
            prio: attr.sched_priority,
        });
    } else if attr.flags & POSIX_SPAWN_SETSCHEDPARAM != 0 {
        steps.push(AttrStep::SetSchedParam(attr.sched_priority));
    }
    if attr.flags & POSIX_SPAWN_RESETIDS != 0 {
        steps.push(AttrStep::ResetIds);
    }
    if attr.flags & POSIX_SPAWN_SETSIGMASK != 0 {
        steps.push(AttrStep::SigMask(attr.sigmask));
    }
    if attr.flags & POSIX_SPAWN_SETSIGDEF != 0 {
        steps.push(AttrStep::SigDefault(attr.sigdefault));
    }
    steps
}

#[cfg(test)]
#[path = "order_ut.rs"]
mod tests;
