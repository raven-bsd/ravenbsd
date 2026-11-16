// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the USB hub/port state.

use super::*;

fn enabled_port() -> HubPort {
    let mut port = HubPort::new(0);
    port.event(HubEvent::Attach(PortSpeed::High))
        .expect("attach");
    port.event(HubEvent::PowerOn).expect("power");
    port.event(HubEvent::Reset).expect("reset");
    port.event(HubEvent::ResetComplete).expect("reset complete");
    port
}

#[test]
fn a_port_moves_from_attach_to_configured() {
    let mut port = HubPort::new(0);
    assert_eq!(port.state(), HubPortState::Disconnected);
    assert_eq!(port.speed(), None);

    assert_eq!(port.event(HubEvent::Attach(PortSpeed::High)), Ok(()));
    assert_eq!(port.state(), HubPortState::Attached);
    assert_eq!(port.speed(), Some(PortSpeed::High));
    assert!(port.changes().connect);

    assert_eq!(port.event(HubEvent::PowerOn), Ok(()));
    assert_eq!(port.state(), HubPortState::Powered);
    assert_eq!(port.event(HubEvent::Reset), Ok(()));
    assert_eq!(port.state(), HubPortState::Resetting);
    assert!(port.changes().reset);
    assert_eq!(port.event(HubEvent::ResetComplete), Ok(()));
    assert_eq!(port.state(), HubPortState::Enabled);
    assert!(port.changes().enable);

    assert_eq!(port.event(HubEvent::Address(7)), Ok(()));
    assert_eq!(port.address(), Some(7));
    assert_eq!(port.event(HubEvent::Configure(1)), Ok(()));
    assert_eq!(port.configuration(), Some(1));
    assert!(port.changes().any());
    port.acknowledge();
    assert!(!port.changes().any());
}

#[test]
fn suspend_resume_and_detach_clear_the_device_state() {
    let mut port = enabled_port();
    port.event(HubEvent::Address(3)).expect("address");
    port.event(HubEvent::Configure(1)).expect("configure");
    port.acknowledge();

    assert_eq!(port.event(HubEvent::Suspend), Ok(()));
    assert_eq!(port.state(), HubPortState::Suspended);
    assert!(port.changes().suspend);
    assert_eq!(port.event(HubEvent::Resume), Ok(()));
    assert_eq!(port.state(), HubPortState::Enabled);

    assert_eq!(port.event(HubEvent::Detach), Ok(()));
    assert_eq!(port.state(), HubPortState::Disconnected);
    assert_eq!(port.speed(), None);
    assert_eq!(port.address(), None);
    assert_eq!(port.configuration(), None);
}

#[test]
fn invalid_events_leave_the_port_unchanged() {
    let mut port = HubPort::new(0);
    assert!(matches!(
        port.event(HubEvent::PowerOn),
        Err(HubError::UnexpectedEvent { .. })
    ));
    assert_eq!(port.state(), HubPortState::Disconnected);
    port.event(HubEvent::Attach(PortSpeed::Full))
        .expect("attach");
    assert!(matches!(
        port.event(HubEvent::Attach(PortSpeed::High)),
        Err(HubError::UnexpectedEvent { .. })
    ));
    assert_eq!(port.speed(), Some(PortSpeed::Full));
    assert_eq!(
        port.event(HubEvent::Address(1)),
        Err(HubError::UnexpectedEvent {
            state: HubPortState::Attached,
            event: HubEvent::Address(1)
        })
    );
    let mut enabled = enabled_port();
    assert_eq!(
        enabled.event(HubEvent::Address(0)),
        Err(HubError::InvalidAddress(0))
    );
    enabled.event(HubEvent::Address(1)).expect("address");
    assert_eq!(
        enabled.event(HubEvent::Configure(0)),
        Err(HubError::InvalidConfiguration(0))
    );
}

#[test]
fn over_current_is_latched_until_cleared() {
    let mut port = enabled_port();
    assert_eq!(port.event(HubEvent::OverCurrent), Ok(()));
    assert_eq!(port.state(), HubPortState::OverCurrent);
    assert!(port.changes().over_current);
    assert_eq!(port.event(HubEvent::ClearOverCurrent), Ok(()));
    assert_eq!(port.state(), HubPortState::Disconnected);
    assert!(port.changes().over_current);
}

#[test]
fn a_hub_bounds_its_ports_and_tracks_enabled_count() {
    assert_eq!(Hub::new(0), Err(HubError::PortCount(0)));
    assert_eq!(
        Hub::new((MAX_HUB_PORTS + 1) as u8),
        Err(HubError::PortCount((MAX_HUB_PORTS + 1) as u8))
    );
    let mut hub = Hub::new(2).expect("hub");
    assert_eq!(hub.port_count(), 2);
    assert_eq!(hub.enabled(), 0);
    hub.event(0, HubEvent::Attach(PortSpeed::Super))
        .expect("attach");
    hub.event(0, HubEvent::PowerOn).expect("power");
    hub.event(0, HubEvent::Reset).expect("reset");
    hub.event(0, HubEvent::ResetComplete).expect("complete");
    assert_eq!(hub.enabled(), 1);
    assert!(hub.any_change());
    hub.acknowledge_all();
    assert!(!hub.any_change());
    assert_eq!(
        hub.event(2, HubEvent::Attach(PortSpeed::Low)),
        Err(HubError::PortOutOfRange(2))
    );
    assert_eq!(
        hub.port(0).map(|port| port.state()),
        Some(HubPortState::Enabled)
    );
    assert_eq!(
        hub.port(1).map(|port| port.state()),
        Some(HubPortState::Disconnected)
    );
}
