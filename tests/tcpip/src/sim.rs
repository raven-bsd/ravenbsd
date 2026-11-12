// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual-time driver for stacks over simulated links.

use core::fmt;

use raven_tcpip::{Duration, Instant, Stack};

use crate::link::{Endpoint, Link};

/// A simulation deadline expired.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Timeout {
    /// Virtual time at which the deadline expired.
    pub now: Instant,
    /// The trace accumulated so far.
    pub trace: String,
}

impl fmt::Display for Timeout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "simulation timed out at {} ms\ntrace:\n{}",
            self.now.as_millis(),
            self.trace
        )
    }
}

impl std::error::Error for Timeout {}

/// A set of stacks driven by virtual time.
pub struct Sim {
    stacks: Vec<Stack<Endpoint>>,
    links: Vec<Link>,
    now: Instant,
}

impl Sim {
    /// Build a simulation from stacks and the links whose traces it should report.
    #[must_use]
    pub fn new(stacks: Vec<Stack<Endpoint>>, links: Vec<Link>, now: Instant) -> Self {
        Self { stacks, links, now }
    }

    /// Current virtual time.
    #[must_use]
    pub const fn now(&self) -> Instant {
        self.now
    }

    /// The stacks.
    #[must_use]
    pub fn stacks(&self) -> &[Stack<Endpoint>] {
        &self.stacks
    }

    /// The stacks, mutably.
    pub fn stacks_mut(&mut self) -> &mut [Stack<Endpoint>] {
        &mut self.stacks
    }

    /// The links.
    #[must_use]
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// The combined trace text of every link.
    #[must_use]
    pub fn trace_text(&self) -> String {
        let mut text = String::new();
        for link in &self.links {
            text.push_str(&link.trace_text());
        }
        text
    }

    /// Run until `done` is true or the virtual deadline passes.
    ///
    /// Time advances by the minimum of every stack's next timer and every link's next delivery;
    /// there is no wall-clock read and no sleep.
    pub fn run_until(
        &mut self,
        mut done: impl FnMut(&Self) -> bool,
        deadline: Duration,
    ) -> Result<(), Timeout> {
        let deadline_at = self.now.saturating_add(deadline);
        loop {
            if done(self) {
                return Ok(());
            }
            if self.now >= deadline_at {
                return Err(Timeout {
                    now: self.now,
                    trace: self.trace_text(),
                });
            }

            let mut next = deadline_at;
            for stack in &mut self.stacks {
                if let Some(delay) = stack.next_poll_delay(self.now) {
                    next = next.min(self.now.saturating_add(delay));
                }
            }
            for link in &self.links {
                if let Some(delivery) = link.next_delivery(self.now) {
                    next = next.min(delivery);
                }
            }
            if next > self.now {
                self.now = next;
            }
            for stack in &mut self.stacks {
                let _ = stack.poll(self.now);
            }
        }
    }
}
