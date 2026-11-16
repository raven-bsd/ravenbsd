// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The firmware-load state machine.
//!
//! The loader drives one [`FirmwareSeam`] operation per [`Loader::step`]: reset, boot-ROM ready
//! poll, one write/doorbell/status round per chunk, then a verify/start round. Timeouts and busy
//! polls are bounded by [`Limits`]; a device fault, a bus error or an exhausted budget moves the
//! machine to [`State::Faulted`] and returns a typed [`FwError`]. [`Loader::reset`] returns it to
//! [`State::Idle`]. The image metadata comes from [`raven_rp1::FirmwareImage`]; the blob bytes are
//! caller data and are never stored in this crate's API beyond the borrow.

use core::fmt;

use raven_rp1::FirmwareImage;

use crate::plan::{Chunk, Plan};
use crate::seam::{FirmwareSeam, SeamError, Status};

/// Safety bound for [`Loader::run`] against a misbehaving seam.
pub const MAX_STEPS: u32 = 4096;

/// Retry budgets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// Consecutive poll timeouts/not-ready answers before a fault.
    pub max_timeouts: u8,
    /// Consecutive busy status answers before a fault.
    pub max_busy_polls: u8,
}

impl Limits {
    /// The model defaults: three timeouts and four busy polls.
    pub const DEFAULT: Self = Self {
        max_timeouts: 3,
        max_busy_polls: 4,
    };
}

/// Where the loader is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Nothing started.
    Idle,
    /// Reset was issued; the boot ROM is being polled.
    WaitingBootRom,
    /// Chunks are being written and rung.
    Transferring,
    /// All chunks are accepted; the image is being verified/started.
    Verifying,
    /// The image was started.
    Running,
    /// A fault stopped the machine; only `reset` leaves this state.
    Faulted,
}

/// What one `step` did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The seam reset the device.
    Reset,
    /// The boot ROM was polled.
    PollBootRom,
    /// One chunk was written into the load window.
    WriteChunk(Chunk),
    /// The chunk's doorbell was rung.
    Doorbell(Chunk),
    /// The device status was polled.
    PollStatus,
    /// The image was started.
    Start,
    /// The load is complete.
    Done,
}

/// Why the load cannot proceed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FwError {
    /// The firmware record is incomplete (path/digest/size/licence).
    Unrecorded,
    /// The record's size differs from the supplied blob.
    BlobMismatch {
        /// Size in the record.
        image: u64,
        /// Blob length.
        blob: usize,
    },
    /// The plan's image length differs from the supplied blob.
    PlanMismatch {
        /// Length in the plan.
        plan: u32,
        /// Blob length.
        blob: usize,
    },
    /// A seam operation failed.
    Seam(SeamError),
    /// The poll budget was exhausted.
    Timeout,
    /// The busy budget was exhausted.
    Busy,
    /// The device reported a failure code.
    DeviceFault(u16),
    /// The machine already faulted; reset it first.
    Faulted,
    /// The run-loop safety bound was reached.
    Loop,
}

impl fmt::Display for FwError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unrecorded => f.write_str("firmware record is incomplete"),
            Self::BlobMismatch { image, blob } => {
                write!(f, "record says {image} bytes, blob has {blob}")
            }
            Self::PlanMismatch { plan, blob } => {
                write!(f, "plan says {plan} bytes, blob has {blob}")
            }
            Self::Seam(error) => match error {
                SeamError::Bus => f.write_str("load seam bus error"),
                SeamError::Timeout => f.write_str("load seam timeout"),
            },
            Self::Timeout => f.write_str("load poll budget exhausted"),
            Self::Busy => f.write_str("device stayed busy"),
            Self::DeviceFault(code) => write!(f, "device fault {code:#x}"),
            Self::Faulted => f.write_str("loader is faulted; reset it"),
            Self::Loop => f.write_str("load did not settle"),
        }
    }
}

impl core::error::Error for FwError {}

