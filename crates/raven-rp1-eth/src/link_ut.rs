// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the link machine and the interrupt state.

use super::*;

#[test]
fn autonegotiation_reaches_a_link() {
    let mut link = LinkMachine::new(2);
    assert_eq!(link.state(), LinkState::Down);
    assert_eq!(link.on(LinkEvent::Start), Ok(LinkAction::PollPhy));
    assert_eq!(link.state(), LinkState::Negotiating);
    assert_eq!(
        link.on(LinkEvent::AutonegComplete {
            speed: Speed::M1000,
            duplex: Duplex::Full
        }),
        Ok(LinkAction::LinkUp {
            speed: Speed::M1000,
            duplex: Duplex::Full
        })
    );
    assert_eq!(
        link.state(),
        LinkState::Up {
            speed: Speed::M1000,
            duplex: Duplex::Full
        }
    );
}

#[test]
fn a_lost_link_returns_to_down() {
    let mut link = LinkMachine::new(0);
    assert!(link.on(LinkEvent::Start).is_ok());
    assert!(
        link.on(LinkEvent::AutonegComplete {
            speed: Speed::M100,
            duplex: Duplex::Half
        })
        .is_ok()
    );
    assert_eq!(link.on(LinkEvent::LinkDown), Ok(LinkAction::LinkDown));
    assert_eq!(link.state(), LinkState::Down);
    assert_eq!(
        link.on(LinkEvent::LinkDown),
        Err(LinkError::UnexpectedEvent {
            state: LinkState::Down,
            event: LinkEvent::LinkDown
        })
    );
}

#[test]
fn timeouts_are_bounded() {
    let mut link = LinkMachine::new(1);
    assert!(link.on(LinkEvent::Start).is_ok());
    assert_eq!(link.on(LinkEvent::Timeout), Ok(LinkAction::PollPhy));
    assert_eq!(link.timeouts(), 1);
    assert_eq!(link.on(LinkEvent::Timeout), Ok(LinkAction::Fault));
    assert_eq!(link.state(), LinkState::Faulted);
    assert_eq!(
        link.on(LinkEvent::Timeout),
        Err(LinkError::UnexpectedEvent {
            state: LinkState::Faulted,
            event: LinkEvent::Timeout
        })
    );
}

#[test]
fn a_phy_fault_is_terminal_until_restart() {
    let mut link = LinkMachine::new(1);
    assert!(link.on(LinkEvent::Start).is_ok());
    assert_eq!(link.on(LinkEvent::PhyFault), Ok(LinkAction::Fault));
    assert_eq!(link.state(), LinkState::Faulted);
    assert_eq!(link.on(LinkEvent::Start), Ok(LinkAction::PollPhy));
    assert_eq!(link.state(), LinkState::Negotiating);
}

#[test]
fn events_outside_their_state_leave_it_unchanged() {
    let mut link = LinkMachine::new(1);
    assert_eq!(
        link.on(LinkEvent::AutonegComplete {
            speed: Speed::M10,
            duplex: Duplex::Half
        }),
        Err(LinkError::UnexpectedEvent {
            state: LinkState::Down,
            event: LinkEvent::AutonegComplete {
                speed: Speed::M10,
                duplex: Duplex::Half
            }
        })
    );
    assert_eq!(link.state(), LinkState::Down);
}

#[test]
fn interrupts_report_only_unmasked_causes() {
    let mut interrupts = InterruptState::new();
    assert!(!interrupts.is_active());
    interrupts.raise(Interrupts {
        rx_complete: true,
        ..Interrupts::NONE
    });
    assert!(interrupts.active().rx_complete);
    assert!(interrupts.is_active());

    interrupts.set_mask(Interrupts {
        rx_complete: true,
        ..Interrupts::NONE
    });
    assert!(!interrupts.is_active());

    interrupts.raise(Interrupts {
        error: true,
        ..Interrupts::NONE
    });
    assert_eq!(
        interrupts.active(),
        Interrupts {
            error: true,
            ..Interrupts::NONE
        }
    );
}

#[test]
fn a_acknowledgement_clears_only_the_named_causes() {
    let mut interrupts = InterruptState::new();
    interrupts.raise(Interrupts::ALL);
    interrupts.acknowledge(Interrupts {
        tx_complete: true,
        link_change: true,
        ..Interrupts::NONE
    });
    assert!(interrupts.pending().rx_complete);
    assert!(!interrupts.pending().tx_complete);
    assert!(!interrupts.pending().link_change);
    assert!(interrupts.pending().error);
    interrupts.acknowledge(Interrupts::ALL);
    assert_eq!(interrupts.pending(), Interrupts::NONE);
}

#[test]
fn every_cause_is_representable() {
    assert!(Interrupts::ALL.any());
    assert!(!Interrupts::NONE.any());
    let mut interrupts = InterruptState::new();
    interrupts.set_mask(Interrupts::ALL);
    interrupts.raise(Interrupts::ALL);
    assert!(!interrupts.is_active());
    assert_eq!(interrupts.mask(), Interrupts::ALL);
}
