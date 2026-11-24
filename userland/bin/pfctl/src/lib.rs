// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RavenBSD `pfctl` front-end: pf.conf parser, AST, canonical printer and table
//! file parser.
//!
//! This crate is the host-testable front-end of the FreeBSD `pfctl` port
//! (K274). It parses the pf.conf subset named by the ticket, keeps a faithful
//! AST, prints the config in the compact form `pfctl -nv` produces, and parses
//! table files. The kernel status/ioctl path waits for the pf ABI (K273): this
//! crate never talks to netd and never loads a ruleset.
//!
//! The grammar and output rules are ports of the permissive-licensed FreeBSD
//! donor `sbin/pfctl` at `133344405f` (BSD-2-Clause): `parse.y` (grammar),
//! `pfctl_parser.c` (`print_rule`, `print_fromto`, `print_port`, `print_op`,
//! `print_flags`, `print_tabledef`), `pfctl_table.c` (table file input) and
//! `pfctl.c` (`-n`/`-v` dry run). The Rust is first-party; no donor code is
//! copied. Donor functions and the divergences are listed in `docs/pfctl.md`.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![warn(missing_docs)]

extern crate alloc;

pub mod ast;
pub mod cli;
pub mod lexer;
pub mod parser;
pub mod print;
pub mod table;

pub use ast::{
    Action, AddrItem, AddrPortSpec, AddressFamily, Config, Direction, FlagsSpec, HostAddr,
    IfaceRef, NatKind, NatRule, OptionDirective, Policy, PolicyValue, PoolOption, PortSpec,
    Protocol, Redirect, Rule, StateMode, StatePolicyValue, Statement, TableDef, TableEntry,
    TableFlag,
};
pub use cli::{Outcome, run};
pub use parser::{
    FileLoader, ParseError, ParseOptions, parse_config, parse_config_with_loader, parse_with,
};
pub use print::print_config;
pub use table::{TableFile, parse_table_file};
