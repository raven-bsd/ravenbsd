// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Pure call/reply channel state machine for the native IPC ABI.
//!
//! A [`Channel`] pairs two [`Endpoint`]s and tracks the calls they exchange. It is the policy
//! half of the kernel `Channel` object: send/receive bookkeeping, the `txid` matching table, the
//! error contract and exactly-once completion. It deliberately knows nothing about wire formats,
//! handles, rights, scheduling or syscall numbers; the later kernel integration ticket supplies
//! those around it.
//!
//! # Model
//!
//! * [`Channel::call`] queues a call at the peer endpoint and records it as outstanding at the
//!   caller. Calls from one endpoint to the other are delivered in send order.
//! * [`Channel::recv`] moves the oldest queued call to the received table. Only a received call
//!   may be replied to.
//! * [`Channel::reply`] matches the `txid`, removes it from both tables and records a
//!   [`Completion`] at the caller. Replies for different `txid`s may be sent in any order.
//! * [`Channel::cancel`] and [`Channel::timeout`] abort a call that has not completed, whether it
//!   is still queued or already received. A late reply then finds no call.
//! * [`Channel::close`] and [`Channel::died`] are terminal endpoint events. They complete every
//!   in-flight call on the channel with [`Error::PeerClosed`] and [`Error::PeerDead`] semantics.
//!
//! # Exactly-once completion
//!
//! An accepted call exists in exactly one of two places at any time: the caller's outstanding
//! table or the caller's completion queue. Every completion path removes the outstanding entry
//! before it records the completion, so a call cannot complete twice and a rejected call never
//! completes. The invariant is checked by the exhaustive state exploration in the tests.
//!
//! # Bounds
//!
//! Both queues are bounded. The receive queue limits calls that have not been received yet; the
//! in-flight budget limits calls that the caller has not reaped yet, whether they are still
//! outstanding or already completed in the caller's completion queue. A caller that never drains
//! completions eventually gets [`Error::InFlightFull`] instead of unbounded kernel memory. There
//! is no allocation on the reply path that the caller does not control.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

mod channel;
mod endpoint;
mod error;
mod limits;
mod msgport;

pub use channel::{Channel, Completion, ReceivedCall};
pub use endpoint::Endpoint;
pub use error::Error;
pub use limits::{DEFAULT_IN_FLIGHT, DEFAULT_RECEIVE_QUEUE, Limits, MAX_CAPACITY};
pub use msgport::{
    MSGF_ABORTABLE, MSGF_DONE, MSGF_DROPABLE, MSGF_INTRANSIT, MSGF_PRIORITY, MSGF_QUEUED,
    MSGF_RECEIPT, MSGF_REPLY, MSGF_SYNC, MSGF_WAITING, MSGPORTF_WAITING, Message, MsgError,
    ReplyPort, SendResult, TargetPort, WaitOutcome, abort, cancel, domsg, dropmsg, recv, reply,
    sendmsg, waitmsg,
};
