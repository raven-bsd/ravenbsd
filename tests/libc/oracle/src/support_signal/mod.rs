// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent models and case types for the signal differentials.
//!
//! Layer A uses the donor C as the oracle (the subject is `raven_libc_signal::SigSet`).
//! Layer C and D use the second implementation below: one model written from the ticket text and
//! the musl `sigaction.c` behaviour, compared with the crate. The model never calls the crate.

use serde::Deserialize;

use raven_libc_signal::dispositions::{
    DispTable, Disposition, SA_NODEFER, SA_RESTART, SA_RESTORER,
};
use raven_libc_signal::sigset::APP_MASK;

/// A signal number the application may name.
pub const fn application_signal(sig: i32) -> bool {
    sig >= 1 && sig <= 64 && !(sig >= 32 && sig <= 34)
}

/// Model of `musl src/signal/sigaction.c` over an array.
pub struct DispositionModel {
    /// Handler/class per signal (0 = SIG_DFL, 1 = SIG_IGN, 2 = function).
    pub handler: [u8; 65],
    /// `sa_mask` word per signal.
    pub mask: [u64; 65],
    /// `sa_flags` per signal.
    pub flags: [u32; 65],
    /// The handler-set word.
    pub handler_set: u64,
    /// The `__eintr_valid_flag`.
    pub eintr_valid: bool,
}

impl Default for DispositionModel {
    fn default() -> Self {
        Self {
            handler: [0; 65],
            mask: [0; 65],
            flags: [0; 65],
            handler_set: 0,
            eintr_valid: false,
        }
    }
}

impl DispositionModel {
    /// Apply one `sigaction` step; returns the old `(handler, mask, flags)` or an error marker.
    #[allow(clippy::result_unit_err)] // the model only needs "rejected", not an error taxonomy
    pub fn apply(
        &mut self,
        sig: i32,
        request: Option<(u8, u64, u32)>,
    ) -> Result<(u8, u64, u32), ()> {
        if !application_signal(sig) {
            return Err(());
        }
        if request.is_some() && (sig == 9 || sig == 19) {
            return Err(());
        }
        let index = sig as usize;
        let old = (self.handler[index], self.mask[index], self.flags[index]);
        if let Some((handler, mask, flags)) = request {
            let flags = flags & !SA_RESTORER;
            self.handler[index] = handler;
            self.mask[index] = mask & APP_MASK;
            self.flags[index] = flags;
            if handler >= 2 {
                self.handler_set |= 1u64 << (sig - 1);
                if flags & SA_RESTART == 0 {
                    self.eintr_valid = true;
                }
            }
        }
        Ok(old)
    }
}

/// Model of the handler mask: `old | sa_mask | sig` unless `SA_NODEFER`.
pub fn model_handler_mask(old: u64, sa_mask: u64, sig: i32, flags: u32) -> u64 {
    let mut mask = old | sa_mask;
    if flags & SA_NODEFER == 0 && application_signal(sig) {
        mask |= 1u64 << (sig - 1);
    }
    mask & APP_MASK
}

/// Model of the deliverable set.
pub fn model_deliverable(pending: u64, mask: u64) -> u64 {
    pending & !mask & APP_MASK
}

/// Model of the lowest deliverable signal.
pub fn model_lowest(bits: u64) -> i32 {
    if bits == 0 {
        0
    } else {
        bits.trailing_zeros() as i32 + 1
    }
}

/// A disposition step for the layer C corpus.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionStep {
    /// Signal number.
    pub sig: i32,
    /// `true` for a write, `false` for a query.
    pub write: bool,
    /// Handler class: 0 DFL, 1 IGN, 2 function.
    pub class: u8,
    /// `sa_mask` word.
    pub mask: u64,
    /// `sa_flags`.
    pub flags: u32,
}

/// A sequence of disposition operations; the observation is the whole table snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispositionCase {
    /// Steps in order.
    pub steps: Vec<ActionStep>,
}

impl<'de> Deserialize<'de> for DispositionCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            steps: Vec<ActionStep>,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self { steps: raw.steps })
    }
}

