// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fetch/render/refresh service core.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::string::String;

use raven_eink::{Delay, Error as PanelError, Glyph, Transport, Uc8179};

use super::*;
use crate::config::Config;
use crate::content::{Content, Font, Page};

struct MockClock {
    now: Rc<Cell<u64>>,
}

impl Clock for MockClock {
    fn now_secs(&self) -> u64 {
        self.now.get()
    }
}

struct MockContent {
    bitmap: [u8; 2],
    text: String,
    use_text: bool,
    fail: Rc<Cell<bool>>,
    calls: Rc<Cell<usize>>,
}

impl Content for MockContent {
    fn fetch(&mut self) -> Result<Page<'_>, Error> {
        self.calls.set(self.calls.get() + 1);
        if self.fail.get() {
            return Err(Error::Fetch);
        }
        if self.use_text {
            Ok(Page::Text { text: &self.text })
        } else {
            Ok(Page::Bitmap {
                width: 8,
                height: 2,
                data: &self.bitmap,
            })
        }
    }
}

struct MockPanel {
    refreshes: Rc<Cell<usize>>,
    last_black: Rc<Cell<usize>>,
    fail: Rc<Cell<bool>>,
}

impl Panel for MockPanel {
    fn refresh(&mut self, frame: &Framebuffer) -> Result<(), Error> {
        self.refreshes.set(self.refreshes.get() + 1);
        if self.fail.get() {
            return Err(Error::Panel(PanelError::Transport));
        }
        let mut black = 0usize;
        let mut y = 0usize;
        while y < frame.panel_height() {
            let mut x = 0usize;
            while x < frame.panel_width() {
                if frame.physical_pixel(x, y) {
                    black += 1;
                }
                x += 1;
            }
            y += 1;
        }
        self.last_black.set(black);
        Ok(())
    }
}

const GLYPH_A: [u8; 7] = [
    0b01110, 0b10001, 0b10001, 0b11111, 0b10001, 0b10001, 0b10001,
];

struct TestFont;

impl Font for TestFont {
    fn glyph(&self, character: u8) -> Option<Glyph<'_>> {
        if character == b'A' {
            Some(Glyph {
                width: 5,
                height: 7,
                data: &GLYPH_A,
            })
        } else {
            None
        }
    }
}

struct Handles {
    now: Rc<Cell<u64>>,
    fail_content: Rc<Cell<bool>>,
    content_calls: Rc<Cell<usize>>,
    fail_panel: Rc<Cell<bool>>,
    refreshes: Rc<Cell<usize>>,
    last_black: Rc<Cell<usize>>,
}

type TestService = Service<MockClock, MockContent, MockPanel, TestFont>;

fn fixture() -> (TestService, Handles) {
    let now = Rc::new(Cell::new(0));
    let fail_content = Rc::new(Cell::new(false));
    let content_calls = Rc::new(Cell::new(0));
    let fail_panel = Rc::new(Cell::new(false));
    let refreshes = Rc::new(Cell::new(0));
    let last_black = Rc::new(Cell::new(0));
    let clock = MockClock {
        now: Rc::clone(&now),
    };
    let content = MockContent {
        bitmap: [0b1000_0001, 0b0100_0010],
        text: String::new(),
        use_text: false,
        fail: Rc::clone(&fail_content),
        calls: Rc::clone(&content_calls),
    };
    let panel = MockPanel {
        refreshes: Rc::clone(&refreshes),
        last_black: Rc::clone(&last_black),
        fail: Rc::clone(&fail_panel),
    };
    let service = Service::new(Config::default(), clock, content, panel, TestFont).unwrap();
    (
        service,
        Handles {
            now,
            fail_content,
            content_calls,
            fail_panel,
            refreshes,
            last_black,
        },
    )
}

#[test]
fn no_fetch_or_refresh_before_ready() {
    let (mut service, handles) = fixture();
    assert_eq!(service.tick(false), Action::Wait);
    assert_eq!(service.state(), State::Waiting);
    assert_eq!(handles.content_calls.get(), 0);
    assert_eq!(handles.refreshes.get(), 0);
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    assert_eq!(handles.content_calls.get(), 1);
    assert_eq!(handles.refreshes.get(), 1);
}

#[test]
fn successful_refresh_renders_and_keeps_last_good() {
    let (mut service, handles) = fixture();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    assert_eq!(handles.last_black.get(), 4);
    assert!(service.last_good().pixel(0, 0));
    assert!(service.last_good().pixel(7, 0));
    assert!(service.last_good().pixel(1, 1));
    assert!(service.last_good().pixel(6, 1));
    assert_eq!(service.state(), State::Idle);
    assert_eq!(service.next_refresh_at(), 300);
}

#[test]
fn interval_schedules_the_next_refresh() {
    let (mut service, handles) = fixture();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    handles.now.set(299);
    assert_eq!(service.tick(true), Action::Wait);
    assert_eq!(handles.refreshes.get(), 1);
    handles.now.set(300);
    assert_eq!(service.tick(true), Action::Refreshed { at: 600 });
    assert_eq!(handles.refreshes.get(), 2);
}

