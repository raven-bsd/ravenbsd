// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! E-Ink service core for the Pi 4 7.5-inch panel.
//!
//! The service fetches a page, renders it into the E1 framebuffer, sends it through the UC8179
//! encoder and schedules the next refresh using a NTP-corrected [`Clock`]. It starts only after
//! the caller reports login/network readiness, so the panel path never blocks getty or the boot
//! gate. Fetch and panel failures keep the last-good frame and use exponential backoff; a clean
//! [`Service::shutdown`] stops all further access.
//!
//! All external effects are seams: [`Content`], [`Panel`], [`Clock`] and [`Font`] are injected.
//! The host tests use mocks and a recorded `Transport`; E3 supplies HTTP fetch and the glyph
//! table, E4 the Pi 4 SPI/GPIO and panel evidence. This crate makes no hardware/network claim.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod config;
mod content;
mod error;
mod service;

pub use config::{Config, PanelProfile};
pub use content::{Content, Font, Page};
pub use error::Error;
pub use service::{Action, Clock, Panel, Service, State, Uc8179Panel};

/// Service name used by the initfs manifest and the supervision graph.
pub const SERVICE_NAME: &str = "einkd";
