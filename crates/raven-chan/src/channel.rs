// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The channel state machine.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use crate::{Endpoint, Error, Limits};

/// A call that the peer has received and not yet completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ReceivedCall {
    /// The identifier the caller chose for this call. Never zero.
    pub txid: u64,
    /// The caller's opaque payload identifier; this crate never interprets it.
    pub payload_id: u64,
}

/// How a call completed at its caller.
///
/// Exactly one completion is recorded for every accepted call, on the endpoint that issued the
/// call. The caller reaps it with [`Channel::pop_completion`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Completion {
    /// The peer replied; `payload_id` is the reply payload.
    Replied {
        /// The call this completion belongs to.
        txid: u64,
        /// The reply payload chosen by the peer.
        payload_id: u64,
    },
    /// The caller cancelled the call before it completed.
    Cancelled {
        /// The call this completion belongs to.
        txid: u64,
    },
    /// The caller's timeout fired before the call completed.
    TimedOut {
        /// The call this completion belongs to.
        txid: u64,
    },
    /// An endpoint closed while the call was in flight.
    PeerClosed {
        /// The call this completion belongs to.
        txid: u64,
    },
    /// The peer's owner died while the call was in flight.
    PeerDied {
        /// The call this completion belongs to.
        txid: u64,
    },
}

impl Completion {
    /// The call this completion belongs to.
    pub const fn txid(self) -> u64 {
        match self {
            Self::Replied { txid, .. }
            | Self::Cancelled { txid }
            | Self::TimedOut { txid }
            | Self::PeerClosed { txid }
            | Self::PeerDied { txid } => txid,
        }
    }

    /// Whether this completion delivered a reply payload.
    pub const fn is_reply(self) -> bool {
        matches!(self, Self::Replied { .. })
    }
}

/// A call waiting in the peer's receive queue.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct QueuedCall {
    caller: Endpoint,
    txid: u64,
    payload_id: u64,
}

/// A call the peer has received and not completed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ReceivedEntry {
    caller: Endpoint,
    payload_id: u64,
}

/// Where an accepted call is before it completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Outbound {
    /// In the peer's receive queue.
    Queued,
    /// In the peer's received table.
    Received,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct EndpointState {
    closed: bool,
    dead: bool,
    inbound: VecDeque<QueuedCall>,
    received: BTreeMap<u64, ReceivedEntry>,
    outstanding: BTreeMap<u64, Outbound>,
    completions: VecDeque<Completion>,
    accepted: usize,
    completed: usize,
    popped: usize,
}

impl EndpointState {
    fn new() -> Self {
        Self {
            closed: false,
            dead: false,
            inbound: VecDeque::new(),
            received: BTreeMap::new(),
            outstanding: BTreeMap::new(),
            completions: VecDeque::new(),
            accepted: 0,
            completed: 0,
            popped: 0,
        }
    }

    fn is_closed(&self) -> bool {
        self.closed || self.dead
    }

    /// Accepted calls not reaped yet: outstanding plus unread completions.
    fn reap_pending(&self) -> usize {
        self.outstanding
            .len()
            .saturating_add(self.completions.len())
    }
}

/// The two endpoints' call/reply state over a bounded in-flight budget.
///
/// See the crate documentation for the model and the exactly-once invariant.
#[derive(Clone, Debug)]
#[cfg_attr(test, derive(PartialEq, Eq, Hash))]
pub struct Channel {
    endpoints: [EndpointState; 2],
    limits: Limits,
}

impl Default for Channel {
    fn default() -> Self {
        Self::new()
    }
}

impl Channel {
    /// Create a channel with the default limits.
    pub fn new() -> Self {
        Self::with_limits(Limits::default())
    }

    /// Create a channel with explicit limits, clamped to `1..=MAX_CAPACITY`.
    pub fn with_limits(limits: Limits) -> Self {
        Self {
            endpoints: [EndpointState::new(), EndpointState::new()],
            limits: limits.clamped(),
        }
    }

    /// The effective limits after clamping.
    pub const fn limits(&self) -> Limits {
        self.limits
    }

    /// Queue a call at the peer endpoint.
    ///
    /// On success the call is outstanding at `from` until it is replied to, cancelled, timed
    /// out, or an endpoint closes or dies. A failure changes no state and creates no completion.
    pub fn call(&mut self, from: Endpoint, txid: u64, payload_id: u64) -> Result<(), Error> {
        let peer = from.peer();
        {
            let caller = self.state(from);
            let target = self.state(peer);
            if caller.is_closed() {
                return Err(Error::EndpointClosed);
            }
            if target.dead {
                return Err(Error::PeerDead);
            }
            if target.closed {
                return Err(Error::PeerClosed);
            }
            if txid == 0 {
                return Err(Error::InvalidTxid);
            }
            if caller.outstanding.contains_key(&txid) {
                return Err(Error::DuplicateTxid);
            }
            if caller.reap_pending() >= self.limits.in_flight {
                return Err(Error::InFlightFull);
            }
            if target.inbound.len() >= self.limits.receive_queue {
                return Err(Error::QueueFull);
            }
        }
        let caller = self.state_mut(from);
        caller.outstanding.insert(txid, Outbound::Queued);
        caller.accepted = caller.accepted.saturating_add(1);
        self.state_mut(peer).inbound.push_back(QueuedCall {
            caller: from,
            txid,
            payload_id,
        });
        Ok(())
    }