impl From<SeamError> for FwError {
    fn from(error: SeamError) -> Self {
        Self::Seam(error)
    }
}

/// The result of a complete run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoadOutcome {
    /// Chunks written.
    pub chunks: u32,
    /// Steps taken.
    pub steps: u32,
}

/// The firmware loader.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Loader<'a> {
    image: FirmwareImage,
    blob: &'a [u8],
    plan: Plan,
    limits: Limits,
    state: State,
    index: u32,
    wrote: bool,
    rang: bool,
    timeouts: u8,
    busy: u8,
}

impl<'a> Loader<'a> {
    /// Build a loader after checking the record, the blob length and the plan.
    ///
    /// # Errors
    ///
    /// Returns [`FwError`] for an unrecorded image, a record/blob/plan size mismatch.
    pub fn new(
        image: FirmwareImage,
        blob: &'a [u8],
        plan: Plan,
        limits: Limits,
    ) -> Result<Self, FwError> {
        if !image.is_recorded() {
            return Err(FwError::Unrecorded);
        }
        let size = image.size.ok_or(FwError::Unrecorded)?;
        if size != blob.len() as u64 {
            return Err(FwError::BlobMismatch {
                image: size,
                blob: blob.len(),
            });
        }
        if plan.blob_len() as usize != blob.len() {
            return Err(FwError::PlanMismatch {
                plan: plan.blob_len(),
                blob: blob.len(),
            });
        }
        Ok(Self {
            image,
            blob,
            plan,
            limits,
            state: State::Idle,
            index: 0,
            wrote: false,
            rang: false,
            timeouts: 0,
            busy: 0,
        })
    }

    /// The firmware record.
    #[must_use]
    pub const fn image(&self) -> FirmwareImage {
        self.image
    }

    /// The chunk plan.
    #[must_use]
    pub const fn plan(&self) -> Plan {
        self.plan
    }

    /// The current state.
    #[must_use]
    pub const fn state(&self) -> State {
        self.state
    }

    /// The chunk currently being written.
    #[must_use]
    pub const fn index(&self) -> u32 {
        self.index
    }

    /// Image length in bytes.
    #[must_use]
    pub fn blob_len(&self) -> usize {
        self.blob.len()
    }

    /// Return to [`State::Idle`] after a fault (or before a new attempt).
    pub fn reset(&mut self) {
        self.state = State::Idle;
        self.index = 0;
        self.wrote = false;
        self.rang = false;
        self.timeouts = 0;
        self.busy = 0;
    }

    /// Perform one seam operation.
    ///
    /// # Errors
    ///
    /// Returns [`FwError`] on a seam error, an exhausted timeout/busy budget, a device fault or
    /// when the machine is already faulted.
    pub fn step(&mut self, seam: &mut dyn FirmwareSeam) -> Result<Step, FwError> {
        match self.state {
            State::Idle => {
                if let Err(error) = seam.reset() {
                    return Err(self.fault(FwError::Seam(error)));
                }
                self.state = State::WaitingBootRom;
                self.timeouts = 0;
                self.busy = 0;
                Ok(Step::Reset)
            }
            State::WaitingBootRom => match seam.boot_ready() {
                Ok(true) => {
                    self.state = State::Transferring;
                    self.index = 0;
                    self.wrote = false;
                    self.rang = false;
                    self.timeouts = 0;
                    Ok(Step::PollBootRom)
                }
                Ok(false) => {
                    self.wait_timeout()?;
                    Ok(Step::PollBootRom)
                }
                Err(SeamError::Timeout) => {
                    self.wait_timeout()?;
                    Ok(Step::PollBootRom)
                }
                Err(error) => Err(self.fault(FwError::Seam(error))),
            },
            State::Transferring => self.transfer(seam),
            State::Verifying => match seam.status() {
                Ok(Status::Ready) => {
                    if let Err(error) = seam.start() {
                        return Err(self.fault(FwError::Seam(error)));
                    }
                    self.state = State::Running;
                    Ok(Step::Start)
                }
                Ok(Status::Busy) => {
                    self.wait_busy()?;
                    Ok(Step::PollStatus)
                }
                Ok(Status::Fault(code)) => Err(self.fault(FwError::DeviceFault(code))),
                Err(SeamError::Timeout) => {
                    self.wait_timeout()?;
                    Ok(Step::PollStatus)
                }
                Err(error) => Err(self.fault(FwError::Seam(error))),
            },
            State::Running => Ok(Step::Done),
            State::Faulted => Err(FwError::Faulted),
        }
    }

