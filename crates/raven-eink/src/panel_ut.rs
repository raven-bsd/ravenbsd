// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the UC8179 encoder and its mock transport.

use std::cell::RefCell;
use std::rc::Rc;
use std::vec::Vec;

use super::*;
use crate::framebuffer::Framebuffer;
use crate::profile::DEFAULT_FRAME_BYTES;

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Reset(bool),
    Command(u8),
    Data(Vec<u8>),
    Delay(u32),
}

type Log = Rc<RefCell<Vec<Event>>>;

struct MockTransport {
    log: Log,
    busy: bool,
}

impl MockTransport {
    fn new(log: &Log) -> Self {
        Self {
            log: Rc::clone(log),
            busy: false,
        }
    }
}

impl Transport for MockTransport {
    fn reset(&mut self, asserted: bool) -> Result<(), Error> {
        self.log.borrow_mut().push(Event::Reset(asserted));
        Ok(())
    }

    fn command(&mut self, command: u8) -> Result<(), Error> {
        self.log.borrow_mut().push(Event::Command(command));
        Ok(())
    }

    fn data(&mut self, data: &[u8]) -> Result<(), Error> {
        self.log.borrow_mut().push(Event::Data(data.to_vec()));
        Ok(())
    }

    fn busy(&mut self) -> Result<bool, Error> {
        Ok(self.busy)
    }
}

struct MockDelay {
    log: Log,
}

impl Delay for MockDelay {
    fn delay_ms(&mut self, milliseconds: u32) {
        self.log.borrow_mut().push(Event::Delay(milliseconds));
    }
}

fn encoder() -> (Uc8179<MockTransport, MockDelay>, Log) {
    let log = Rc::new(RefCell::new(Vec::new()));
    let transport = MockTransport::new(&log);
    let delay = MockDelay {
        log: Rc::clone(&log),
    };
    (Uc8179::new(transport, delay), log)
}

#[test]
fn init_emits_reset_power_configuration_and_luts() {
    let (mut panel, log) = encoder();
    panel.init().unwrap();
    let events = log.borrow();
    assert!(matches!(events.first(), Some(Event::Reset(true))));
    assert_eq!(
        events
            .iter()
            .filter(|event| matches!(event, Event::Reset(false)))
            .count(),
        1
    );
    assert!(events.contains(&Event::Command(CMD_PANEL_SETTING)));
    assert!(events.contains(&Event::Command(CMD_POWER_SETTING)));
    assert!(events.contains(&Event::Command(CMD_POWER_ON)));
    assert!(events.contains(&Event::Command(CMD_PLL_CONTROL)));
    assert!(events.contains(&Event::Command(CMD_RESOLUTION)));
    assert!(events.contains(&Event::Command(CMD_VCOM)));
    assert!(events.contains(&Event::Command(CMD_LUT_VCOM)));
    assert!(events.contains(&Event::Command(CMD_LUT_WW)));
    assert!(events.contains(&Event::Command(CMD_LUT_BW)));
    assert!(events.contains(&Event::Command(CMD_LUT_WB)));
    assert!(events.contains(&Event::Command(CMD_LUT_BB)));
    assert!(events.contains(&Event::Command(CMD_TCON)));
    assert!(events.contains(&Event::Delay(RESET_MS)));
    assert!(events.contains(&Event::Delay(POWER_ON_MS)));
    assert!(events.iter().any(|event| {
        matches!(event, Event::Data(data) if data.as_slice() == LUT_VCOM.as_slice())
    }));
}

#[test]
fn display_sends_a_full_frame_and_refresh() {
    let (mut panel, log) = encoder();
    panel.init().unwrap();
    let fb = Framebuffer::new();
    panel.display(&fb).unwrap();

    let events = log.borrow();
    let data_index = events
        .iter()
        .position(|event| matches!(event, Event::Data(data) if data.len() == DEFAULT_FRAME_BYTES))
        .expect("frame data event");
    assert_eq!(
        events.get(data_index.saturating_sub(1)),
        Some(&Event::Command(CMD_DATA_START))
    );
    assert!(
        matches!(events.get(data_index), Some(Event::Data(data)) if data.first() == Some(&0xff))
    );
    assert_eq!(
        events.get(data_index.saturating_add(1)),
        Some(&Event::Command(CMD_DISPLAY_REFRESH))
    );
    assert!(events.contains(&Event::Delay(REFRESH_MS)));
}

#[test]
fn sleep_sends_power_off_deep_sleep_and_wake_byte() {
    let (mut panel, log) = encoder();
    panel.sleep().unwrap();
    let events = log.borrow();
    assert!(events.contains(&Event::Command(CMD_POWER_OFF)));
    assert!(events.contains(&Event::Command(CMD_DEEP_SLEEP)));
    assert!(events.iter().any(|event| {
        matches!(event, Event::Data(data) if data.as_slice() == [0xa5u8].as_slice())
    }));
}

#[test]
fn busy_is_bounded() {
    struct AlwaysBusy;

    impl Transport for AlwaysBusy {
        fn reset(&mut self, _: bool) -> Result<(), Error> {
            Ok(())
        }
        fn command(&mut self, _: u8) -> Result<(), Error> {
            Ok(())
        }
        fn data(&mut self, _: &[u8]) -> Result<(), Error> {
            Ok(())
        }
        fn busy(&mut self) -> Result<bool, Error> {
            Ok(true)
        }
    }

    struct NoDelay;
    impl Delay for NoDelay {
        fn delay_ms(&mut self, _: u32) {}
    }

    let mut panel = Uc8179::new(AlwaysBusy, NoDelay);
    assert_eq!(panel.init(), Err(Error::BusyTimeout));
}
