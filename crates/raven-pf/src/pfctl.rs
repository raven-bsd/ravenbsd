// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The first-party `pfctl` client ABI: a small bounded request/response codec.
//!
//! This is not a kernel ioctl. The userspace `pfctl` client and the `raven-pf` owner exchange
//! this record inside netd; the format is stable and independent of the FreeBSD `pfioc_*` structs,
//! which are kernel/mbuf shaped.

use alloc::vec::Vec;

use crate::engine::CounterSnapshot;

/// ABI version.
pub const ABI_VERSION: u16 = 1;
/// Largest accepted request/response payload.
pub const MAX_PAYLOAD: usize = 64 * 1024;

/// One request from `pfctl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Replace the ruleset text.
    Load(Vec<u8>),
    /// Read the global counters.
    Stats,
    /// Reset the global counters.
    ClearStats,
    /// Read the state table occupancy.
    StateCount,
    /// Read one rule's counters.
    RuleStats(u32),
}

/// One response to `pfctl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    /// Request accepted.
    Ok,
    /// Global counters.
    Stats(CounterSnapshot),
    /// State table occupancy.
    StateCount(u32),
    /// One rule's counters.
    RuleStats(u64, u64, u64, u64),
    /// Request rejected; the code is stable.
    Error(u16),
}

const LOAD: u16 = 1;
const STATS: u16 = 2;
const CLEAR: u16 = 3;
const STATE_COUNT: u16 = 4;
const RULE_STATS: u16 = 5;
const OK: u16 = 0x8000;
const ERROR: u16 = 0x8001;

/// Encode a request into `out`.
#[must_use]
pub fn encode_request(request: &Request, out: &mut [u8]) -> Option<usize> {
    match request {
        Request::Load(text) => {
            let length = u16::try_from(text.len()).ok()?;
            if text.len() > MAX_PAYLOAD {
                return None;
            }
            let total = 4usize.checked_add(text.len())?;
            out.get_mut(..2)?.copy_from_slice(&LOAD.to_le_bytes());
            out.get_mut(2..4)?.copy_from_slice(&length.to_le_bytes());
            out.get_mut(4..total)?.copy_from_slice(text);
            Some(total)
        }
        Request::Stats => Some(tag_only(STATS, out)?),
        Request::ClearStats => Some(tag_only(CLEAR, out)?),
        Request::StateCount => Some(tag_only(STATE_COUNT, out)?),
        Request::RuleStats(index) => {
            out.get_mut(..2)?.copy_from_slice(&RULE_STATS.to_le_bytes());
            out.get_mut(2..6)?.copy_from_slice(&index.to_le_bytes());
            Some(6)
        }
    }
}

fn tag_only(tag: u16, out: &mut [u8]) -> Option<usize> {
    out.get_mut(..2)?.copy_from_slice(&tag.to_le_bytes());
    Some(2)
}

/// Decode a request.
#[must_use]
pub fn decode_request(bytes: &[u8]) -> Option<Request> {
    let tag = u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?);
    match tag {
        LOAD => {
            let length = usize::from(u16::from_le_bytes(bytes.get(2..4)?.try_into().ok()?));
            if length > MAX_PAYLOAD {
                return None;
            }
            Some(Request::Load(
                bytes.get(4..4usize.checked_add(length)?)?.to_vec(),
            ))
        }
        STATS => Some(Request::Stats),
        CLEAR => Some(Request::ClearStats),
        STATE_COUNT => Some(Request::StateCount),
        RULE_STATS => Some(Request::RuleStats(u32::from_le_bytes(
            bytes.get(2..6)?.try_into().ok()?,
        ))),
        _ => None,
    }
}

/// Encode a response.
#[must_use]
pub fn encode_response(response: &Response, out: &mut [u8]) -> Option<usize> {
    match response {
        Response::Ok => tag_only(OK, out),
        Response::Stats(stats) => {
            out.get_mut(..2)?.copy_from_slice(&STATS.to_le_bytes());
            let mut offset = 2usize;
            for value in [
                stats.in_pass,
                stats.in_block,
                stats.out_pass,
                stats.out_block,
                stats.states_created,
                stats.states_expired,
                stats.state_hits,
            ] {
                out.get_mut(offset..offset + 8)?
                    .copy_from_slice(&value.to_le_bytes());
                offset = offset.checked_add(8)?;
            }
            Some(offset)
        }
        Response::StateCount(count) => {
            out.get_mut(..2)?
                .copy_from_slice(&STATE_COUNT.to_le_bytes());
            out.get_mut(2..6)?.copy_from_slice(&count.to_le_bytes());
            Some(6)
        }
        Response::RuleStats(evaluations, matches, packets, bytes) => {
            out.get_mut(..2)?.copy_from_slice(&RULE_STATS.to_le_bytes());
            let mut offset = 2usize;
            for value in [*evaluations, *matches, *packets, *bytes] {
                out.get_mut(offset..offset + 8)?
                    .copy_from_slice(&value.to_le_bytes());
                offset = offset.checked_add(8)?;
            }
            Some(offset)
        }
        Response::Error(code) => {
            out.get_mut(..2)?.copy_from_slice(&ERROR.to_le_bytes());
            out.get_mut(2..4)?.copy_from_slice(&code.to_le_bytes());
            Some(4)
        }
    }
}

/// Decode a response.
#[must_use]
pub fn decode_response(bytes: &[u8]) -> Option<Response> {
    let tag = u16::from_le_bytes(bytes.get(..2)?.try_into().ok()?);
    match tag {
        OK => Some(Response::Ok),
        STATS => {
            let mut values = [0u64; 7];
            for (index, value) in values.iter_mut().enumerate() {
                let offset = 2usize.checked_add(index.checked_mul(8)?)?;
                *value = u64::from_le_bytes(bytes.get(offset..offset + 8)?.try_into().ok()?);
            }
            Some(Response::Stats(CounterSnapshot {
                in_pass: values[0],
                in_block: values[1],
                out_pass: values[2],
                out_block: values[3],
                states_created: values[4],
                states_expired: values[5],
                state_hits: values[6],
            }))
        }
        STATE_COUNT => Some(Response::StateCount(u32::from_le_bytes(
            bytes.get(2..6)?.try_into().ok()?,
        ))),
        RULE_STATS => {
            let mut values = [0u64; 4];
            for (index, value) in values.iter_mut().enumerate() {
                let offset = 2usize.checked_add(index.checked_mul(8)?)?;
                *value = u64::from_le_bytes(bytes.get(offset..offset + 8)?.try_into().ok()?);
            }
            Some(Response::RuleStats(
                values[0], values[1], values[2], values[3],
            ))
        }
        ERROR => Some(Response::Error(u16::from_le_bytes(
            bytes.get(2..4)?.try_into().ok()?,
        ))),
        _ => None,
    }
}

#[cfg(test)]
#[path = "pfctl_ut.rs"]
mod tests;