    /// Run to completion, returning the chunk and step counts.
    ///
    /// # Errors
    ///
    /// Returns the first [`FwError`] from [`Loader::step`], or [`FwError::Loop`] when the seam
    /// does not settle within [`MAX_STEPS`].
    pub fn run(&mut self, seam: &mut dyn FirmwareSeam) -> Result<LoadOutcome, FwError> {
        let mut steps = 0u32;
        loop {
            let step = self.step(seam)?;
            steps = steps.saturating_add(1);
            if matches!(step, Step::Done) {
                return Ok(LoadOutcome {
                    chunks: self.plan.chunks(),
                    steps,
                });
            }
            if steps > MAX_STEPS {
                return Err(self.fault(FwError::Loop));
            }
        }
    }

    fn transfer(&mut self, seam: &mut dyn FirmwareSeam) -> Result<Step, FwError> {
        let Some(chunk) = self.plan.chunk(self.index) else {
            self.state = State::Verifying;
            return Ok(Step::PollStatus);
        };
        if !self.wrote {
            let start = chunk.offset as usize;
            let end = start.saturating_add(chunk.length as usize);
            let Some(data) = self.blob.get(start..end) else {
                return Err(self.fault(FwError::PlanMismatch {
                    plan: self.plan.blob_len(),
                    blob: self.blob.len(),
                }));
            };
            if let Err(error) = seam.write_chunk(chunk.offset, data) {
                return Err(self.fault(FwError::Seam(error)));
            }
            self.wrote = true;
            return Ok(Step::WriteChunk(chunk));
        }
        if !self.rang {
            if let Err(error) = seam.doorbell(chunk.offset) {
                return Err(self.fault(FwError::Seam(error)));
            }
            self.rang = true;
            return Ok(Step::Doorbell(chunk));
        }
        match seam.status() {
            Ok(Status::Ready) => {
                self.index = self.index.saturating_add(1);
                self.wrote = false;
                self.rang = false;
                self.busy = 0;
                self.timeouts = 0;
                if self.index >= self.plan.chunks() {
                    self.state = State::Verifying;
                }
                Ok(Step::PollStatus)
            }
            Ok(Status::Busy) => {
                self.wait_busy()?;
                Ok(Step::PollStatus)
            }
            Ok(Status::Fault(code)) => Err(self.fault(FwError::DeviceFault(code))),
            Err(SeamError::Timeout) => {
                self.wait_timeout()?;
                Ok(Step::PollStatus)
            }
            Err(error) => Err(self.fault(FwError::Seam(error))),
        }
    }

    fn wait_timeout(&mut self) -> Result<(), FwError> {
        self.timeouts = self.timeouts.saturating_add(1);
        if self.timeouts > self.limits.max_timeouts {
            return Err(self.fault(FwError::Timeout));
        }
        Ok(())
    }

    fn wait_busy(&mut self) -> Result<(), FwError> {
        self.busy = self.busy.saturating_add(1);
        if self.busy > self.limits.max_busy_polls {
            return Err(self.fault(FwError::Busy));
        }
        Ok(())
    }

    fn fault(&mut self, error: FwError) -> FwError {
        self.state = State::Faulted;
        error
    }
}

#[cfg(test)]
#[path = "loader_ut.rs"]
mod tests;
