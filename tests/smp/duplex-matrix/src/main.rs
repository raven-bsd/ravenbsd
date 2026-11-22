// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K496 duplex semantics matrix.
//!
//! The suite measures the kernel duplex contract rather than hiding it: ownership and rights of a
//! queued handle, self/two/three-object reference cycles, control truncation and positioning,
//! 1-byte/zero/peer-shutdown reads, failures after handle validation or dequeue, and a 10k-op
//! seeded stateful fuzz. Every row prints `ok=yes|no`; a deviation is a finding, not a crash. The
//! summary hash is stable over the row/name/observation tuple.

#![no_std]
#![no_main]
#![allow(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_abi::types::{Handle, Rights, prot};
use raven_rt::entry;
use raven_rt::pipe::{O_NONBLOCK, pipe};
use raven_rt::println;
use raven_rt::sys;

entry!(main);

/// Raw duplex pair number.
const DUPLEX_PAIR: usize = 0x110;
/// Raw duplex shutdown number.
const DUPLEX_SHUTDOWN: usize = 0x111;
/// Raw duplex sendmsg number.
const DUPLEX_SENDMSG: usize = 0x112;
/// Raw duplex recvmsg number.
const DUPLEX_RECVMSG: usize = 0x113;
/// Raw `HANDLE_REVOKE` number.
const HANDLE_REVOKE: usize = 0x53;
/// Raw `PROTECT` number.
const PROTECT: usize = 0x42;

/// Diagnostic flag for `duplex_pair`: write the ownership counters instead of creating a pair.
const DUPLEX_DIAGNOSTIC: u32 = 0x8000_0000;
/// Diagnostic flag for `duplex_pair`: run one cycle-collection pass, then write the counters.
const DUPLEX_COLLECT: u32 = 0x4000_0000;
/// Datagram mode.
const DUPLEX_DATAGRAM: u32 = 1;
/// `MSG_NOSIGNAL` (suppress the SIGPIPE queue on `EPIPE`).
const DUPLEX_MSG_NOSIGNAL: u32 = 0x4000;
/// `MSG_WAITALL` (FreeBSD/NetBSD 0x40): refused explicitly by the raw recv path.
const MSG_WAITALL: u32 = 0x40;
/// `MSG_CTRUNC` (FreeBSD/NetBSD 0x20): set on receive when ancillary data did not fit.
const MSG_CTRUNC: u32 = 0x20;
/// `SOL_SOCKET`.
const SOL_SOCKET: u32 = 1;
/// `SCM_RIGHTS`.
const SCM_RIGHTS: u32 = 1;
/// `SHUT_RD`.
const SHUT_RD: u8 = 0;
/// `SHUT_WR`.
const SHUT_WR: u8 = 1;

/// One control header.
const CMSG_BYTES: usize = 16;
/// Seeded fuzz length.
const FUZZ_OPS: usize = 10_000;
/// Fuzz seed (printed and hashed).
const SEED: u64 = 0x4b34_3936_4455_5000;

/// User-visible duplex message header.
#[repr(C)]
struct Message {
    data: u64,
    data_len: u64,
    control: u64,
    control_len: u64,
    flags: u32,
    handle_count: u32,
}

/// Ownership counters returned by the diagnostic query.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Stats {
    pairs: u64,
    ends: u64,
    pending: u64,
    controls: u64,
}

impl Stats {
    fn delta(self, before: Stats) -> Stats {
        Stats {
            pairs: self.pairs.saturating_sub(before.pairs),
            ends: self.ends.saturating_sub(before.ends),
            pending: self.pending.saturating_sub(before.pending),
            controls: self.controls.saturating_sub(before.controls),
        }
    }

    fn zero(self) -> bool {
        self.pairs == 0 && self.ends == 0 && self.pending == 0 && self.controls == 0
    }
}

/// Row table and stable trace hash.
struct Table {
    rows: usize,
    deviations: usize,
    hash: u64,
}

impl Table {
    fn new() -> Self {
        Self {
            rows: 0,
            deviations: 0,
            hash: 0xcbf2_9ce4_8422_2325,
        }
    }

    fn row(&mut self, group: &str, name: &str, ok: bool, value: u64) {
        if !ok {
            self.deviations = self.deviations.saturating_add(1);
        }
        self.hash = fnv(self.hash, group.as_bytes());
        self.hash = fnv(self.hash, name.as_bytes());
        self.hash = fnv(self.hash, &[u8::from(ok)]);
        self.hash = fnv(self.hash, &value.to_le_bytes());
        self.rows = self.rows.saturating_add(1);
        println!(
            "duplex-matrix: row group={group} name={name} ok={} value={value}",
            if ok { "yes" } else { "no" }
        );
    }

    fn summary(&self) {
        println!(
            "duplex-matrix: summary rows={} deviations={} seed=0x{SEED:016x} table-hash=0x{:016x}",
            self.rows, self.deviations, self.hash
        );
    }
}

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn raw(number: usize, args: [usize; 6]) -> isize {
    // SAFETY: every caller passes a documented raw number and either a live buffer or a deliberate
    // invalid value the kernel validates with `UserSlice`/fault-safe copy.
    unsafe { sys::syscall6(number, args[0], args[1], args[2], args[3], args[4], args[5]) }
}

fn checked(ret: isize) -> Result<usize, Errno> {
    Errno::from_ret(ret)
}

fn pair(flags: u32) -> Result<(Handle, Handle), Errno> {
    let mut handles = [0u32; 2];
    checked(raw(
        DUPLEX_PAIR,
        [handles.as_mut_ptr() as usize, flags as usize, 0, 0, 0, 0],
    ))?;
    Ok((Handle(handles[0]), Handle(handles[1])))
}

fn stats() -> Result<Stats, Errno> {
    let mut value = Stats::default();
    checked(raw(
        DUPLEX_PAIR,
        [
            (&mut value as *mut Stats) as usize,
            DUPLEX_DIAGNOSTIC as usize,
            0,
            0,
            0,
            0,
        ],
    ))?;
    Ok(value)
}

fn collect() -> Result<Stats, Errno> {
    let mut value = Stats::default();
    checked(raw(
        DUPLEX_PAIR,
        [
            (&mut value as *mut Stats) as usize,
            (DUPLEX_DIAGNOSTIC | DUPLEX_COLLECT) as usize,
            0,
            0,
            0,
            0,
        ],
    ))?;
    Ok(value)
}

fn sendmsg_raw(
    handle: Handle,
    data: usize,
    data_len: usize,
    control: usize,
    control_len: usize,
    flags: u32,
) -> Result<usize, Errno> {
    let mut message = Message {
        data: data as u64,
        data_len: data_len as u64,
        control: control as u64,
        control_len: control_len as u64,
        flags,
        handle_count: 0,
    };
    checked(raw(
        DUPLEX_SENDMSG,
        [
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        ],
    ))
}

