// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Futex operations handled locally by the shim.

use raven_abi::errno::Errno;
use raven_abi::types::Timespec;
use raven_linux_abi::flags::futex as flags;

use crate::Host;

const NANOS_PER_SEC: u64 = 1_000_000_000;

/// Linux `futex`.
///
/// `FUTEX_WAIT` and `FUTEX_WAIT_BITSET` use relative and absolute timeouts respectively. The
/// bitset mask is validated but the host wait primitive is a single 32-bit word, so masked wait
/// semantics are not forwarded yet.
pub fn futex(
    host: &mut dyn Host,
    addr: usize,
    op: u32,
    val: u32,
    timeout: Option<Timespec>,
    _addr2: usize,
    val3: u32,
) -> Result<isize, Errno> {
    if !addr.is_multiple_of(4) {
        return Err(Errno::EINVAL);
    }
    let command = op & !(flags::FUTEX_PRIVATE_FLAG | flags::FUTEX_CLOCK_REALTIME);
    match command {
        flags::FUTEX_WAIT => {
            let timeout_ns = match timeout {
                Some(timeout) => Some(relative_ns(timeout)?),
                None => None,
            };
            host.futex_wait(addr, val, timeout_ns)?;
            Ok(0)
        }
        flags::FUTEX_WAKE => Ok(host.futex_wake(addr, val) as isize),
        flags::FUTEX_WAIT_BITSET => {
            if val3 == 0 {
                return Err(Errno::EINVAL);
            }
            let timeout_ns = match timeout {
                Some(timeout) => {
                    let absolute = absolute_ns(timeout)?;
                    let native = if op & flags::FUTEX_CLOCK_REALTIME != 0 {
                        0
                    } else {
                        1
                    };
                    let (seconds, nanos) = host.clock_get(native)?;
                    let now = absolute_from_parts(seconds, nanos)?;
                    if absolute <= now {
                        return Err(Errno::ETIMEDOUT);
                    }
                    Some(absolute - now)
                }
                None => None,
            };
            host.futex_wait(addr, val, timeout_ns)?;
            Ok(0)
        }
        flags::FUTEX_WAKE_BITSET => {
            if val3 == 0 {
                return Err(Errno::EINVAL);
            }
            Ok(host.futex_wake(addr, val) as isize)
        }
        _ => Err(Errno::ENOSYS),
    }
}

fn relative_ns(timeout: Timespec) -> Result<u64, Errno> {
    let seconds = u64::try_from(timeout.tv_sec).map_err(|_| Errno::EINVAL)?;
    let nanos = u64::try_from(timeout.tv_nsec).map_err(|_| Errno::EINVAL)?;
    if nanos >= NANOS_PER_SEC {
        return Err(Errno::EINVAL);
    }
    seconds
        .checked_mul(NANOS_PER_SEC)
        .and_then(|value| value.checked_add(nanos))
        .ok_or(Errno::EINVAL)
}

fn absolute_ns(timeout: Timespec) -> Result<u64, Errno> {
    absolute_from_parts(timeout.tv_sec, timeout.tv_nsec)
}

fn absolute_from_parts(seconds: i64, nanos: i64) -> Result<u64, Errno> {
    let seconds = u64::try_from(seconds).map_err(|_| Errno::EINVAL)?;
    let nanos = u64::try_from(nanos).map_err(|_| Errno::EINVAL)?;
    if nanos >= NANOS_PER_SEC {
        return Err(Errno::EINVAL);
    }
    seconds
        .checked_mul(NANOS_PER_SEC)
        .and_then(|value| value.checked_add(nanos))
        .ok_or(Errno::EINVAL)
}
