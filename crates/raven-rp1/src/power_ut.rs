// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the power-domain model.

use super::*;

fn model() -> PowerModel {
    let mut model = PowerModel::new();
    assert!(
        model
            .insert(DomainSpec {
                id: 1,
                parent: None
            })
            .is_ok()
    );
    assert!(
        model
            .insert(DomainSpec {
                id: 2,
                parent: Some(1)
            })
            .is_ok()
    );
    assert!(
        model
            .insert(DomainSpec {
                id: 3,
                parent: Some(1)
            })
            .is_ok()
    );
    model
}

#[test]
fn a_domain_starts_off() {
    let model = model();
    assert_eq!(model.len(), 3);
    assert!(!model.is_empty());
    assert_eq!(model.state(1), Some(DomainState::Off));
    assert_eq!(model.state(99), None);
}

#[test]
fn a_child_needs_its_parent_on() {
    let mut model = model();
    assert_eq!(
        model.power_on(2),
        Err(PowerError::ParentOff { id: 2, parent: 1 })
    );
    assert_eq!(model.power_on(1), Ok(PowerAction::PoweredOn(1)));
    assert_eq!(model.power_on(1), Ok(PowerAction::AlreadyOn(1)));
    assert_eq!(model.power_on(2), Ok(PowerAction::PoweredOn(2)));
    assert_eq!(model.state(2), Some(DomainState::On));
}

#[test]
fn users_prevent_a_power_off() {
    let mut model = model();
    assert_eq!(model.power_on(1), Ok(PowerAction::PoweredOn(1)));
    assert_eq!(model.acquire(1), Ok(()));
    assert_eq!(
        model.power_off(1),
        Err(PowerError::InUse { id: 1, users: 1 })
    );
    assert_eq!(model.release(1), Ok(()));
    assert_eq!(model.release(1), Err(PowerError::UsersUnderflow(1)));
    assert_eq!(model.power_off(1), Ok(PowerAction::PoweredOff(1)));
    assert_eq!(model.power_off(1), Ok(PowerAction::AlreadyOff(1)));
}

#[test]
fn an_acquire_needs_a_powered_domain() {
    let mut model = model();
    assert_eq!(model.acquire(2), Err(PowerError::NotOn(2)));
}

#[test]
fn a_powered_child_blocks_the_parent() {
    let mut model = model();
    assert_eq!(model.power_on(1), Ok(PowerAction::PoweredOn(1)));
    assert_eq!(model.power_on(3), Ok(PowerAction::PoweredOn(3)));
    assert_eq!(
        model.power_off(1),
        Err(PowerError::ChildOn { id: 1, child: 3 })
    );
    assert_eq!(model.power_off(3), Ok(PowerAction::PoweredOff(3)));
    assert_eq!(model.power_off(1), Ok(PowerAction::PoweredOff(1)));
}

#[test]
fn a_reset_needs_an_unused_off_domain() {
    let mut model = model();
    assert_eq!(model.reset(3), Ok(PowerAction::Reset(3)));
    assert_eq!(model.power_on(1), Ok(PowerAction::PoweredOn(1)));
    assert_eq!(model.power_on(2), Ok(PowerAction::PoweredOn(2)));
    assert_eq!(model.reset(2), Err(PowerError::ResetWhileOn(2)));
    assert_eq!(model.power_off(2), Ok(PowerAction::PoweredOff(2)));
    assert_eq!(model.acquire(1), Ok(()));
    assert_eq!(model.reset(1), Err(PowerError::InUse { id: 1, users: 1 }));
    assert_eq!(model.release(1), Ok(()));
    assert_eq!(model.reset(1), Err(PowerError::ResetWhileOn(1)));
    assert_eq!(model.power_off(1), Ok(PowerAction::PoweredOff(1)));
    assert_eq!(model.reset(1), Ok(PowerAction::Reset(1)));
}

#[test]
fn unknown_domains_are_rejected() {
    let mut model = model();
    assert_eq!(model.power_on(9), Err(PowerError::Unknown(9)));
    assert_eq!(model.power_off(9), Err(PowerError::Unknown(9)));
    assert_eq!(model.acquire(9), Err(PowerError::Unknown(9)));
    assert_eq!(model.release(9), Err(PowerError::Unknown(9)));
    assert_eq!(model.reset(9), Err(PowerError::Unknown(9)));
}

#[test]
fn an_unknown_parent_is_reported() {
    let mut model = PowerModel::new();
    assert!(
        model
            .insert(DomainSpec {
                id: 4,
                parent: Some(9)
            })
            .is_ok()
    );
    assert_eq!(model.power_on(4), Err(PowerError::UnknownParent(9)));
}

#[test]
fn duplicate_ids_and_a_full_model_are_rejected() {
    let mut model = model();
    assert_eq!(
        model.insert(DomainSpec {
            id: 1,
            parent: None
        }),
        Err(PowerError::Duplicate(1))
    );
    let mut full = PowerModel::new();
    for id in 0..MAX_DOMAINS as u16 {
        assert!(full.insert(DomainSpec { id, parent: None }).is_ok());
    }
    assert_eq!(
        full.insert(DomainSpec {
            id: 100,
            parent: None
        }),
        Err(PowerError::Full)
    );
}