fn sendmsg(handle: Handle, data: &[u8], control: &[u8], flags: u32) -> Result<usize, Errno> {
    sendmsg_raw(
        handle,
        data.as_ptr() as usize,
        data.len(),
        control.as_ptr() as usize,
        control.len(),
        flags,
    )
}

fn recvmsg(handle: Handle, data: &mut [u8], control: &mut [u8]) -> Result<(usize, Message), Errno> {
    let mut message = Message {
        data: data.as_mut_ptr() as u64,
        data_len: data.len() as u64,
        control: control.as_mut_ptr() as u64,
        control_len: control.len() as u64,
        flags: 0,
        handle_count: 0,
    };
    let copied = checked(raw(
        DUPLEX_RECVMSG,
        [
            handle.0 as usize,
            (&mut message as *mut Message) as usize,
            0,
            0,
            0,
            0,
        ],
    ))?;
    Ok((copied, message))
}

fn recvmsg_at(handle: Handle, message: *mut Message) -> Result<usize, Errno> {
    checked(raw(
        DUPLEX_RECVMSG,
        [handle.0 as usize, message as usize, 0, 0, 0, 0],
    ))
}

fn shutdown(handle: Handle, how: u8) -> Result<(), Errno> {
    checked(raw(
        DUPLEX_SHUTDOWN,
        [handle.0 as usize, how as usize, 0, 0, 0, 0],
    ))
    .map(|_| ())
}

fn revoke(handle: Handle, rights: Rights) -> Result<(), Errno> {
    checked(raw(
        HANDLE_REVOKE,
        [handle.0 as usize, rights.bits() as usize, 0, 0, 0, 0],
    ))
    .map(|_| ())
}

fn protect(address: usize, length: usize, protection: usize) -> Result<(), Errno> {
    checked(raw(
        PROTECT,
        [
            Handle::SELF_PROC.0 as usize,
            address,
            length,
            protection,
            0,
            0,
        ],
    ))
    .map(|_| ())
}

fn close(handle: Handle) {
    let _ = sys::handle_close(handle);
}

fn put_u32(buffer: &mut [u8], offset: usize, value: u32) {
    if let Some(slot) = buffer.get_mut(offset..offset.saturating_add(4)) {
        slot.copy_from_slice(&value.to_le_bytes());
    }
}

fn get_u32(buffer: &[u8], offset: usize) -> Result<u32, Errno> {
    let slot = buffer
        .get(offset..offset.saturating_add(4))
        .ok_or(Errno::EINVAL)?;
    let bytes: [u8; 4] = slot.try_into().map_err(|_| Errno::EINVAL)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Write one `SCM_RIGHTS` record at `offset`; return its length.
fn control_record(handles: &[Handle], buffer: &mut [u8], offset: usize) -> usize {
    let length = CMSG_BYTES.saturating_add(handles.len().saturating_mul(4));
    put_u32(buffer, offset, length as u32);
    put_u32(buffer, offset.saturating_add(4), SOL_SOCKET);
    put_u32(buffer, offset.saturating_add(8), SCM_RIGHTS);
    put_u32(buffer, offset.saturating_add(12), handles.len() as u32);
    for (index, handle) in handles.iter().enumerate() {
        put_u32(
            buffer,
            offset.saturating_add(CMSG_BYTES).saturating_add(index * 4),
            handle.0,
        );
    }
    length
}

fn control_one(handle: Handle, buffer: &mut [u8]) -> usize {
    control_record(&[handle], buffer, 0)
}

fn first_record_count(control: &[u8], control_len: usize) -> Result<(u32, u32), Errno> {
    if control_len < CMSG_BYTES {
        return Err(Errno::EINVAL);
    }
    let length = get_u32(control, 0)?;
    let level = get_u32(control, 4)?;
    let kind = get_u32(control, 8)?;
    let count = get_u32(control, 12)?;
    if length as usize != control_len || level != SOL_SOCKET || kind != SCM_RIGHTS {
        return Err(Errno::EINVAL);
    }
    Ok((count, get_u32(control, CMSG_BYTES)?))
}

/// Parse exactly two 20-byte `SCM_RIGHTS` records, one handle each (K521 donor shape).
fn two_single_records(control: &[u8], control_len: usize) -> Option<[Handle; 2]> {
    if control_len != 40 {
        return None;
    }
    let len1 = get_u32(control, 0).ok()?;
    let count1 = get_u32(control, 12).ok()?;
    let raw1 = get_u32(control, CMSG_BYTES).ok()?;
    let len2 = get_u32(control, 20).ok()?;
    let level2 = get_u32(control, 24).ok()?;
    let kind2 = get_u32(control, 28).ok()?;
    let count2 = get_u32(control, 32).ok()?;
    let raw2 = get_u32(control, 36).ok()?;
    if len1 != 20
        || count1 != 1
        || len2 != 20
        || level2 != SOL_SOCKET
        || kind2 != SCM_RIGHTS
        || count2 != 1
    {
        return None;
    }
    Some([Handle(raw1), Handle(raw2)])
}

fn observed(value: Stats) -> u64 {
    value
        .pairs
        .wrapping_mul(0x9e37_79b9)
        .wrapping_add(value.ends.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(value.pending.wrapping_mul(0xc2b2_ae35))
        .wrapping_add(value.controls.wrapping_mul(0x27d4_eb2f))
}

fn baseline(table: &mut Table, group: &str) {
    let value = stats();
    let ok = value.is_ok_and(|value| value.zero());
    table.row(group, "baseline", ok, value.map_or(u64::MAX, observed));
}

// ---------------------------------------------------------------------------------------------
// Group 1: ownership and rights.
// ---------------------------------------------------------------------------------------------

fn ownership(table: &mut Table) {
    baseline(table, "ownership");
    let base = stats().unwrap_or_default();

    // A delivers the write end of a pipe to B, A closes its copy, B receives and uses it.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"x", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(left);
        close(write_end);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let received = recvmsg(right, &mut data, &mut received_control);
        let ok = match received {
            Ok((1, message)) if message.handle_count == 1 => {
                let handle = first_record_count(&received_control, message.control_len as usize)
                    .map(|(count, raw)| (count == 1).then_some(Handle(raw)))
                    .ok()
                    .flatten();
                match handle {
                    Some(handle) => {
                        let wrote = sys::write(handle, b"ok") == Ok(2);
                        let mut reply = [0u8; 2];
                        let got = sys::read(read_end, &mut reply) == Ok(2) && &reply == b"ok";
                        close(handle);
                        wrote && got
                    }
                    None => false,
                }
            }
            _ => false,
        };
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default();
        table.row(
            "ownership",
            "deliver-close-source",
            sent && ok && after.delta(before).zero(),
            observed(after.delta(before)),
        );
    } else {
        table.row("ownership", "deliver-close-source", false, u64::MAX);
    }

    // The in-flight reference survives A's close, then the last B handle closes it exactly once.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"i", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(left);
        close(write_end);
        let pending = stats().unwrap_or_default().delta(before);
        let alive = pending.pending == 1 && pending.pairs == 1 && pending.ends == 1;
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        let eof = sys::read(read_end, &mut [0u8; 1]) == Ok(0);
        close(read_end);
        let value = u64::from(sent)
            | (u64::from(alive) << 1)
            | (u64::from(after.zero()) << 2)
            | (u64::from(eof) << 3);
        table.row(
            "ownership",
            "inflight-close-receiver",
            sent && alive && after.zero() && eof,
            value,
        );
    } else {
        table.row("ownership", "inflight-close-receiver", false, u64::MAX);
    }

    // Revoke WRITE before the receiver installs the handle: C17 EPERM.
    ownership_revoke(table, true);
    // Revoke WRITE after installation: the installed child sees the ancestor revocation.
    ownership_revoke(table, false);
    // A right that was never granted is EACCES, not EPERM.
    ownership_never_granted(table);
}

