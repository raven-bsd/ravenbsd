// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic pf-compatible packet filter for `netd`.
//!
//! The crate is the host-testable core of the packet filter: a pf.conf-compatible ruleset
//! parser, a stateful IP-family-independent match engine, NAT/redirect, named tables, a
//! bounded connection table with pf timeouts, `PFRES_*` counters, bounded logging and the
//! `pf.h` constants a pfctl client needs. It is `no_std` + `alloc`, has no dependency and
//! forbids `unsafe`.
//!
//! Donor: FreeBSD `sys/netpfil/pf` (`pf.c`, `pf_lb.c`, `pf_table.c`, `pf.h`) at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`, all BSD-2-Clause. The exact file/function map
//! and the deviations are in `docs/pf.md` and the ticket report.
//!
//! `netd` is not edited by this crate's ticket. The filter is a pure seam:
//! [`Engine::evaluate`] takes a [`Packet`] and a monotonic `now` in seconds and returns a
//! [`Decision`]; `netd` translates its stack events into packets, applies
//! [`Decision::translation`] and acts on the verdict before forwarding. The follow-up hook is
//! named in `docs/pf.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)] // `pf::...` types are already namespaced

extern crate alloc;

pub mod abi;
pub mod address;
pub mod ast;
pub mod engine;
pub mod log;
pub mod nat;
pub mod parser;
pub mod state;
pub mod table;

pub use address::{AddrSpec, IpAddr, Ipv4Addr, Ipv6Addr, PortSpec};
pub use ast::{
    Action, AddrPort, Direction, Family, IfaceSpec, Packet, Protocol, Rule, Ruleset, StatePolicy,
};
pub use engine::{CounterSnapshot, Decision, Engine, MAX_RULES, RuleStatSnapshot, Verdict};
pub use log::{LogEntry, MAX_LOG_ENTRIES, PacketLog};
pub use nat::{MAX_NAT_RULES, NatAction, NatError, NatRule, NatTable, Pool, PoolKind, Translation};
pub use parser::{ParseError, parse};
pub use state::{
    MAX_STATES, Orientation, StateDisposition, StateEntry, StateKey, StateTable, TcpState, Timeouts,
};
pub use table::{MAX_TABLE_ENTRIES, MAX_TABLES, Table, TableError};
