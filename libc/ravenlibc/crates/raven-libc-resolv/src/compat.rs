// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `res_*` compatibility layer. There is no `_res` global and no `h_errno` global: the
//! caller owns the state and the error value (T-12-15).

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use raven_libc_abi::errno::{EAGAIN, EINVAL, ENOENT, ENOMEM};

use crate::conf::{parse_options_ext, parse_resolv_conf};
use crate::consts::{HOST_NOT_FOUND, NO_DATA, NO_RECOVERY, TRY_AGAIN};
use crate::engine::msend;
use crate::msg::res_mkquery;
use crate::traits::Env;

pub use crate::errors::h_errno_from_rcode;

/// The always-empty `__res_state` replacement; `res_init` is a no-op.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ResState {
    /// The last query ID (always zero).
    pub id: u16,
    /// The resolver options (always zero).
    pub options: u32,
    /// The retransmission timeout (always zero).
    pub retrans: u32,
    /// The retry count (always zero).
    pub retry: u32,
}

/// A `res_*` failure: an errno plus the `h_errno` the donor would have set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResError {
    /// The C errno value.
    pub errno: crate::Errno,
    /// The legacy `h_errno` value.
    pub h_errno: i32,
}

impl ResError {
    const fn new(errno: crate::Errno, h_errno: i32) -> Self {
        Self { errno, h_errno }
    }
}

/// `res_init`: a no-op returning zero.
#[must_use]
pub const fn res_init() -> i32 {
    0
}

/// `__res_state`: zeros, never a global.
#[must_use]
pub const fn res_state() -> ResState {
    ResState {
        id: 0,
        options: 0,
        retrans: 0,
        retry: 0,
    }
}

/// `__res_send`.
pub fn res_send(msg: &[u8], answer: &mut [u8], env: &Env) -> Result<usize, ResError> {
    let conf = parse_resolv_conf(env.read("/etc/resolv.conf").ok().as_deref());
    let ext = parse_options_ext(env.read("/etc/resolv.conf").ok().as_deref().unwrap_or(&[]));
    if answer.len() < 512 {
        let mut temporary = alloc::vec![0u8; 512];
        let queries: [&[u8]; 1] = [msg];
        let mut answers: [&mut [u8]; 1] = [temporary.as_mut_slice()];
        let mut alens = [0usize; 1];
        msend(env, &conf, &ext, &queries, &mut answers, &mut alens)
            .map_err(|errno| ResError::new(errno, TRY_AGAIN))?;
        let length = *alens.first().unwrap_or(&0);
        if length == 0 {
            return Err(ResError::new(EAGAIN, TRY_AGAIN));
        }
        let copy = length.min(answer.len());
        if let (Some(target), Some(source)) = (answer.get_mut(..copy), temporary.get(..copy)) {
            target.copy_from_slice(source);
        }
        return Ok(length);
    }
    let queries: [&[u8]; 1] = [msg];
    let mut answers: [&mut [u8]; 1] = [answer];
    let mut alens = [0usize; 1];
    msend(env, &conf, &ext, &queries, &mut answers, &mut alens)
        .map_err(|errno| ResError::new(errno, TRY_AGAIN))?;
    match alens.first().copied().unwrap_or(0) {
        0 => Err(ResError::new(EAGAIN, TRY_AGAIN)),
        length => Ok(length),
    }
}

fn query_error(errno: crate::Errno, herror: i32) -> ResError {
    ResError::new(errno, herror)
}

/// `res_query`.
pub fn res_query(
    name: &[u8],
    class: u16,
    rtype: u16,
    dest: &mut [u8],
    env: &Env,
) -> Result<usize, ResError> {
    let mut id_bytes = [0u8; 2];
    if !env.random(&mut id_bytes) {
        return Err(query_error(EAGAIN, NO_RECOVERY));
    }
    let mut query = [0u8; 280];
    let Some(qlen) = res_mkquery(
        0,
        name,
        class,
        rtype,
        u16::from_be_bytes(id_bytes),
        &mut query,
    ) else {
        return Err(query_error(EINVAL, NO_RECOVERY));
    };
    let Some(query) = query.get(..qlen) else {
        return Err(query_error(EINVAL, NO_RECOVERY));
    };
    let length = res_send(query, dest, env)?;
    if length < 12 {
        return Err(query_error(EAGAIN, TRY_AGAIN));
    }
    let rcode = crate::u8_at(dest, 3) & 15;
    if rcode == 3 {
        return Err(query_error(ENOENT, HOST_NOT_FOUND));
    }
    if rcode == 0 && crate::be16_at(dest, 6) == Some(0) {
        return Err(query_error(ENOENT, NO_DATA));
    }
    Ok(length)
}

/// `res_search`: musl aliases `res_query` (A-net-5); the real search list lives in the
/// resolver proper.
pub fn res_search(
    name: &[u8],
    class: u16,
    rtype: u16,
    dest: &mut [u8],
    env: &Env,
) -> Result<usize, ResError> {
    res_query(name, class, rtype, dest, env)
}

/// `res_querydomain`.
pub fn res_querydomain(
    name: &[u8],
    domain: &[u8],
    class: u16,
    rtype: u16,
    dest: &mut [u8],
    env: &Env,
) -> Result<usize, ResError> {
    let nl = name.len().min(255);
    let dl = domain.len().min(255);
    if nl.saturating_add(dl).saturating_add(1) > 254 {
        return Err(query_error(EINVAL, NO_RECOVERY));
    }
    let mut joined: Vec<u8> = Vec::with_capacity(nl.saturating_add(dl).saturating_add(2));
    joined.extend_from_slice(name.get(..nl).unwrap_or(&[]));
    joined.push(b'.');
    joined.extend_from_slice(domain.get(..dl).unwrap_or(&[]));
    res_query(&joined, class, rtype, dest, env)
}

/// `hstrerror`: the musl table.
const HSTRERROR_MESSAGES: [&str; 4] = [
    "Host not found",
    "Try again",
    "Non-recoverable error",
    "Address not available",
];

/// The `hstrerror` text; unknown values give `"Unknown error"`.
#[must_use]
pub fn hstrerror(code: i32) -> &'static str {
    if !(1..=4).contains(&code) {
        return "Unknown error";
    }
    let index = usize::try_from(code.saturating_sub(1)).unwrap_or(0);
    HSTRERROR_MESSAGES
        .get(index)
        .copied()
        .unwrap_or("Unknown error")
}

/// The `herror` text (the shell writes it); no file descriptor is touched here.
#[must_use]
pub fn herror_text(message: Option<&str>, code: i32) -> String {
    match message {
        Some(text) => format!("{text}: {}", hstrerror(code)),
        None => String::from(hstrerror(code)),
    }
}

/// `herror`: the donor prints this text to stderr; the shell owns the stream.
#[must_use]
pub fn herror(message: Option<&str>, code: i32) -> String {
    herror_text(message, code)
}

/// `ENOMEM` is the donor mapping for an allocation failure in the `_r` service path.
#[must_use]
pub const fn service_memory_error() -> crate::Errno {
    ENOMEM
}

#[cfg(test)]
#[path = "compat_ut.rs"]
mod tests;
