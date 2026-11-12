// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The OS-independent RavenBSD TCP/IP stack boundary (contract C15).
//!
//! The native stack is a standalone, OS-independent library. It never calls the kernel, never reads a
//! clock and never allocates frames from a driver. Everything crosses the boundary defined here.
//!
//! **Time.** [`Instant`] and [`Duration`] are plain wrappers over `i64`/`u64` milliseconds. The
//! caller supplies `now` to every entry point; the stack never asks for the time.
//!
//! **Link layer.** The stack owns no driver. It talks to one trait, ours (not smoltcp's):
//! [`Device`]. [`Device::receive`] hands the next frame to a callback and returns `true`, or returns
//! `false` when the queue is empty. [`Device::transmit`] reserves `len` bytes, lets a callback write
//! them, then queues the frame.
//!
//! **Driving the stack.** [`Stack::poll`] is the only thing that moves packets or runs timers.
//! [`Stack::next_poll_delay`] returns `None` when nothing is scheduled. [`Stack::take_events`]
//! reports readiness **transitions**, not levels.
//!
//! [`Event`] keeps the shape `raven-netcore` already publishes, so the socket layer above does not
//! change when the backend changes. Errors are [`Errno`], mapped exactly as B07 mapped them.
//!
//! **Socket API.** [`Stack::tcp_listen`], [`Stack::tcp_accept`], [`Stack::tcp_connect`],
//! [`Stack::send`], [`Stack::recv`], [`Stack::shutdown_write`], [`Stack::close`],
//! [`Stack::udp_bind`], [`Stack::udp_send_to`] and [`Stack::udp_recv_from`] are all non-blocking,
//! all `Result<_, Errno>`, `EAGAIN` for "would block", and `Ok(0)` from `recv` for an orderly EOF.
//!
//! **Rules.** `no_std` + `alloc`, `#![forbid(unsafe_code)]`, no threads, no interior wall-clock
//! reads, no `unwrap`/`expect` on anything that came off the wire. No public type from any
//! third-party crate appears in the public API.
//!
//! **Memory limits.** Every queue has a byte cap from [`StackConfig`]. Exceeding it drops and counts;
//! it never allocates without bound and never panics. No buffer is ever sized from a value the peer
//! supplied.

#![no_std]
#![forbid(unsafe_code)]
#![allow(missing_docs)] // Socket operation names are documented at module level; C15 is copied above.

extern crate alloc;

mod cc;
mod device;
mod event;
mod icmp_stack;
mod ipv4_stack;
mod ipv6_stack;
mod neighbor;
mod reassembly;
mod recovery;
mod route;
mod rto;
mod sack;
mod seq;
mod socket;
mod stack;
mod tcp_data;
mod tcp_state;
mod time;
mod udp_stack;

pub use raven_abi::errno::Errno;

pub use cc::{CcContext, CongestionControl, Cubic, LossKind, NewReno, Prr, icbrt};
pub use device::{ChecksumOffload, Device, DeviceCaps, Medium, TxError};
pub use event::{Event, PollResult, SocketId};
pub use ipv6_stack::{
    IpAddr, Ipv6Addr, Ipv6AddressState, Ipv6Cidr, Ipv6Config, Neighbor6Cache, Neighbor6Entry,
    Neighbor6State,
};
pub use neighbor::{NeighborCache, NeighborEntry};
pub use recovery::{Ecn, Rack, SpuriousUndo, Tlp, pto};
pub use route::{Route, RouteTable};
pub use rto::RtoEstimator;
pub use sack::{NextKind, NextSeg, SackResult, SackScoreboard, receiver_blocks, sack_option_bytes};
pub use stack::{Ipv4Addr, Ipv4Config, MAX_TIMERS, Stack, StackConfig, TimerId};
pub use tcp_state::TcpState;
pub use time::{Duration, Instant};

#[cfg(any(test, feature = "testutil"))]
pub mod testutil;

#[cfg(any(test, feature = "testutil"))]
pub use testutil::{LoopbackDevice, NullDevice};
