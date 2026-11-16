// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Driver-side release network acceptance.
//!
//! [`ReleaseSession`] joins the frame backend's debounced link event to the host-tested
//! [`raven_genet::release::ReleaseGate`]. The board integration feeds PHY samples through
//! [`FrameBackend::observe_link`], records the DHCP lease `netd` applied and records the
//! `ping <gateway>` result. The resulting transcript is the evidence consumed by
//! `scripts/pi4-net-evidence.sh`; this module does not start the network stack or touch MMIO.

use crate::frame::FrameBackend;
use crate::{DriverError, Seam};
use raven_genet::link::LinkEvent;
use raven_genet::release::{Lease, PingReport, ReleaseGate, ReleaseTranscript};

/// One release network gate tied to a live frame backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReleaseSession {
    gate: ReleaseGate,
}

impl ReleaseSession {
    /// A session waiting for the first debounced link-up event.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            gate: ReleaseGate::new(),
        }
    }

    /// The underlying gate.
    #[must_use]
    pub const fn gate(&self) -> &ReleaseGate {
        &self.gate
    }

    /// The evidence transcript once the link, DHCP and ping stages are complete.
    #[must_use]
    pub fn transcript(&self) -> Option<ReleaseTranscript> {
        self.gate.transcript()
    }

    /// Feed one debounced link event to the release gate.
    pub fn link_event(&mut self, event: LinkEvent) -> Result<(), DriverError> {
        self.gate.link_event(event).map_err(DriverError::Release)?;
        Ok(())
    }

    /// Feed one BMSR/AUXSTS sample through the frame backend and the release gate.
    ///
    /// The backend applies a debounced transition to `UMAC_CMD`/`EXT_RGMII_OOB_CTRL`; the
    /// release gate then records the up event. A link-down transition is reported as
    /// [`DriverError::Release`].
    pub fn observe_link<S: Seam>(
        &mut self,
        backend: &mut FrameBackend<S>,
        bmsr: u16,
        auxsts: u16,
    ) -> Result<Option<LinkEvent>, DriverError> {
        let event = backend.observe_link(bmsr, auxsts)?;
        if let Some(event) = event {
            self.link_event(event)?;
        }
        Ok(event)
    }

    /// Record the DHCP lease that `netd` applied.
    pub fn dhcp_bound(&mut self, lease: Lease) -> Result<(), DriverError> {
        self.gate.dhcp_bound(lease).map_err(DriverError::Release)
    }

    /// Record the `ping <gateway>` result.
    pub fn ping_reply(&mut self, report: PingReport) -> Result<(), DriverError> {
        self.gate.ping_reply(report).map_err(DriverError::Release)
    }
}

impl Default for ReleaseSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "release_ut.rs"]
mod tests;
