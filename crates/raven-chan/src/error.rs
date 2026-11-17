// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Why a channel operation was rejected.

/// A channel operation failure.
///
/// The kernel integration maps these onto its syscall error values; this crate keeps its own
/// names so the pure model does not depend on the ABI crate. The mapping is documented in
/// `docs/ipc.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Error {
    /// `txid` zero is reserved for one-way messages and cannot name a call.
    InvalidTxid,
    /// The calling endpoint already has an outstanding call with this `txid`.
    DuplicateTxid,
    /// The caller has reached its in-flight budget, including unread completions.
    InFlightFull,
    /// The peer's receive queue is full.
    QueueFull,
    /// No call matches the `txid`, or it is not in the state the operation requires.
    NoSuchCall,
    /// The receive queue is empty and the peer is still reachable.
    WouldBlock,
    /// The endpoint performing the operation is already closed (or dead).
    EndpointClosed,
    /// The peer endpoint is closed.
    PeerClosed,
    /// The peer endpoint's owner died.
    PeerDead,
}

impl Error {
    /// A stable short name for logs and telemetry.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidTxid => "invalid-txid",
            Self::DuplicateTxid => "duplicate-txid",
            Self::InFlightFull => "in-flight-full",
            Self::QueueFull => "queue-full",
            Self::NoSuchCall => "no-such-call",
            Self::WouldBlock => "would-block",
            Self::EndpointClosed => "endpoint-closed",
            Self::PeerClosed => "peer-closed",
            Self::PeerDead => "peer-dead",
        }
    }
}

impl core::fmt::Display for Error {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