    /// Receive the oldest queued call at `at` and move it to the received table.
    ///
    /// `WouldBlock` means the queue is empty but the peer is still reachable; `PeerClosed` and
    /// `PeerDead` mean the queue is empty and no further call will arrive.
    pub fn recv(&mut self, at: Endpoint) -> Result<ReceivedCall, Error> {
        if self.state(at).is_closed() {
            return Err(Error::EndpointClosed);
        }
        let Some(call) = self.state_mut(at).inbound.pop_front() else {
            return Err(self.empty_recv_error(at));
        };
        self.state_mut(at).received.insert(
            call.txid,
            ReceivedEntry {
                caller: call.caller,
                payload_id: call.payload_id,
            },
        );
        if let Some(slot) = self.state_mut(call.caller).outstanding.get_mut(&call.txid) {
            *slot = Outbound::Received;
        }
        Ok(ReceivedCall {
            txid: call.txid,
            payload_id: call.payload_id,
        })
    }

    /// Reply to a call that `at` has received but not completed.
    ///
    /// The reply payload is delivered to the caller as [`Completion::Replied`]. A reply for an
    /// unknown, queued, already-completed, cancelled or timed-out `txid` fails with
    /// [`Error::NoSuchCall`] and changes no state.
    pub fn reply(&mut self, at: Endpoint, txid: u64, payload_id: u64) -> Result<(), Error> {
        if self.state(at).is_closed() {
            return Err(Error::EndpointClosed);
        }
        let Some(entry) = self.state(at).received.get(&txid).copied() else {
            return Err(Error::NoSuchCall);
        };
        if !self.state(entry.caller).outstanding.contains_key(&txid) {
            return Err(Error::NoSuchCall);
        }
        self.state_mut(at).received.remove(&txid);
        self.state_mut(entry.caller).outstanding.remove(&txid);
        self.complete(entry.caller, Completion::Replied { txid, payload_id });
        Ok(())
    }

    /// Reply to the current call and receive the next one in one operation.
    ///
    /// If the reply fails, no call is dequeued and the result is the same error as
    /// [`Channel::reply`]. If it succeeds, the next queued call is returned already moved to the
    /// received table, or `None` when no call is queued. A peer that has closed or died is then
    /// discovered by the next [`Channel::recv`] (a closed peer completes every in-flight call, so
    /// its queue and received table are empty by construction).
    pub fn reply_recv(
        &mut self,
        at: Endpoint,
        txid: u64,
        payload_id: u64,
    ) -> Result<Option<ReceivedCall>, Error> {
        self.reply(at, txid, payload_id)?;
        if !self.state(at).inbound.is_empty() {
            return self.recv(at).map(Some);
        }
        Ok(None)
    }

    /// Cancel an outstanding call that `from` issued.
    ///
    /// The call completes with [`Completion::Cancelled`]. If it was still queued it is removed
    /// from the peer's queue, so the peer never observes it; if the peer had already received it,
    /// a later reply is rejected.
    pub fn cancel(&mut self, from: Endpoint, txid: u64) -> Result<(), Error> {
        self.abort(from, txid, false)
    }

    /// Complete an outstanding call with [`Completion::TimedOut`], with the same cleanup as
    /// [`Channel::cancel`].
    pub fn timeout(&mut self, from: Endpoint, txid: u64) -> Result<(), Error> {
        self.abort(from, txid, true)
    }

    /// Close `at`. Every in-flight call on the channel completes with
    /// [`Completion::PeerClosed`], and the peer can no longer call this endpoint.
    ///
    /// Idempotent: a second close records no further completion.
    pub fn close(&mut self, at: Endpoint) {
        if self.state(at).is_closed() {
            return;
        }
        self.state_mut(at).closed = true;
        self.fail_all(false);
    }

    /// Record that the owner of `at` died. Every in-flight call on the channel completes with
    /// [`Completion::PeerDied`], and the peer can no longer call this endpoint.
    ///
    /// Idempotent: a second death records no further completion.
    pub fn died(&mut self, at: Endpoint) {
        if self.state(at).dead {
            return;
        }
        self.state_mut(at).dead = true;
        self.state_mut(at).closed = true;
        self.fail_all(true);
    }

