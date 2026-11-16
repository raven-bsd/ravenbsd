// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn connect_poll_reset_and_enable() {
    let mut port = PortMachine::new();
    assert_eq!(port.state(), PortState::NotConnected);
    assert_eq!(port.on(PortEvent::Connect), Ok(PortAction::Reset));
    assert_eq!(port.state(), PortState::Polling);
    assert!(port.changes().connect);
    port.acknowledge();
    assert!(!port.changes().any());
    assert_eq!(port.on(PortEvent::Reset), Ok(PortAction::Reset));
    assert_eq!(port.on(PortEvent::ResetComplete), Ok(PortAction::Enabled));
    assert_eq!(port.state(), PortState::Enabled);
    assert!(port.changes().enable && port.changes().reset && port.changes().link);
}

#[test]
fn link_up_enables_a_polled_port() {
    let mut port = PortMachine::new();
    port.on(PortEvent::Connect).expect("connect");
    assert_eq!(port.on(PortEvent::LinkUp), Ok(PortAction::Enabled));
    assert_eq!(port.state(), PortState::Enabled);
}

#[test]
fn suspend_resume_and_disconnect() {
    let mut port = PortMachine::new();
    port.on(PortEvent::Connect).expect("connect");
    port.on(PortEvent::Reset).expect("reset");
    port.on(PortEvent::ResetComplete).expect("reset complete");
    assert_eq!(port.on(PortEvent::Suspend), Ok(PortAction::Suspended));
    assert_eq!(port.on(PortEvent::Resume), Ok(PortAction::Resumed));
    assert_eq!(port.on(PortEvent::LinkUp), Ok(PortAction::Enabled));
    assert_eq!(port.on(PortEvent::Disconnect), Ok(PortAction::Disconnected));
    assert_eq!(port.state(), PortState::NotConnected);
}

#[test]
fn invalid_transitions_leave_the_state_unchanged() {
    let mut port = PortMachine::new();
    assert_eq!(
        port.on(PortEvent::Reset),
        Err(PortError::UnexpectedEvent {
            state: PortState::NotConnected,
            event: PortEvent::Reset,
        })
    );
    assert_eq!(port.state(), PortState::NotConnected);
    assert_eq!(
        port.on(PortEvent::Disconnect),
        Err(PortError::UnexpectedEvent {
            state: PortState::NotConnected,
            event: PortEvent::Disconnect,
        })
    );
}

#[test]
fn overcurrent_and_error_fault_the_port() {
    let mut port = PortMachine::new();
    port.on(PortEvent::Connect).expect("connect");
    assert_eq!(port.on(PortEvent::Overcurrent), Ok(PortAction::Fault));
    assert_eq!(port.state(), PortState::Inactive);
    assert!(port.changes().overcurrent);
    let mut port = PortMachine::new();
    port.on(PortEvent::Connect).expect("connect");
    assert_eq!(port.on(PortEvent::Error), Ok(PortAction::Fault));
    assert_eq!(port.state(), PortState::Inactive);
}

#[test]
fn roothub_bounds_and_dispatch() {
    assert_eq!(Roothub::new(0).unwrap_err(), PortError::PortCount(0));
    assert_eq!(Roothub::new(9).unwrap_err(), PortError::PortCount(9));
    let mut hub = Roothub::new(4).expect("hub");
    assert_eq!(hub.count(), 4);
    assert_eq!(hub.port(0).unwrap_err(), PortError::PortOutOfRange(0));
    assert_eq!(hub.port(5).unwrap_err(), PortError::PortOutOfRange(5));
    assert_eq!(hub.on(2, PortEvent::Connect), Ok(PortAction::Reset));
    assert_eq!(hub.port(2).expect("port").state(), PortState::Polling);
    assert_eq!(hub.port(1).expect("port").state(), PortState::NotConnected);
}

#[test]
fn change_bits_are_acked_per_port() {
    let mut hub = Roothub::new(2).expect("hub");
    hub.on(1, PortEvent::Connect).expect("connect");
    hub.on(2, PortEvent::Connect).expect("connect");
    hub.port_mut(1).expect("port").acknowledge();
    assert!(!hub.port(1).expect("port").changes().any());
    assert!(hub.port(2).expect("port").changes().connect);
}