/// The observation of a disposition run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DispositionObs {
    /// Final handler classes (0/1/2).
    pub handler: Vec<u8>,
    /// Final mask words.
    pub mask: Vec<u64>,
    /// Final flags.
    pub flags: Vec<u32>,
    /// Final handler set.
    pub handler_set: u64,
    /// Final EINTR flag.
    pub eintr_valid: bool,
    /// Rejected step count.
    pub rejected: usize,
}

impl DispositionCase {
    /// A seeded sequence of 1..8 steps.
    #[must_use]
    pub fn random(rng: &mut crate::rng::Rng) -> Self {
        let length = (rng.below(8) + 1) as usize;
        let mut steps = Vec::with_capacity(length);
        for _ in 0..length {
            let sig = (rng.below(74) as i32) - 3;
            let mask = match rng.below(4) {
                0 => 0,
                1 => APP_MASK,
                2 => rng.next_u64() & APP_MASK,
                _ => 1u64 << (rng.below(64)),
            };
            let flags = match rng.below(6) {
                0 => 0,
                1 => SA_RESTART,
                2 => SA_NODEFER,
                3 => SA_RESTORER,
                4 => SA_RESTART | SA_NODEFER,
                _ => rng.next_u64() as u32,
            };
            steps.push(ActionStep {
                sig,
                write: rng.below(4) != 0,
                class: rng.below(3) as u8,
                mask,
                flags,
            });
        }
        Self { steps }
    }

    /// Run the model.
    #[must_use]
    pub fn run_model(&self) -> DispositionObs {
        let mut model = DispositionModel::default();
        let mut rejected = 0;
        for step in &self.steps {
            let request = if step.write {
                Some((step.class, step.mask, step.flags))
            } else {
                None
            };
            if model.apply(step.sig, request).is_err() {
                rejected += 1;
            }
        }
        DispositionObs {
            handler: model.handler[1..].to_vec(),
            mask: model.mask[1..].to_vec(),
            flags: model.flags[1..].to_vec(),
            handler_set: model.handler_set,
            eintr_valid: model.eintr_valid,
            rejected,
        }
    }

    /// A tiny exhaustive domain (rejected ranges and the flag combinations of the model).
    #[must_use]
    pub fn exhaustive_small() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for sig in [-1, 0, 1, 9, 19, 32, 33, 34, 64, 65] {
            for write in [false, true] {
                for flags in [0, SA_RESTART, SA_NODEFER, SA_RESTORER] {
                    cases.push(Self {
                        steps: vec![ActionStep {
                            sig,
                            write,
                            class: 2,
                            mask: APP_MASK,
                            flags,
                        }],
                    });
                }
            }
        }
        Box::new(cases.into_iter())
    }

    /// Run the crate.
    #[must_use]
    pub fn run_subject(&self) -> DispositionObs {
        raven_libc_signal::action::clear_eintr_valid();
        let table = DispTable::new();
        let mut rejected = 0;
        for step in &self.steps {
            let request = if step.write {
                Some(Disposition {
                    handler: usize::from(step.class),
                    mask: step.mask,
                    flags: step.flags,
                })
            } else {
                None
            };
            if raven_libc_signal::action::sigaction(&table, step.sig, request.as_ref(), None)
                .is_err()
            {
                rejected += 1;
            }
        }
        let mut handler = Vec::new();
        let mut mask = Vec::new();
        let mut flags = Vec::new();
        for sig in 1..=64 {
            let disposition = table.read(sig);
            handler.push(if disposition.handler == 0 {
                0
            } else if disposition.handler == 1 {
                1
            } else {
                2
            });
            mask.push(disposition.mask);
            flags.push(disposition.flags);
        }
        DispositionObs {
            handler,
            mask,
            flags,
            handler_set: table.handler_set().app_word(),
            eintr_valid: raven_libc_signal::action::eintr_valid(),
            rejected,
        }
    }
}

impl crate::diff::Case for DispositionCase {
    type Obs = DispositionObs;

    fn shrink(&self) -> Vec<Self> {
        if self.steps.len() <= 1 {
            return Vec::new();
        }
        vec![Self {
            steps: self.steps[..self.steps.len() - 1].to_vec(),
        }]
    }

