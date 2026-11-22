// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference RFC 8899 PLPMTUD binary search and black-hole fallback model.

use raven_tcpip::PlpmtudState;

/// Reference PLPMTUD search state.
#[derive(Clone, Debug)]
pub struct RefPlpmtud {
    /// Current state.
    pub state: PlpmtudState,
    /// Safe base.
    pub base: u32,
    /// Ceiling.
    pub max: u32,
    /// Largest confirmed size.
    pub current: u32,
    /// Lower search bound.
    pub low: u32,
    /// Upper search bound.
    pub high: u32,
    /// Last probe size.
    pub probe_size: u32,
    /// Probes used.
    pub probes: usize,
    /// Probe budget.
    pub budget: usize,
}

impl RefPlpmtud {
    /// Start a reference search with the product's clamps.
    pub fn new(base: u32, max: u32, budget: usize) -> Self {
        let base = base.max(1);
        let max = max.max(base);
        Self {
            state: PlpmtudState::Base,
            base,
            max,
            current: base,
            low: base,
            high: max,
            probe_size: base,
            probes: 0,
            budget: budget.max(1),
        }
    }

    /// Begin the search.
    pub fn start(&mut self) {
        self.state = PlpmtudState::Searching;
        self.low = self.base;
        self.high = self.max;
        self.probes = 0;
        self.probe_size = self.next_size();
    }

    fn next_size(&self) -> u32 {
        if self.high <= self.low + 1 {
            return self.high;
        }
        let mid = self.low + (self.high - self.low) / 2;
        mid.max(self.low + 1)
    }

    /// Next probe size.
    pub fn next_probe(&mut self) -> Option<u32> {
        if self.state != PlpmtudState::Searching {
            return None;
        }
        if self.probes >= self.budget {
            self.state = PlpmtudState::Error;
            self.current = self.base;
            return None;
        }
        if self.high <= self.low + 1 {
            if self.current < self.high {
                self.probes += 1;
                self.probe_size = self.high;
                return Some(self.high);
            }
            self.state = PlpmtudState::SearchComplete;
            self.current = self.low;
            return None;
        }
        self.probes += 1;
        self.probe_size = self.next_size();
        Some(self.probe_size)
    }

    /// Probe acknowledged.
    pub fn on_probe_ack(&mut self, size: u32) {
        self.low = size.max(self.low);
        self.current = self.low;
        if self.high <= self.low + 1 {
            self.state = PlpmtudState::SearchComplete;
        }
    }

    /// Probe lost.
    pub fn on_probe_loss(&mut self, size: u32) {
        self.high = size.min(self.high);
        if self.high <= self.low + 1 {
            self.state = PlpmtudState::SearchComplete;
            self.current = self.low;
        }
    }
}

/// Reference probe sequence.
pub fn ref_probe_sequence(base: u32, max: u32, budget: usize) -> Vec<u32> {
    let mut search = RefPlpmtud::new(base, max, budget);
    search.start();
    let mut sizes = Vec::new();
    while let Some(size) = search.next_probe() {
        sizes.push(size);
        search.on_probe_ack(size);
    }
    sizes
}

/// Reference black-hole fallback: a fresh search from the floor to the current MSS.
pub fn ref_blackhole(floor: u32, mss: u32, budget: usize) -> RefPlpmtud {
    let mut search = RefPlpmtud::new(floor, mss, budget);
    search.start();
    search
}