#[test]
fn fetch_failure_keeps_last_good_and_backs_off() {
    let (mut service, handles) = fixture();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    let previous = handles.last_black.get();
    assert_eq!(previous, 4);

    handles.now.set(300);
    handles.fail_content.set(true);
    assert_eq!(service.tick(true), Action::FetchFailed { retry_at: 305 });
    assert_eq!(handles.refreshes.get(), 1);
    assert_eq!(handles.last_black.get(), previous);
    assert!(service.last_good().pixel(0, 0));
    assert_eq!(service.state(), State::Backoff);

    handles.now.set(304);
    assert_eq!(service.tick(true), Action::Wait);
    handles.now.set(305);
    handles.fail_content.set(false);
    assert_eq!(service.tick(true), Action::Refreshed { at: 605 });
    assert_eq!(handles.refreshes.get(), 2);
}

#[test]
fn panel_failure_keeps_last_good_and_doubles_backoff() {
    let (mut service, handles) = fixture();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    let previous = handles.last_black.get();

    handles.now.set(300);
    handles.fail_panel.set(true);
    assert_eq!(service.tick(true), Action::PanelFailed { retry_at: 305 });
    assert_eq!(handles.refreshes.get(), 2);
    assert_eq!(handles.last_black.get(), previous);

    handles.now.set(305);
    assert_eq!(service.tick(true), Action::PanelFailed { retry_at: 315 });
    assert_eq!(handles.refreshes.get(), 3);

    handles.now.set(315);
    handles.fail_panel.set(false);
    assert_eq!(service.tick(true), Action::Refreshed { at: 615 });
    assert_eq!(handles.refreshes.get(), 4);
}

#[test]
fn text_page_renders_through_the_font() {
    let (_old, handles) = fixture();
    let content = MockContent {
        bitmap: [0; 2],
        text: String::from("A"),
        use_text: true,
        fail: Rc::clone(&handles.fail_content),
        calls: Rc::clone(&handles.content_calls),
    };
    let panel = MockPanel {
        refreshes: Rc::clone(&handles.refreshes),
        last_black: Rc::clone(&handles.last_black),
        fail: Rc::clone(&handles.fail_panel),
    };
    let mut service = Service::new(
        Config::default(),
        MockClock {
            now: Rc::clone(&handles.now),
        },
        content,
        panel,
        TestFont,
    )
    .unwrap();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });
    assert!(handles.last_black.get() > 0);
}

#[test]
fn shutdown_stops_all_access() {
    let (mut service, handles) = fixture();
    service.shutdown();
    assert_eq!(service.state(), State::Shutdown);
    assert_eq!(service.tick(true), Action::Shutdown);
    assert_eq!(handles.content_calls.get(), 0);
    assert_eq!(handles.refreshes.get(), 0);
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Event {
    Reset(bool),
    Command(u8),
    Data(std::vec::Vec<u8>),
    Delay(u32),
}

struct RecordingTransport {
    log: Rc<RefCell<std::vec::Vec<Event>>>,
}

impl Transport for RecordingTransport {
    fn reset(&mut self, asserted: bool) -> Result<(), PanelError> {
        self.log.borrow_mut().push(Event::Reset(asserted));
        Ok(())
    }
    fn command(&mut self, command: u8) -> Result<(), PanelError> {
        self.log.borrow_mut().push(Event::Command(command));
        Ok(())
    }
    fn data(&mut self, data: &[u8]) -> Result<(), PanelError> {
        self.log.borrow_mut().push(Event::Data(data.to_vec()));
        Ok(())
    }
}

struct RecordingDelay {
    log: Rc<RefCell<std::vec::Vec<Event>>>,
}

impl Delay for RecordingDelay {
    fn delay_ms(&mut self, milliseconds: u32) {
        self.log.borrow_mut().push(Event::Delay(milliseconds));
    }
}

#[test]
fn uc8179_panel_drives_the_recorded_command_sequence() {
    let log = Rc::new(RefCell::new(std::vec::Vec::new()));
    let transport = RecordingTransport {
        log: Rc::clone(&log),
    };
    let delay = RecordingDelay {
        log: Rc::clone(&log),
    };
    let panel = Uc8179Panel::new(Uc8179::new(transport, delay));
    let content = MockContent {
        bitmap: [0xff; 2],
        text: String::new(),
        use_text: false,
        fail: Rc::new(Cell::new(false)),
        calls: Rc::new(Cell::new(0)),
    };
    let mut service = Service::new(
        Config::default(),
        MockClock {
            now: Rc::new(Cell::new(0)),
        },
        content,
        panel,
        TestFont,
    )
    .unwrap();
    assert_eq!(service.tick(true), Action::Refreshed { at: 300 });

    let events = log.borrow();
    assert!(events.contains(&Event::Command(raven_eink::CMD_DATA_START)));
    assert!(events.contains(&Event::Command(raven_eink::CMD_DISPLAY_REFRESH)));
    assert!(events.iter().any(
        |event| matches!(event, Event::Data(data) if data.len() == raven_eink::DEFAULT_FRAME_BYTES)
    ));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Event::Reset(true)))
    );
}