    /// Reap the oldest completion recorded for `at`.
    pub fn pop_completion(&mut self, at: Endpoint) -> Option<Completion> {
        let state = self.state_mut(at);
        let completion = state.completions.pop_front()?;
        state.popped = state.popped.saturating_add(1);
        Some(completion)
    }

    /// Whether the endpoint itself is closed or dead.
    pub fn is_closed(&self, at: Endpoint) -> bool {
        self.state(at).is_closed()
    }

    /// Whether the endpoint's owner is recorded as dead.
    pub fn is_dead(&self, at: Endpoint) -> bool {
        self.state(at).dead
    }

    /// Whether both endpoints are closed (the pair may be destroyed).
    pub fn both_closed(&self) -> bool {
        self.state(Endpoint::A).is_closed() && self.state(Endpoint::B).is_closed()
    }

    /// Calls waiting to be received at `at`.
    pub fn queued(&self, at: Endpoint) -> usize {
        self.state(at).inbound.len()
    }

    /// Calls `at` has received and not completed.
    pub fn received(&self, at: Endpoint) -> usize {
        self.state(at).received.len()
    }

    /// Calls issued by `at` and not completed yet.
    pub fn outstanding(&self, at: Endpoint) -> usize {
        self.state(at).outstanding.len()
    }

    /// Completions recorded for `at` and not reaped yet.
    pub fn pending_completions(&self, at: Endpoint) -> usize {
        self.state(at).completions.len()
    }

    /// Total calls accepted from `at` since the channel was created.
    pub fn accepted_calls(&self, at: Endpoint) -> usize {
        self.state(at).accepted
    }

    /// Total completions recorded for `at` since the channel was created.
    pub fn completed_calls(&self, at: Endpoint) -> usize {
        self.state(at).completed
    }

    /// Total completions reaped by `at` since the channel was created.
    pub fn popped_completions(&self, at: Endpoint) -> usize {
        self.state(at).popped
    }

    /// Whether `at` has an outstanding call with `txid`.
    pub fn is_outstanding(&self, at: Endpoint, txid: u64) -> bool {
        self.state(at).outstanding.contains_key(&txid)
    }

    /// Whether `at` has received a call with `txid` and not completed it.
    pub fn is_received(&self, at: Endpoint, txid: u64) -> bool {
        self.state(at).received.contains_key(&txid)
    }

    /// The outstanding `txid`s of `at`, in ascending order.
    pub fn outstanding_txids(&self, at: Endpoint) -> impl Iterator<Item = u64> + '_ {
        self.state(at).outstanding.keys().copied()
    }

    /// The unreaped completions of `at`, oldest first.
    pub fn completions(&self, at: Endpoint) -> impl Iterator<Item = Completion> + '_ {
        self.state(at).completions.iter().copied()
    }

    fn state(&self, at: Endpoint) -> &EndpointState {
        &self.endpoints[at.index()]
    }

    fn state_mut(&mut self, at: Endpoint) -> &mut EndpointState {
        &mut self.endpoints[at.index()]
    }

    fn empty_recv_error(&self, at: Endpoint) -> Error {
        let peer = self.state(at.peer());
        if peer.dead {
            Error::PeerDead
        } else if peer.closed {
            Error::PeerClosed
        } else {
            Error::WouldBlock
        }
    }

    fn abort(&mut self, from: Endpoint, txid: u64, timed_out: bool) -> Result<(), Error> {
        if self.state(from).is_closed() {
            return Err(Error::EndpointClosed);
        }
        let Some(location) = self.state(from).outstanding.get(&txid).copied() else {
            return Err(Error::NoSuchCall);
        };
        let peer = from.peer();
        match location {
            Outbound::Queued => {
                self.state_mut(peer)
                    .inbound
                    .retain(|call| !(call.caller == from && call.txid == txid));
            }
            Outbound::Received => {
                self.state_mut(peer).received.remove(&txid);
            }
        }
        self.state_mut(from).outstanding.remove(&txid);
        let completion = if timed_out {
            Completion::TimedOut { txid }
        } else {
            Completion::Cancelled { txid }
        };
        self.complete(from, completion);
        Ok(())
    }

    fn fail_all(&mut self, peer_died: bool) {
        for at in Endpoint::ALL {
            let txids: Vec<u64> = self.state(at).outstanding.keys().copied().collect();
            for txid in txids {
                self.state_mut(at).outstanding.remove(&txid);
                let completion = if peer_died {
                    Completion::PeerDied { txid }
                } else {
                    Completion::PeerClosed { txid }
                };
                self.complete(at, completion);
            }
            let state = self.state_mut(at);
            state.inbound.clear();
            state.received.clear();
        }
    }

    fn complete(&mut self, at: Endpoint, completion: Completion) {
        let state = self.state_mut(at);
        state.completed = state.completed.saturating_add(1);
        state.completions.push_back(completion);
    }
}

#[cfg(test)]
#[path = "channel_ut.rs"]
mod tests;
