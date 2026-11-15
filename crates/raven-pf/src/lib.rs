// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic pf-like packet filter for `netd`.
//!
//! The crate is the host-testable core of the M5 packet filter (`architecture/07-network.md`
//! section 1): a pf-like ruleset parser, a stateless/stateful match engine, a bounded connection
//! table with pf timeouts and per-rule/global counters. It is `no_std` + `alloc`, has no
//! dependency and forbids `unsafe`.
//!
//! `netd` is not edited by this crate's ticket. The filter is a pure seam:
//! [`Engine::evaluate`] takes a [`Packet`] and a monotonic `now` in seconds and returns a
//! [`Decision`]; `netd` will translate its stack events into packets and act on the verdict
//! before forwarding. The follow-up hook is named in `docs/pf.md`.
//!
//! This is a clean-room implementation written from the published pf/npf behaviour; no donor
//! code was copied. The donor files consulted are listed in `docs/pf.md` and in the ticket
//! report.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)] // `pf::...` types are already namespaced

extern crate alloc;

pub mod address;
pub mod ast;
pub mod engine;
pub mod parser;
pub mod state;

pub use address::{AddrSpec, Ipv4Addr, PortSpec};
pub use ast::{
    Action, AddrPort, Direction, IfaceSpec, Packet, Protocol, Rule, Ruleset, StatePolicy,
};
pub use engine::{CounterSnapshot, Decision, Engine, MAX_RULES, RuleStatSnapshot, Verdict};
pub use parser::{ParseError, parse};
pub use state::{
    MAX_STATES, Orientation, StateDisposition, StateEntry, StateKey, StateTable, TcpState, Timeouts,
};