fn ownership_revoke(table: &mut Table, before_install: bool) {
    let before = stats().unwrap_or_default();
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"r", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        if before_install {
            let _ = revoke(write_end, Rights::WRITE);
        }
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let received = recvmsg(right, &mut data, &mut received_control);
        let (ok, after_revoke) = match received {
            Ok((1, message)) if message.handle_count == 1 => {
                let handle = first_record_count(&received_control, message.control_len as usize)
                    .ok()
                    .map(|(_, raw)| Handle(raw));
                match handle {
                    Some(handle) => {
                        if !before_install {
                            let _ = revoke(write_end, Rights::WRITE);
                        }
                        let denied = sys::write(handle, b"z") == Err(Errno::EPERM);
                        close(handle);
                        (denied, denied)
                    }
                    None => (false, false),
                }
            }
            _ => (false, false),
        };
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        let value = u64::from(sent)
            | (u64::from(ok) << 1)
            | (u64::from(after_revoke) << 2)
            | (u64::from(after.zero()) << 3);
        table.row(
            "ownership",
            if before_install {
                "revoke-before-install"
            } else {
                "revoke-after-install"
            },
            sent && ok && after_revoke && after.zero(),
            value,
        );
    } else {
        table.row("ownership", "revoke", false, u64::MAX);
    }
}

