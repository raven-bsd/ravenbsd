// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! An in-memory RP1 boot-ROM/load-window double.
//!
//! `SimSeam` holds a copy of the expected image (up to [`MAX_IMAGE`]), accepts chunk writes and
//! doorbells, and answers the status polls. The tests configure readiness, busy polls, a fault
//! code or a bus error to exercise the loader's failure paths. This is a model, not a device.

use crate::seam::{FirmwareSeam, SeamError, Status};

/// Largest synthetic image the double holds.
pub const MAX_IMAGE: usize = 256;

/// The in-memory seam.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimSeam<'a> {
    expected: &'a [u8],
    image: [u8; MAX_IMAGE],
    written: usize,
    ready: bool,
    busy_polls: u8,
    fault: Option<u16>,
    started: bool,
    resets: u32,
    doorbells: u32,
    fail_write: bool,
    fail_reset: bool,
    fail_boot: bool,
    status_error: Option<SeamError>,
}

impl<'a> SimSeam<'a> {
    /// Build a double for `expected`, or `None` when it exceeds [`MAX_IMAGE`].
    #[must_use]
    pub fn new(expected: &'a [u8]) -> Option<Self> {
        if expected.len() > MAX_IMAGE {
            return None;
        }
        Some(Self {
            expected,
            image: [0; MAX_IMAGE],
            written: 0,
            ready: true,
            busy_polls: 0,
            fault: None,
            started: false,
            resets: 0,
            doorbells: 0,
            fail_write: false,
            fail_reset: false,
            fail_boot: false,
            status_error: None,
        })
    }

    /// Set whether the boot ROM reports ready.
    pub fn set_ready(&mut self, ready: bool) {
        self.ready = ready;
    }

    /// Answer `Busy` this many times before `Ready`.
    pub fn set_busy_polls(&mut self, polls: u8) {
        self.busy_polls = polls;
    }

    /// Report a device fault code on every status poll.
    pub fn set_fault(&mut self, fault: Option<u16>) {
        self.fault = fault;
    }

    /// Fail every chunk write with a bus error.
    pub fn set_fail_write(&mut self, fail: bool) {
        self.fail_write = fail;
    }

    /// Fail the reset with a bus error.
    pub fn set_fail_reset(&mut self, fail: bool) {
        self.fail_reset = fail;
    }

    /// Fail the boot-ready poll with a bus error.
    pub fn set_fail_boot(&mut self, fail: bool) {
        self.fail_boot = fail;
    }

    /// Fail the status poll with `error`.
    pub fn set_status_error(&mut self, error: Option<SeamError>) {
        self.status_error = error;
    }

    /// Bytes written so far (the highest end offset).
    #[must_use]
    pub const fn written(&self) -> usize {
        self.written
    }

    /// True when the loader called `start`.
    #[must_use]
    pub const fn started(&self) -> bool {
        self.started
    }

    /// Reset calls.
    #[must_use]
    pub const fn resets(&self) -> u32 {
        self.resets
    }

    /// Doorbell calls.
    #[must_use]
    pub const fn doorbells(&self) -> u32 {
        self.doorbells
    }

    /// The image bytes written so far.
    #[must_use]
    pub fn image(&self) -> &[u8] {
        self.image.get(..self.written).unwrap_or_default()
    }

    /// True when the written bytes equal the expected image.
    #[must_use]
    pub fn verify(&self) -> bool {
        self.written >= self.expected.len()
            && self.image.get(..self.expected.len()) == Some(self.expected)
    }
}

impl FirmwareSeam for SimSeam<'_> {
    fn reset(&mut self) -> Result<(), SeamError> {
        if self.fail_reset {
            return Err(SeamError::Bus);
        }
        self.image = [0; MAX_IMAGE];
        self.written = 0;
        self.started = false;
        self.resets = self.resets.saturating_add(1);
        Ok(())
    }

    fn boot_ready(&mut self) -> Result<bool, SeamError> {
        if self.fail_boot {
            return Err(SeamError::Bus);
        }
        Ok(self.ready)
    }

    fn write_chunk(&mut self, offset: u32, data: &[u8]) -> Result<(), SeamError> {
        if self.fail_write {
            return Err(SeamError::Bus);
        }
        let start = offset as usize;
        let end = start.checked_add(data.len()).ok_or(SeamError::Bus)?;
        if end > MAX_IMAGE {
            return Err(SeamError::Bus);
        }
        if let Some(slot) = self.image.get_mut(start..end) {
            slot.copy_from_slice(data);
        }
        self.written = self.written.max(end);
        Ok(())
    }

    fn doorbell(&mut self, _offset: u32) -> Result<(), SeamError> {
        self.doorbells = self.doorbells.saturating_add(1);
        Ok(())
    }

    fn status(&mut self) -> Result<Status, SeamError> {
        if let Some(error) = self.status_error {
            return Err(error);
        }
        if let Some(code) = self.fault {
            return Ok(Status::Fault(code));
        }
        if self.busy_polls > 0 {
            self.busy_polls -= 1;
            return Ok(Status::Busy);
        }
        Ok(Status::Ready)
    }

    fn start(&mut self) -> Result<(), SeamError> {
        self.started = true;
        Ok(())
    }
}

#[cfg(test)]
#[path = "sim_ut.rs"]
mod tests;
