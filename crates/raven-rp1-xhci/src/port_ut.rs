// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the port and roothub state machine.

use super::*;

#[test]
fn a_connect_starts_polling_and_link_up_enables() {
    let mut port = PortMachine::new();
    assert_eq!(port.state(), PortState::NotConnected);
    assert_eq!(port.on(PortEvent::Connect), Ok(PortAction::Reset));
    assert_eq!(port.state(), PortState::Polling);
    assert!(port.changes().connect);
    assert_eq!(port.on(PortEvent::LinkUp), Ok(PortAction::Enabled));
    assert_eq!(port.state(), PortState::Enabled);
    assert!(port.changes().enable);
}

#[test]
fn a_port_reset_completes_into_enabled() {
    let mut port = PortMachine::new();
    assert!(port.on(PortEvent::Connect).is_ok());
    assert_eq!(port.on(PortEvent::Reset), Ok(PortAction::Reset));
    assert_eq!(port.state(), PortState::Resetting);
    assert_eq!(port.on(PortEvent::ResetComplete), Ok(PortAction::Enabled));
    assert_eq!(port.state(), PortState::Enabled);
    assert!(port.changes().reset);
    port.acknowledge();
    assert!(!port.changes().any());
}

#[test]
fn suspend_and_resume_walk_the_link_states() {
    let mut port = PortMachine::new();
    assert!(port.on(PortEvent::Connect).is_ok());
    assert!(port.on(PortEvent::LinkUp).is_ok());
    port.acknowledge();
    assert_eq!(port.on(PortEvent::Suspend), Ok(PortAction::Suspended));
    assert_eq!(port.state(), PortState::Suspended);
    assert_eq!(port.on(PortEvent::Resume), Ok(PortAction::Resumed));
    assert_eq!(port.state(), PortState::Resumed);
    assert_eq!(port.on(PortEvent::LinkUp), Ok(PortAction::Enabled));
    assert_eq!(port.state(), PortState::Enabled);
}

#[test]
fn a_disconnect_returns_to_not_connected() {
    let mut port = PortMachine::new();
    assert!(port.on(PortEvent::Connect).is_ok());
    port.acknowledge();
    assert_eq!(port.on(PortEvent::Disconnect), Ok(PortAction::Disconnected));
    assert_eq!(port.state(), PortState::NotConnected);
    assert!(port.changes().connect);
    assert_eq!(
        port.on(PortEvent::Disconnect),
        Err(PortError::UnexpectedEvent {
            state: PortState::NotConnected,
            event: PortEvent::Disconnect
        })
    );
}

#[test]
fn overcurrent_and_errors_inactivate_the_port() {
    let mut port = PortMachine::new();
    assert!(port.on(PortEvent::Connect).is_ok());
    assert_eq!(port.on(PortEvent::Overcurrent), Ok(PortAction::Fault));
    assert_eq!(port.state(), PortState::Inactive);
    assert!(port.changes().overcurrent);

    let mut port = PortMachine::new();
    assert_eq!(port.on(PortEvent::Error), Ok(PortAction::Fault));
    assert_eq!(port.state(), PortState::Inactive);
}

#[test]
fn events_outside_their_state_leave_it_unchanged() {
    let mut port = PortMachine::new();
    assert_eq!(
        port.on(PortEvent::Suspend),
        Err(PortError::UnexpectedEvent {
            state: PortState::NotConnected,
            event: PortEvent::Suspend
        })
    );
    assert_eq!(port.state(), PortState::NotConnected);
}

#[test]
fn a_roothub_bounds_its_ports() {
    assert!(Roothub::new(0).is_err());
    assert!(Roothub::new(MAX_PORTS as u8 + 1).is_err());
    let mut hub = Roothub::new(2).expect("hub");
    assert_eq!(hub.port_count(), 2);
    assert!(hub.port(0).is_some());
    assert!(hub.port(2).is_none());
    assert_eq!(
        hub.event(2, PortEvent::Connect),
        Err(PortError::PortOutOfRange(2))
    );
    assert!(hub.event(1, PortEvent::Connect).is_ok());
    assert!(hub.any_change());
    assert_eq!(hub.enabled_count(), 0);
    assert!(hub.event(1, PortEvent::LinkUp).is_ok());
    assert_eq!(hub.enabled_count(), 1);
    hub.acknowledge_all();
    assert!(!hub.any_change());
}
