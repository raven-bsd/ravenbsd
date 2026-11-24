// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host-testable pty master/slave state.
//!
//! A pty pair is one canonical/raw line discipline plus two byte queues. Master writes enter the
//! discipline and emerge on the slave (echo goes back to the master); slave writes appear on the
//! master. A Ctrl-C event is delivered as SIGINT to the foreground process group through the
//! caller-supplied signal sink.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod pairs;
pub mod pty;

pub use pairs::{Pairs, inode};
pub use pty::{Pty, SignalSink};