fn ownership_never_granted(table: &mut Table) {
    let before = stats().unwrap_or_default();
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let restricted = match sys::handle_dup(
            write_end,
            Rights::READ.union(Rights::DUP).union(Rights::TRANSFER),
        ) {
            Ok(handle) => handle,
            Err(_) => Handle::NONE,
        };
        let mut control = [0u8; 64];
        let control_len = control_one(restricted, &mut control);
        let sent = restricted != Handle::NONE
            && sendmsg(left, b"n", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let received = recvmsg(right, &mut data, &mut received_control);
        let denied = match received {
            Ok((1, message)) if message.handle_count == 1 => {
                match first_record_count(&received_control, message.control_len as usize) {
                    Ok((_, raw)) => {
                        let handle = Handle(raw);
                        let result = sys::write(handle, b"z");
                        close(handle);
                        result == Err(Errno::EACCES)
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        close(restricted);
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "ownership",
            "never-granted-eacces",
            sent && denied && after.zero(),
            observed(after),
        );
    } else {
        table.row("ownership", "never-granted-eacces", false, u64::MAX);
    }
}

// ---------------------------------------------------------------------------------------------
// Group 2: reference cycles.
// ---------------------------------------------------------------------------------------------

fn cycles(table: &mut Table) {
    baseline(table, "cycles");

    // Self-send: the pair's own queue holds one of its endpoints after every external handle is
    // closed. There is no collector, so the expected invariant is a measured failure.
    let before = stats().unwrap_or_default();
    if let Ok((left, right)) = pair(0) {
        let mut control = [0u8; 64];
        let control_len = control_one(right, &mut control);
        let sent = sendmsg(left, b"s", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(left);
        close(right);
        let collected = collect().is_ok();
        let after = stats().unwrap_or_default().delta(before);
        let value = u64::from(sent) | (u64::from(collected) << 1) | (u64::from(after.zero()) << 2);
        table.row(
            "cycles",
            "self-send-closed",
            sent && collected && after.zero(),
            value,
        );
    } else {
        table.row("cycles", "self-send-closed", false, u64::MAX);
    }

    // A reachable self-send must survive the collector: the receiving endpoint is still a live
    // handle, so its receive queue is a root and the queued endpoint is reachable.
    let before_reachable = stats().unwrap_or_default();
    if let Ok((left, right)) = pair(0) {
        let mut control = [0u8; 64];
        let control_len = control_one(right, &mut control);
        let sent = sendmsg(left, b"r", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(left);
        let collected_while_reachable = collect().is_ok();
        let retained = stats().unwrap_or_default().delta(before_reachable);
        let retained_ok = retained.pairs == 1 && retained.ends == 1 && retained.pending == 1;
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let delivered = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 1 => {
                match first_record_count(&received_control, message.control_len as usize) {
                    Ok((_, raw)) => {
                        close(Handle(raw));
                        true
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        close(right);
        let collected_after = collect().is_ok();
        let after = stats().unwrap_or_default().delta(before_reachable);
        let value = u64::from(sent)
            | (u64::from(collected_while_reachable) << 1)
            | (u64::from(retained_ok) << 2)
            | (u64::from(delivered) << 3)
            | (u64::from(collected_after) << 4)
            | (u64::from(after.zero()) << 5);
        table.row(
            "cycles",
            "self-send-reachable-retained",
            sent && collected_while_reachable
                && retained_ok
                && delivered
                && collected_after
                && after.zero(),
            value,
        );
    } else {
        table.row("cycles", "self-send-reachable-retained", false, u64::MAX);
    }

    // A reachable self-send is delivered and then reclaimed.
    let before_delivered = stats().unwrap_or_default();
    if let Ok((left, right)) = pair(0) {
        let mut control = [0u8; 64];
        let control_len = control_one(right, &mut control);
        let sent = sendmsg(left, b"s", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let received = recvmsg(right, &mut data, &mut received_control);
        let delivered = match received {
            Ok((1, message)) if message.handle_count == 1 => {
                match first_record_count(&received_control, message.control_len as usize) {
                    Ok((_, raw)) => {
                        close(Handle(raw));
                        true
                    }
                    Err(_) => false,
                }
            }
            _ => false,
        };
        close(left);
        close(right);
        let collected = collect().is_ok();
        let after = stats().unwrap_or_default().delta(before_delivered);
        table.row(
            "cycles",
            "self-send-delivered",
            sent && delivered && collected && after.zero(),
            observed(after),
        );
    } else {
        table.row("cycles", "self-send-delivered", false, u64::MAX);
    }

    // Two-object cycle: pair A's queue holds B's endpoint and pair B's queue holds A's.
    let before_two = stats().unwrap_or_default();
    if let (Ok((a_left, a_right)), Ok((b_left, b_right))) = (pair(0), pair(0)) {
        let mut control = [0u8; 64];
        let a_len = control_one(b_left, &mut control);
        let sent_a = sendmsg(a_left, b"a", &control[..a_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let b_len = control_one(a_left, &mut control);
        let sent_b = sendmsg(b_left, b"b", &control[..b_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(a_left);
        close(a_right);
        close(b_left);
        close(b_right);
        let collected = collect().is_ok();
        let after = stats().unwrap_or_default().delta(before_two);
        let value = u64::from(sent_a)
            | (u64::from(sent_b) << 1)
            | (u64::from(collected) << 2)
            | (u64::from(after.zero()) << 3);
        table.row(
            "cycles",
            "two-object-ring-closed",
            sent_a && sent_b && collected && after.zero(),
            value,
        );
    } else {
        table.row("cycles", "two-object-ring-closed", false, u64::MAX);
    }

    // Three-object ring A->B->C->A.
    let before_three = stats().unwrap_or_default();
    if let (Ok((a_left, a_right)), Ok((b_left, b_right)), Ok((c_left, c_right))) =
        (pair(0), pair(0), pair(0))
    {
        let mut control = [0u8; 64];
        let a_len = control_one(b_left, &mut control);
        let sent_a = sendmsg(a_left, b"a", &control[..a_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let b_len = control_one(c_left, &mut control);
        let sent_b = sendmsg(b_left, b"b", &control[..b_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let c_len = control_one(a_left, &mut control);
        let sent_c = sendmsg(c_left, b"c", &control[..c_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        close(a_left);
        close(a_right);
        close(b_left);
        close(b_right);
        close(c_left);
        close(c_right);
        let collected = collect().is_ok();
        let after = stats().unwrap_or_default().delta(before_three);
        let value = u64::from(sent_a)
            | (u64::from(sent_b) << 1)
            | (u64::from(sent_c) << 2)
            | (u64::from(collected) << 3)
            | (u64::from(after.zero()) << 4);
        table.row(
            "cycles",
            "three-object-ring-closed",
            sent_a && sent_b && sent_c && collected && after.zero(),
            value,
        );
    } else {
        table.row("cycles", "three-object-ring-closed", false, u64::MAX);
    }
}

// ---------------------------------------------------------------------------------------------
// Group 3: truncation and control positioning.
// ---------------------------------------------------------------------------------------------

fn truncation(table: &mut Table) {
    baseline(table, "truncation");
    let base = stats().unwrap_or_default();

    // A too-small control buffer is not an error: the data is delivered, the whole SCM_RIGHTS
    // record is dropped and MSG_CTRUNC reports the loss (NetBSD uipc_syscalls.c:867,
    // FreeBSD uipc_syscalls.c:1020). The message is consumed exactly once.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(O_NONBLOCK), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"t", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut small = [0u8; 8];
        let delivered = match recvmsg(right, &mut data, &mut small) {
            Ok((1, message)) => {
                data[0] == b't'
                    && message.flags & MSG_CTRUNC != 0
                    && message.handle_count == 0
                    && message.control_len == 0
            }
            _ => false,
        };
        let mut again = [0u8; 8];
        let consumed = matches!(recvmsg(right, &mut data, &mut again), Err(Errno::EAGAIN));
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "control-small-ctrunc",
            sent && delivered && consumed && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "control-small-ctrunc", false, u64::MAX);
    }

    // A datagram shorter than the user buffer is truncated and the whole message is consumed.
    if let Ok((left, right)) = pair(DUPLEX_DATAGRAM | O_NONBLOCK) {
        let before = base;
        let sent = sendmsg(left, b"hello", &[], DUPLEX_MSG_NOSIGNAL) == Ok(5);
        let mut small = [0u8; 2];
        let mut control = [0u8; 64];
        let first = recvmsg(right, &mut small, &mut control);
        let second = recvmsg(right, &mut small, &mut control);
        let ok = matches!(first, Ok((2, _))) && matches!(second, Err(Errno::EAGAIN));
        close(left);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "datagram-data-small",
            sent && ok && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "datagram-data-small", false, u64::MAX);
    }

    // K482 boundary: the control is attached to the byte at offset 1 and a plain read stops at
    // that exact offset; the following recvmsg must still see it.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let first = sendmsg(left, b"A", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let second = sendmsg(left, b"B", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut one = [0u8; 1];
        let read_a = sys::read(right, &mut one) == Ok(1) && one[0] == b'A';
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let boundary = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 1 => {
                data[0] == b'B'
                    && first_record_count(&received_control, message.control_len as usize).is_ok()
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "control-exact-offset",
            first && second && read_a && boundary && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "control-exact-offset", false, u64::MAX);
    }

    // Two SCM_RIGHTS records in one datagram stay two records with their own headers (K521).
    if let (Ok((left, right)), Ok((read_end, write_end)), Ok((read_end2, write_end2))) =
        (pair(DUPLEX_DATAGRAM | O_NONBLOCK), pipe(), pipe())
    {
        let before = base;
        let mut control = [0u8; 128];
        let first = control_record(&[write_end], &mut control, 0);
        let second = control_record(&[write_end2], &mut control, first);
        let total = first.saturating_add(second);
        let sent = sendmsg(left, b"2", &control[..total], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 128];
        let delivered = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 2 => {
                match two_single_records(&received_control, message.control_len as usize) {
                    Some([one, two]) => {
                        close(one);
                        close(two);
                        true
                    }
                    // A flattened count=2 record: consume both handles and report the divergence.
                    None => {
                        if message.control_len >= 20
                            && let Ok((count, _)) =
                                first_record_count(&received_control, message.control_len as usize)
                        {
                            for index in 0..count.min(2) {
                                if let Ok(raw) =
                                    get_u32(&received_control, CMSG_BYTES + index as usize * 4)
                                {
                                    close(Handle(raw));
                                }
                            }
                        }
                        false
                    }
                }
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(write_end2);
        close(right);
        close(read_end);
        close(read_end2);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "two-records-one-datagram",
            sent && delivered && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "two-records-one-datagram", false, u64::MAX);
    }

    // The same two records on a stream send: the data byte and both headers arrive together.
    if let (Ok((left, right)), Ok((read_end, write_end)), Ok((read_end2, write_end2))) =
        (pair(0), pipe(), pipe())
    {
        let before = base;
        let mut control = [0u8; 128];
        let first = control_record(&[write_end], &mut control, 0);
        let second = control_record(&[write_end2], &mut control, first);
        let total = first.saturating_add(second);
        let sent = sendmsg(left, b"s", &control[..total], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 128];
        let delivered = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 2 && data[0] == b's' => {
                match two_single_records(&received_control, message.control_len as usize) {
                    Some([one, two]) => {
                        close(one);
                        close(two);
                        true
                    }
                    None => {
                        if let Ok((count, _)) =
                            first_record_count(&received_control, message.control_len as usize)
                        {
                            for index in 0..count.min(2) {
                                if let Ok(raw) =
                                    get_u32(&received_control, CMSG_BYTES + index as usize * 4)
                                {
                                    close(Handle(raw));
                                }
                            }
                        }
                        false
                    }
                }
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(write_end2);
        close(right);
        close(read_end);
        close(read_end2);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "two-records-one-stream",
            sent && delivered && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "two-records-one-stream", false, u64::MAX);
    }

    // A control capacity exactly equal to the record length succeeds.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"e", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut exact = [0u8; CMSG_BYTES + 4];
        let exact_ok = match recvmsg(right, &mut data, &mut exact) {
            Ok((1, message)) if message.handle_count == 1 && message.control_len == 20 => {
                close(Handle(get_u32(&exact, CMSG_BYTES).unwrap_or(u32::MAX)));
                true
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "truncation",
            "control-exact-capacity",
            sent && exact_ok && after.zero(),
            observed(after),
        );
    } else {
        table.row("truncation", "control-exact-capacity", false, u64::MAX);
    }
}

// ---------------------------------------------------------------------------------------------
// Group 4: partial reads.
// ---------------------------------------------------------------------------------------------

fn partial(table: &mut Table) {
    baseline(table, "partial");
    let base = stats().unwrap_or_default();

    // recvmsg 1 byte at a time across the control byte: 'A', then 'B' + control, then 'C'.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let a = sendmsg(left, b"A", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let b = sendmsg(left, b"BCD", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(3);
        let mut data = [0u8; 1];
        let mut received_control = [0u8; 64];
        let first = recvmsg(right, &mut data, &mut received_control);
        let one = matches!(first, Ok((1, message)) if message.handle_count == 0 && data[0] == b'A');
        let second = recvmsg(right, &mut data, &mut received_control);
        let two = match second {
            Ok((1, message)) if message.handle_count == 1 => {
                data[0] == b'B'
                    && first_record_count(&received_control, message.control_len as usize).is_ok()
            }
            _ => false,
        };
        let handle = first_record_count(&received_control, 20)
            .ok()
            .map(|(_, raw)| Handle(raw));
        let third = recvmsg(right, &mut data, &mut received_control);
        let three =
            matches!(third, Ok((1, message)) if message.handle_count == 0 && data[0] == b'C');
        if let Some(handle) = handle {
            close(handle);
        }
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "partial",
            "recvmsg-one-byte",
            a && b && one && two && three && after.zero(),
            observed(after),
        );
    } else {
        table.row("partial", "recvmsg-one-byte", false, u64::MAX);
    }

    // A plain read that passes the control offset explicitly discards the record; the handle is
    // closed and the following recvmsg reports no control.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let a = sendmsg(left, b"A", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let b = sendmsg(left, b"B", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let c = sendmsg(left, b"C", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut one = [0u8; 1];
        let read_a = sys::read(right, &mut one) == Ok(1) && one[0] == b'A';
        let read_b = sys::read(right, &mut one) == Ok(1) && one[0] == b'B';
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let discarded = matches!(
            recvmsg(right, &mut data, &mut received_control),
            Ok((1, message)) if message.handle_count == 0 && data[0] == b'C'
        );
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "partial",
            "read-one-byte-discards-control",
            a && b && c && read_a && read_b && discarded && after.zero(),
            observed(after),
        );
    } else {
        table.row("partial", "read-one-byte-discards-control", false, u64::MAX);
    }

    // A zero-length read returns 0 and consumes nothing; the following recvmsg gets all of it.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"Z", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut empty = [0u8; 1];
        let zero = sys::read(right, &mut empty[..0]) == Ok(0);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let delivered = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 1 => {
                close(Handle(
                    first_record_count(&received_control, message.control_len as usize)
                        .map(|(_, raw)| raw)
                        .unwrap_or(u32::MAX),
                ));
                true
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        let value = u64::from(sent)
            | (u64::from(zero) << 1)
            | (u64::from(delivered) << 2)
            | (u64::from(after.zero()) << 3);
        table.row(
            "partial",
            "zero-length-read",
            sent && zero && delivered && after.zero(),
            value,
        );
    } else {
        table.row("partial", "zero-length-read", false, u64::MAX);
    }

    // Peer SHUT_WR: after the peer shuts its write side the queued byte is readable, then EOF.
    // The peer's read side stays open, so the reverse send is still accepted.
    if let Ok((left, right)) = pair(0) {
        let before = base;
        let sent = sendmsg(left, b"Q", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let shut = shutdown(left, SHUT_WR).is_ok();
        let mut one = [0u8; 1];
        let got = sys::read(right, &mut one) == Ok(1) && one[0] == b'Q';
        let eof = sys::read(right, &mut one) == Ok(0);
        let reverse = sendmsg(right, b"x", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let reverse_got = sys::read(left, &mut one) == Ok(1) && one[0] == b'x';
        close(left);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "partial",
            "peer-shut-wr",
            sent && shut && got && eof && reverse && reverse_got && after.zero(),
            observed(after),
        );
    } else {
        table.row("partial", "peer-shut-wr", false, u64::MAX);
    }

    // Local SHUT_RD: read returns 0; the peer's send fails EPIPE.
    if let Ok((left, right)) = pair(0) {
        let before = base;
        let shut = shutdown(left, SHUT_RD).is_ok();
        let eof = sys::read(left, &mut [0u8; 1]) == Ok(0);
        let epipe = sendmsg(right, b"x", &[], DUPLEX_MSG_NOSIGNAL) == Err(Errno::EPIPE);
        close(left);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "partial",
            "local-shut-rd",
            shut && eof && epipe && after.zero(),
            observed(after),
        );
    } else {
        table.row("partial", "local-shut-rd", false, u64::MAX);
    }
}

// ---------------------------------------------------------------------------------------------
// Group 5: abandoned and failed transfers.
// ---------------------------------------------------------------------------------------------

fn abandoned(table: &mut Table) {
    baseline(table, "abandoned");
    let base = stats().unwrap_or_default();

    // A bad data pointer fails after the control handles were validated; they must be dropped.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let failed = sendmsg_raw(
            left,
            0x1,
            1,
            control.as_mut_ptr() as usize,
            control_len,
            DUPLEX_MSG_NOSIGNAL,
        ) == Err(Errno::EFAULT);
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "abandoned",
            "send-bad-data",
            failed && after.zero(),
            observed(after),
        );
    } else {
        table.row("abandoned", "send-bad-data", false, u64::MAX);
    }

    // A bad receive data pointer fails before the datagram is dequeued; the message is retryable.
    if let (Ok((left, right)), Ok((read_end, write_end))) = (pair(0), pipe()) {
        let before = base;
        let mut control = [0u8; 64];
        let control_len = control_one(write_end, &mut control);
        let sent = sendmsg(left, b"d", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut message = Message {
            data: 0x1,
            data_len: 8,
            control: 0,
            control_len: 0,
            flags: 0,
            handle_count: 0,
        };
        let failed = recvmsg_at(right, &mut message) == Err(Errno::EFAULT);
        let mut data = [0u8; 8];
        let mut received_control = [0u8; 64];
        let retried = match recvmsg(right, &mut data, &mut received_control) {
            Ok((1, message)) if message.handle_count == 1 => {
                close(Handle(
                    first_record_count(&received_control, message.control_len as usize)
                        .map(|(_, raw)| raw)
                        .unwrap_or(u32::MAX),
                ));
                true
            }
            _ => false,
        };
        close(left);
        close(write_end);
        close(right);
        close(read_end);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "abandoned",
            "recv-bad-data-retry",
            sent && failed && retried && after.zero(),
            observed(after),
        );
    } else {
        table.row("abandoned", "recv-bad-data-retry", false, u64::MAX);
    }

    // A bad control pointer fails after install; `close_installed` must close every installed
    // handle. The payload is a duplex endpoint so a leak shows in the live-end counter.
    abandoned_bad_control(table);

    // A read-only message page fails the final write-back after install. This is the
    // `close_installed` path the ticket names.
    abandoned_bad_message(table);
}

fn abandoned_bad_control(table: &mut Table) {
    let before = stats().unwrap_or_default();
    if let (Ok((left, right)), Ok((carrier_left, carrier_right))) = (pair(0), pair(0)) {
        let mut control = [0u8; 64];
        let control_len = control_one(carrier_left, &mut control);
        let sent = sendmsg(left, b"c", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut message = Message {
            data: data.as_mut_ptr() as u64,
            data_len: data.len() as u64,
            control: 1,
            control_len: 64,
            flags: 0,
            handle_count: 0,
        };
        let failed = recvmsg_at(right, &mut message) == Err(Errno::EFAULT);
        close(left);
        close(carrier_left);
        close(carrier_right);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        let value = u64::from(sent) | (u64::from(failed) << 1) | (u64::from(after.zero()) << 2);
        table.row(
            "abandoned",
            "recv-bad-control-close-installed",
            sent && failed && after.zero(),
            value,
        );
    } else {
        table.row(
            "abandoned",
            "recv-bad-control-close-installed",
            false,
            u64::MAX,
        );
    }
}

fn abandoned_bad_message(table: &mut Table) {
    let before = stats().unwrap_or_default();
    if let (Ok((left, right)), Ok((carrier_left, carrier_right))) = (pair(0), pair(0)) {
        let mut control = [0u8; 64];
        let control_len = control_one(carrier_left, &mut control);
        let sent = sendmsg(left, b"m", &control[..control_len], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let page = sys::map_anonymous(4096).unwrap_or(0);
        let mut result = false;
        if page != 0 {
            let message = page as *mut Message;
            let mut data = [0u8; 8];
            let mut received_control = [0u8; 64];
            // SAFETY: the page is a live private mapping of at least 4096 bytes; the message is
            // written before the page is made read-only.
            unsafe {
                message.write(Message {
                    data: data.as_mut_ptr() as u64,
                    data_len: data.len() as u64,
                    control: received_control.as_mut_ptr() as u64,
                    control_len: received_control.len() as u64,
                    flags: 0,
                    handle_count: 0,
                });
            }
            let read_only = protect(page, 4096, prot::READ).is_ok();
            let failed = read_only && recvmsg_at(right, message) == Err(Errno::EFAULT);
            let _ = protect(page, 4096, prot::READ | prot::WRITE);
            let _ = sys::unmap(Handle::SELF_PROC, page, 4096);
            result = failed;
        }
        close(left);
        close(carrier_left);
        close(carrier_right);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        let value = u64::from(sent) | (u64::from(result) << 1) | (u64::from(after.zero()) << 2);
        table.row(
            "abandoned",
            "recv-bad-message-close-installed",
            sent && result && after.zero(),
            value,
        );
    } else {
        table.row(
            "abandoned",
            "recv-bad-message-close-installed",
            false,
            u64::MAX,
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Group 6: MSG_WAITALL.
// ---------------------------------------------------------------------------------------------

fn waitall(table: &mut Table) {
    baseline(table, "waitall");
    let before = stats().unwrap_or_default();
    if let Ok((left, right)) = pair(DUPLEX_DATAGRAM | O_NONBLOCK) {
        let sent = sendmsg(left, b"w", &[], DUPLEX_MSG_NOSIGNAL) == Ok(1);
        let mut data = [0u8; 8];
        let mut control = [0u8; 64];
        let mut message = Message {
            data: data.as_mut_ptr() as u64,
            data_len: 4,
            control: control.as_mut_ptr() as u64,
            control_len: control.len() as u64,
            flags: MSG_WAITALL,
            handle_count: 0,
        };
        let unsupported = recvmsg_at(right, &mut message) == Err(Errno::EOPNOTSUPP);
        close(left);
        close(right);
        let after = stats().unwrap_or_default().delta(before);
        table.row(
            "waitall",
            "msg-waitall-eopnotsupp",
            sent && unsupported && after.zero(),
            observed(after),
        );
    } else {
        table.row("waitall", "msg-waitall-eopnotsupp", false, u64::MAX);
    }
}

// ---------------------------------------------------------------------------------------------
// Group 7: 10k-op stateful fuzz.
// ---------------------------------------------------------------------------------------------

/// Small deterministic xorshift generator.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: u64) -> u64 {
        if limit == 0 { 0 } else { self.next() % limit }
    }
}

/// Fuzz direction: the endpoint a message travels *to*.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Left,
    Right,
}

/// One datagram queue entry in the fuzz model: whether it carries a handle.
struct Queued {
    has_handle: bool,
    /// The carrier's WRITE was still effective when this message was sent. A handle queued
    /// after the carrier revoke never held WRITE, so its use is EACCES; one queued before the
    /// revoke is granted-then-revoked at install time and its use is EPERM.
    write_at_send: bool,
}

struct FuzzPair {
    left: Handle,
    right: Handle,
    /// left -> right and right -> left queues.
    queues: [Vec<Queued>; 2],
}

impl FuzzPair {
    fn handle(&self, side: Side) -> Handle {
        match side {
            Side::Left => self.left,
            Side::Right => self.right,
        }
    }

    fn queue(&self, side: Side) -> usize {
        match side {
            Side::Left => 0,
            Side::Right => 1,
        }
    }

    fn peer(side: Side) -> Side {
        match side {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
        }
    }
}

struct Fuzz {
    pairs: [FuzzPair; 3],
    carrier_revoked: bool,
    pending: u64,
    hash: u64,
    steps: usize,
    deviations: usize,
}

impl Fuzz {
    fn stats_ok(&self) -> bool {
        let value = stats().unwrap_or_default();
        value.pairs == 3 && value.ends == 6 && value.pending == self.pending && value.controls == 0
    }

    fn record(&mut self, kind: u8, side: u8, ret: isize) {
        self.hash = fnv(self.hash, &[kind, side]);
        self.hash = fnv(self.hash, &ret.to_le_bytes());
        self.steps = self.steps.saturating_add(1);
    }

    fn break_invariant(&mut self, step: usize, kind: u8, side: u8) {
        let value = stats().unwrap_or_default();
        self.deviations = self.deviations.saturating_add(1);
        println!(
            "duplex-matrix: fuzz-break step={step} op={kind} side={side} expected-pending={} actual-pairs={} actual-ends={} actual-pending={} actual-controls={}",
            self.pending, value.pairs, value.ends, value.pending, value.controls
        );
    }

    fn send_data(&mut self, pair: usize, side: Side, step: usize) {
        let handle = self.pairs[pair].handle(side);
        let queue = self.pairs[pair].queue(side);
        let result = sendmsg(handle, b"d", &[], DUPLEX_MSG_NOSIGNAL);
        if result == Ok(1) {
            self.pairs[pair].queues[queue].push(Queued {
                has_handle: false,
                write_at_send: false,
            });
        }
        self.record(1, side as u8 + pair as u8 * 2, ret_of(result));
        if !self.stats_ok() {
            self.break_invariant(step, 1, side as u8);
        }
    }

    fn send_handle(&mut self, pair: usize, side: Side, step: usize) {
        let carrier = self.pairs[2].left;
        let mut control = [0u8; 64];
        let control_len = control_one(carrier, &mut control);
        let handle = self.pairs[pair].handle(side);
        let queue = self.pairs[pair].queue(side);
        let result = sendmsg(handle, b"h", &control[..control_len], DUPLEX_MSG_NOSIGNAL);
        if result == Ok(1) {
            self.pairs[pair].queues[queue].push(Queued {
                has_handle: true,
                write_at_send: !self.carrier_revoked,
            });
            self.pending = self.pending.saturating_add(1);
        }
        self.record(2, side as u8 + pair as u8 * 2, ret_of(result));
        if !self.stats_ok() {
            self.break_invariant(step, 2, side as u8);
        }
    }

    fn recv(&mut self, pair: usize, side: Side, small: bool, step: usize) {
        let handle = self.pairs[pair].handle(side);
        let incoming = FuzzPair::peer(side);
        let queue = self.pairs[pair].queue(incoming);
        let mut data = [0u8; 8];
        let mut control = [0u8; 64];
        let mut message = Message {
            data: data.as_mut_ptr() as u64,
            data_len: data.len() as u64,
            control: control.as_mut_ptr() as u64,
            control_len: if small { 8 } else { control.len() as u64 },
            flags: 0,
            handle_count: 0,
        };
        let result = recvmsg_at(handle, &mut message);
        if result == Ok(0) {
            // EOF or a read-shutdown leaves the queue untouched.
        } else if result.is_ok() {
            let Some(entry) = self.pairs[pair].queues[queue].first() else {
                self.break_invariant(step, 3, side as u8);
                return;
            };
            let had_handle = entry.has_handle;
            let write_at_send = entry.write_at_send;
            let _ = self.pairs[pair].queues[queue].remove(0);
            if had_handle {
                self.pending = self.pending.saturating_sub(1);
                if small {
                    // A control buffer too small for the record delivers the data, drops the
                    // whole SCM_RIGHTS record and reports MSG_CTRUNC (K521 donor rule).
                    if message.flags & MSG_CTRUNC == 0
                        || message.handle_count != 0
                        || message.control_len != 0
                    {
                        println!(
                            "duplex-matrix: fuzz-detail step={step} kind=truncated-control flags=0x{:x} handles={} control-len={}",
                            message.flags, message.handle_count, message.control_len
                        );
                        self.break_invariant(step, 4, side as u8);
                    }
                } else if message.handle_count != 1 || message.control_len < 20 {
                    println!(
                        "duplex-matrix: fuzz-detail step={step} kind=delivered-count handles={} control-len={}",
                        message.handle_count, message.control_len
                    );
                    self.break_invariant(step, 4, side as u8);
                } else {
                    let raw = get_u32(&control, CMSG_BYTES).unwrap_or(u32::MAX);
                    let delivered = Handle(raw);
                    let write = sendmsg(delivered, b"z", &[], DUPLEX_MSG_NOSIGNAL);
                    let rights_ok = if write_at_send {
                        if self.carrier_revoked {
                            // Granted at send, revoked by the ancestor before the use.
                            write == Err(Errno::EPERM)
                        } else {
                            matches!(write, Ok(_) | Err(Errno::EAGAIN) | Err(Errno::EPIPE))
                        }
                    } else {
                        // Queued after the revoke: this handle never held WRITE.
                        write == Err(Errno::EACCES)
                    };
                    if !rights_ok {
                        println!(
                            "duplex-matrix: fuzz-detail step={step} kind=delivered-rights revoked={} send-write={} write={write:?}",
                            self.carrier_revoked, write_at_send
                        );
                        self.break_invariant(step, 4, side as u8);
                    }
                    close(delivered);
                }
            } else if message.flags & MSG_CTRUNC != 0 {
                // No ancillary data was queued, so there is nothing to truncate.
                println!(
                    "duplex-matrix: fuzz-detail step={step} kind=spurious-ctrunc flags=0x{:x}",
                    message.flags
                );
                self.break_invariant(step, 5, side as u8);
            }
        } else if result == Err(Errno::EMSGSIZE) {
            // The donor delivers the data and sets MSG_CTRUNC; EMSGSIZE must not come back.
            self.break_invariant(step, 5, side as u8);
        } else if result != Err(Errno::EAGAIN) && result != Err(Errno::EPIPE) {
            self.break_invariant(step, 6, side as u8);
        }
        self.record(3, side as u8 + pair as u8 * 2, ret_of(result));
        if !self.stats_ok() {
            self.break_invariant(step, 3, side as u8);
        }
    }

    fn read(&mut self, pair: usize, side: Side, step: usize) {
        let handle = self.pairs[pair].handle(side);
        let incoming = FuzzPair::peer(side);
        let queue = self.pairs[pair].queue(incoming);
        let result = sys::read(handle, &mut [0u8; 8]);
        if result == Ok(1) {
            if let Some(entry) = self.pairs[pair].queues[queue].first() {
                if entry.has_handle {
                    self.pending = self.pending.saturating_sub(1);
                }
                let _ = self.pairs[pair].queues[queue].remove(0);
            }
        }
        self.record(7, side as u8 + pair as u8 * 2, ret_of(result));
        if !self.stats_ok() {
            self.break_invariant(step, 7, side as u8);
        }
    }

    fn shutdown(&mut self, pair: usize, side: Side, how: u8, step: usize) {
        let handle = self.pairs[pair].handle(side);
        let result = shutdown(handle, how).map(|_| 0);
        self.record(8, side as u8 + pair as u8 * 2, ret_of(result));
        if !self.stats_ok() {
            self.break_invariant(step, 8, side as u8);
        }
    }

    fn revoke_carrier(&mut self, step: usize) {
        let result = revoke(self.pairs[2].left, Rights::WRITE);
        if result.is_ok() {
            self.carrier_revoked = true;
        }
        self.record(9, 0, ret_of(result.map(|_| 0)));
        if !self.stats_ok() {
            self.break_invariant(step, 9, 0);
        }
    }

    fn drain(&mut self) {
        for pair in 0..3 {
            for side in [Side::Left, Side::Right] {
                let incoming = FuzzPair::peer(side);
                let queue = self.pairs[pair].queue(incoming);
                loop {
                    if self.pairs[pair].queues[queue].is_empty() {
                        break;
                    }
                    let handle = self.pairs[pair].handle(side);
                    let mut data = [0u8; 8];
                    let mut control = [0u8; 64];
                    let mut message = Message {
                        data: data.as_mut_ptr() as u64,
                        data_len: data.len() as u64,
                        control: control.as_mut_ptr() as u64,
                        control_len: control.len() as u64,
                        flags: 0,
                        handle_count: 0,
                    };
                    let result = recvmsg_at(handle, &mut message);
                    if result.is_err() || result == Ok(0) {
                        break;
                    }
                    let had_handle = self.pairs[pair].queues[queue]
                        .first()
                        .is_some_and(|entry| entry.has_handle);
                    let _ = self.pairs[pair].queues[queue].remove(0);
                    if had_handle {
                        self.pending = self.pending.saturating_sub(1);
                        if message.handle_count == 1 && message.control_len >= 20 {
                            close(Handle(get_u32(&control, CMSG_BYTES).unwrap_or(u32::MAX)));
                        }
                    }
                }
            }
        }
    }
}

fn ret_of(result: Result<usize, Errno>) -> isize {
    match result {
        Ok(value) => value as isize,
        Err(error) => error.to_ret(),
    }
}

fn fuzz(table: &mut Table) {
    baseline(table, "fuzz");
    let mut rng = Rng(SEED);
    let mut fuzz = Fuzz {
        pairs: [
            FuzzPair {
                left: Handle::NONE,
                right: Handle::NONE,
                queues: [Vec::new(), Vec::new()],
            },
            FuzzPair {
                left: Handle::NONE,
                right: Handle::NONE,
                queues: [Vec::new(), Vec::new()],
            },
            FuzzPair {
                left: Handle::NONE,
                right: Handle::NONE,
                queues: [Vec::new(), Vec::new()],
            },
        ],
        carrier_revoked: false,
        pending: 0,
        hash: 0xcbf2_9ce4_8422_2325,
        steps: 0,
        deviations: 0,
    };
    let flags = DUPLEX_DATAGRAM | O_NONBLOCK;
    let mut ready = true;
    for index in 0..3 {
        match pair(flags) {
            Ok((left, right)) => {
                fuzz.pairs[index].left = left;
                fuzz.pairs[index].right = right;
            }
            Err(_) => ready = false,
        }
    }
    if !ready {
        table.row("fuzz", "setup", false, u64::MAX);
        return;
    }

    let before = stats().unwrap_or_default();
    // Deterministic C17 seed: queue the carrier before the revoke, install it after. With the
    // granted-snapshot fix the use is EPERM; dropping the snapshot makes it EACCES.
    fuzz.send_handle(0, Side::Left, 0);
    fuzz.revoke_carrier(0);
    fuzz.recv(0, Side::Right, false, 0);
    let mut step = 0;
    while step < FUZZ_OPS {
        let pair = rng.below(3) as usize;
        let side = if rng.below(2) == 0 {
            Side::Left
        } else {
            Side::Right
        };
        let operation = rng.below(10);
        match operation {
            0 | 1 => fuzz.send_data(pair, side, step),
            2 | 3 => {
                // Send the carrier endpoint; the carrier is only a handle payload.
                fuzz.send_handle(pair, side, step);
            }
            4 | 5 => {
                let small = rng.below(3) == 0;
                fuzz.recv(pair, side, small, step);
            }
            6 => fuzz.read(pair, side, step),
            7 => fuzz.shutdown(pair, side, SHUT_WR, step),
            8 => fuzz.revoke_carrier(step),
            _ => {
                // Duplicate and close one endpoint: exercises generation/derived rights without
                // removing the last reference.
                let handle = fuzz.pairs[pair].handle(side);
                match sys::handle_dup(handle, Rights::ALL) {
                    Ok(dup) => {
                        let _ = sys::write(dup, b"");
                        close(dup);
                        fuzz.record(10, side as u8 + pair as u8 * 2, 0);
                        if !fuzz.stats_ok() {
                            fuzz.break_invariant(step, 10, side as u8);
                        }
                    }
                    Err(error) => {
                        fuzz.record(10, side as u8 + pair as u8 * 2, error.to_ret());
                    }
                }
            }
        }
        step = step.saturating_add(1);
    }

    fuzz.drain();
    for pair in &fuzz.pairs {
        close(pair.left);
        close(pair.right);
    }
    let after = stats().unwrap_or_default().delta(before);
    let ok = fuzz.deviations == 0
        && fuzz.steps >= FUZZ_OPS
        && after.zero()
        && stats().unwrap_or_default().zero();
    table.row("fuzz", "stateful-10k", ok, fuzz.hash ^ observed(after));
    println!(
        "duplex-matrix: fuzz summary steps={} deviations={} seed=0x{SEED:016x} trace-hash=0x{:016x}",
        fuzz.steps, fuzz.deviations, fuzz.hash
    );
}

// ---------------------------------------------------------------------------------------------
// Entry.
// ---------------------------------------------------------------------------------------------

fn main(_args: raven_rt::Args) -> i32 {
    println!("duplex-matrix: start seed=0x{SEED:016x} fuzz-ops={FUZZ_OPS} groups=7");
    let mut table = Table::new();
    fuzz(&mut table);
    ownership(&mut table);
    truncation(&mut table);
    partial(&mut table);
    abandoned(&mut table);
    waitall(&mut table);
    cycles(&mut table);
    table.summary();
    println!("duplex-matrix: alive");
    0
}
