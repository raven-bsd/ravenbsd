// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme server and client framework for RavenBSD.
//!
//! Servers implement [`Handler`], call [`Server::register`] and run [`Server::run`]. The server
//! loop receives S11 packets, validates them with `raven-ipc`, dispatches the typed operation and
//! sends the response. [`Completer`] supports deferred replies for servers such as `consd`.
//! Filesystem servers keep open-file descriptions in [`OpenTable`] and override
//! [`Handler::open_with`] to receive validated K66 [`raven_proto_fs::OpenFlags`].
//!
//! Clients use [`Client`] for `OPEN READ WRITE CLOSE DUP FSTAT CALL`. The real implementation is
//! [`RtKernel`]; host tests and downstream services can supply a fake [`Kernel`] instead. The
//! The `testing` feature exposes `testing::FakeKernel`.
//!
//! Captured client buffers are exposed as slices only for the duration of one handler call, as
//! required by the S11 capture-window contract. The crate is `no_std` plus `alloc`, has no
//! dependencies outside the path crates `raven-abi`, `raven-ipc` and `raven-proto-fs`, and does
//! not use global mutable state.

#![cfg_attr(not(test), no_std)]
#![allow(unsafe_code)] // the spin lock and S11 capture slices; every block has a SAFETY comment.
#![deny(unsafe_op_in_unsafe_fn)]
#![warn(missing_docs)]

extern crate alloc;

pub mod client;
pub mod kernel;
pub mod ofd;
pub mod server;
pub mod sync;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

#[cfg(test)]
mod client_tests;
#[cfg(test)]
mod server_tests;

mod sys;

pub use client::Client;
pub use kernel::{Kernel, RtKernel};
pub use ofd::{OpenFile, OpenTable};
pub use server::{Completer, Ctx, Handler, HandlerError, HandlerResult, Opened, Packet, Server};
pub use sync::{SpinGuard, SpinLock};