    fn to_toml(&self) -> String {
        let steps: Vec<String> = self
            .steps
            .iter()
            .map(|step| {
                format!(
                    "{{ sig = {}, write = {}, class = {}, mask = {}, flags = {} }}",
                    step.sig, step.write, step.class, step.mask, step.flags
                )
            })
            .collect();
        format!("steps = [{}]", steps.join(", "))
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "handler_set = {}\nrejected = {}\neintr_valid = {}",
            obs.handler_set, obs.rejected, obs.eintr_valid
        )
    }
}

/// A layer D decision case: pending sets and mask words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecisionCase {
    /// Pending word.
    pub pending: u64,
    /// Two mask words (old and `sa_mask`).
    pub masks: [u64; 2],
    /// Signal.
    pub sig: i32,
    /// Flags.
    pub flags: u32,
}

impl DecisionCase {
    /// Model observation: `(deliverable, lowest, handler_mask)`.
    #[must_use]
    pub fn run_model(&self) -> (u64, i32, u64) {
        (
            model_deliverable(self.pending, self.masks[0]),
            model_lowest(model_deliverable(self.pending, self.masks[0])),
            model_handler_mask(self.masks[0], self.masks[1], self.sig, self.flags),
        )
    }

    /// Crate observation.
    #[must_use]
    pub fn run_subject(&self) -> (u64, i32, u64) {
        let ready = raven_libc_signal::deliver::deliverable(self.pending, self.masks[0]);
        (
            ready,
            raven_libc_signal::deliver::lowest(ready),
            raven_libc_signal::mask::handler_mask(
                self.masks[0],
                self.masks[1],
                self.sig,
                self.flags,
            ),
        )
    }

    /// The small exhaustive domain: signal boundary values, masks and flags.
    #[must_use]
    pub fn exhaustive_small() -> Box<dyn Iterator<Item = Self>> {
        let mut cases = Vec::new();
        for sig in [-1, 0, 1, 32, 34, 35, 64, 65] {
            for masks in [[0, 0], [APP_MASK, APP_MASK], [1, 2], [1 << 31, 1 << 63]] {
                for flags in [0, SA_RESTART, SA_NODEFER] {
                    cases.push(Self {
                        pending: u64::MAX,
                        masks,
                        sig,
                        flags,
                    });
                }
            }
        }
        Box::new(cases.into_iter())
    }

    /// A seeded case.
    #[must_use]
    pub fn random(rng: &mut crate::rng::Rng) -> Self {
        let pending = rng.next_u64() & !0x3_8000_0000u64;
        Self {
            pending,
            masks: [rng.next_u64() & APP_MASK, rng.next_u64() & APP_MASK],
            sig: (rng.below(70) as i32) - 3,
            flags: rng.next_u64() as u32,
        }
    }
}

impl<'de> Deserialize<'de> for DecisionCase {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Raw {
            pending: u64,
            mask0: u64,
            mask1: u64,
            sig: i32,
            flags: u32,
        }
        let raw = Raw::deserialize(deserializer)?;
        Ok(Self {
            pending: raw.pending,
            masks: [raw.mask0, raw.mask1],
            sig: raw.sig,
            flags: raw.flags,
        })
    }
}

impl crate::diff::Case for DecisionCase {
    type Obs = (u64, i32, u64);

    fn shrink(&self) -> Vec<Self> {
        let mut out = Vec::new();
        if self.sig > 1 {
            out.push(Self {
                sig: self.sig - 1,
                ..self.clone()
            });
        }
        if self.masks[0] != 0 {
            out.push(Self {
                masks: [self.masks[0] & (self.masks[0] - 1), self.masks[1]],
                ..self.clone()
            });
        }
        out
    }

    fn to_toml(&self) -> String {
        format!(
            "pending = {}\nmask0 = {}\nmask1 = {}\nsig = {}\nflags = {}",
            self.pending, self.masks[0], self.masks[1], self.sig, self.flags
        )
    }

    fn obs_toml(obs: &Self::Obs) -> String {
        format!(
            "ready = {}\nlowest = {}\nhandler_mask = {}",
            obs.0, obs.1, obs.2
        )
    }
}
