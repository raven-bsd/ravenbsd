// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic fetch -> render -> refresh service core.
//!
//! The core has no threads, sleeps or network calls. The caller drives [`Service::tick`] with the
//! login/network readiness flag; a [`Clock`] supplies NTP-corrected wall seconds; [`Content`],
//! [`Panel`] and [`Font`] are injected seams. This keeps the whole state machine host-testable
//! with mocks and leaves the real HTTP/SPI/GPIO paths to E3/E4.

use raven_eink::{Delay, Framebuffer, Glyph, Transport, Uc8179};

use crate::Error;
use crate::config::Config;
use crate::content::{Content, Font, Page};

/// Monotonic/NTP-corrected wall clock in seconds.
pub trait Clock {
    /// Current wall-clock seconds.
    fn now_secs(&self) -> u64;
}

/// A panel that consumes one packed full frame.
pub trait Panel {
    /// Refresh the panel with `frame`.
    fn refresh(&mut self, frame: &Framebuffer) -> Result<(), Error>;
}

/// Adapter from the E1 UC8179 encoder to the service [`Panel`] seam.
///
/// The encoder is initialised once; each refresh sends the packed frame and requests a full
/// refresh. The SPI/GPIO [`Transport`]/[`Delay`] implementation is E2/E4.
pub struct Uc8179Panel<T: Transport, D: Delay> {
    driver: Uc8179<T, D>,
    initialized: bool,
}

impl<T: Transport, D: Delay> Uc8179Panel<T, D> {
    /// Wrap a UC8179 encoder.
    pub const fn new(driver: Uc8179<T, D>) -> Self {
        Self {
            driver,
            initialized: false,
        }
    }
}

impl<T: Transport, D: Delay> Panel for Uc8179Panel<T, D> {
    fn refresh(&mut self, frame: &Framebuffer) -> Result<(), Error> {
        if !self.initialized {
            self.driver.init().map_err(Error::Panel)?;
            self.initialized = true;
        }
        self.driver.display(frame).map_err(Error::Panel)
    }
}

/// Service lifecycle state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Waiting for login/network/NTP readiness.
    Waiting,
    /// Idle until the next scheduled refresh.
    Idle,
    /// Fetching content.
    Fetching,
    /// Rendering the fetched page.
    Rendering,
    /// Sending the frame to the panel.
    Refreshing,
    /// Waiting out a retry/backoff delay.
    Backoff,
    /// Clean shutdown requested.
    Shutdown,
}

/// Result of one service tick.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do yet.
    Wait,
    /// A frame was refreshed; the next scheduled refresh is at `at`.
    Refreshed {
        /// Next scheduled refresh time.
        at: u64,
    },
    /// Fetch failed; retry at `retry_at`.
    FetchFailed {
        /// Backoff retry time.
        retry_at: u64,
    },
    /// Panel refresh failed; retry at `retry_at`.
    PanelFailed {
        /// Backoff retry time.
        retry_at: u64,
    },
    /// The service is shut down.
    Shutdown,
}

/// Fetch/render/refresh service core.
pub struct Service<C: Clock, T: Content, P: Panel, F: Font> {
    config: Config,
    clock: C,
    content: T,
    panel: P,
    font: F,
    working: Framebuffer,
    last_good: Framebuffer,
    next_at: u64,
    backoff_secs: u64,
    state: State,
}

impl<C: Clock, T: Content, P: Panel, F: Font> Service<C, T, P, F> {
    /// Validate the config and build a service in the waiting state.
    pub fn new(config: Config, clock: C, content: T, panel: P, font: F) -> Result<Self, Error> {
        config.validate()?;
        let mut working = Framebuffer::new();
        let mut last_good = Framebuffer::new();
        working.set_rotation(config.rotation);
        last_good.set_rotation(config.rotation);
        Ok(Self {
            config,
            clock,
            content,
            panel,
            font,
            working,
            last_good,
            next_at: 0,
            backoff_secs: 0,
            state: State::Waiting,
        })
    }

    /// Current state.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// Next scheduled refresh/retry time.
    #[must_use]
    pub const fn next_refresh_at(&self) -> u64 {
        self.next_at
    }

    /// The last successfully displayed frame.
    #[must_use]
    pub const fn last_good(&self) -> &Framebuffer {
        &self.last_good
    }

    /// Request a clean shutdown; no further fetch or panel access occurs.
    pub fn shutdown(&mut self) {
        self.state = State::Shutdown;
    }

    /// Advance one scheduler step.
    ///
    /// `ready` is the login/network/NTP readiness gate. While false the service performs no fetch
    /// and no panel access. The caller drives this from a post-login worker.
    pub fn tick(&mut self, ready: bool) -> Action {
        if self.state == State::Shutdown {
            return Action::Shutdown;
        }
        if !ready {
            self.state = State::Waiting;
            return Action::Wait;
        }
        let now = self.clock.now_secs();
        if now < self.next_at {
            return Action::Wait;
        }

        self.state = State::Fetching;
        let page = match self.content.fetch() {
            Ok(page) => page,
            Err(_) => return self.fail(now, false),
        };

        self.state = State::Rendering;
        render(&mut self.working, page, &self.font);

        self.state = State::Refreshing;
        if self.panel.refresh(&self.working).is_err() {
            return self.fail(now, true);
        }

        core::mem::swap(&mut self.last_good, &mut self.working);
        self.backoff_secs = 0;
        self.next_at = now.saturating_add(self.config.interval_secs);
        self.state = State::Idle;
        Action::Refreshed { at: self.next_at }
    }

    fn fail(&mut self, now: u64, panel: bool) -> Action {
        let next = if self.backoff_secs == 0 {
            self.config.retry_secs
        } else {
            self.backoff_secs
                .saturating_mul(2)
                .min(self.config.max_backoff_secs)
        };
        self.backoff_secs = next;
        self.next_at = now.saturating_add(next);
        self.state = State::Backoff;
        if panel {
            Action::PanelFailed {
                retry_at: self.next_at,
            }
        } else {
            Action::FetchFailed {
                retry_at: self.next_at,
            }
        }
    }
}

/// Render `page` into `frame`. Unsupported glyphs and out-of-range bitmap data are skipped, never
/// fatal: the last-good frame stays untouched on the panel.
fn render<F: Font>(frame: &mut Framebuffer, page: Page<'_>, font: &F) {
    frame.clear(false);
    match page {
        Page::Bitmap {
            width,
            height,
            data,
        } => {
            let _ = frame.blit_bitmap(0, 0, width, height, data, true);
        }
        Page::Text { text } => {
            let margin = 16i32;
            let line_height = 9i32;
            let mut x = margin;
            let mut y = margin;
            for character in text.bytes() {
                if character == b'\n' {
                    x = margin;
                    y = y.saturating_add(line_height);
                    continue;
                }
                let Some(glyph): Option<Glyph<'_>> = font.glyph(character) else {
                    continue;
                };
                let width = i32::try_from(glyph.width).unwrap_or(i32::MAX);
                if x.saturating_add(width) > frame.width() as i32 - margin {
                    x = margin;
                    y = y.saturating_add(line_height);
                }
                if y.saturating_add(7) > frame.height() as i32 - margin {
                    break;
                }
                let _ = frame.blit_glyph(x, y, &glyph, true);
                x = x.saturating_add(width).saturating_add(1);
            }
        }
    }
}

#[cfg(test)]
#[path = "service_ut.rs"]
mod tests;
