// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2003,2004 The DragonFly Project.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from DragonFlyBSD sys/kern/lwkt_msgport.c at 94448f622a06:
// lwkt_domsg() (line 184) and the synchronous arm of lwkt_thread_replyport() (line 591).
// The DragonFly header is the three-clause BSD licence. The port keeps the synchronous
// completion rule: a reply to an MSGF_SYNC message sets MSGF_DONE|MSGF_REPLY and wakes the
// waiting reply port directly, while an asynchronous reply is queued on the reply port.

//! A bounded message-port completion model from DragonFlyBSD.
//!
//! This is the pure policy half of the msgport: one [`Message`] carries its reply port, flags
//! and txid; [`domsg`] requests synchronous completion; [`reply`] returns exactly once. It does
//! not schedule or own kernel objects; the kernel integration remains outside this crate.

use alloc::collections::VecDeque;

/// The message has completed.
pub const MSGF_DONE: u32 = 0x0001;
/// The message has been replied to.
pub const MSGF_REPLY: u32 = 0x0002;
/// The message is queued on a port.
pub const MSGF_QUEUED: u32 = 0x0004;
/// The caller waits for synchronous completion.
pub const MSGF_SYNC: u32 = 0x0008;
/// The message may be aborted while pending.
pub const MSGF_ABORTABLE: u32 = 0x0080;
/// The reply port has a waiter.
pub const MSGPORTF_WAITING: u32 = 0x0001;

/// Why a reply or abort was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgError {
    /// The message has no reply port.
    InvalidReplyPort,
    /// The message was not in the completed state expected by `domsg`.
    NotDone,
    /// The message already completed; a second reply is a late reply.
    AlreadyComplete,
    /// The message is not marked abortable.
    NotAbortable,
    /// The message is not pending (already done or replied).
    NotPending,
}

/// One message in the msgport model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    /// The original reply port.
    pub reply_port: u32,
    /// The target port, if a forwarding target is recorded.
    pub target_port: u32,
    /// Completion payload or error code.
    pub code: i32,
    /// `MSGF_*` flags.
    pub flags: u32,
    /// The caller's transaction id.
    pub txid: u64,
}

impl Message {
    /// A fresh message in the completed state expected by [`domsg`].
    #[must_use]
    pub const fn new(reply_port: u32, txid: u64) -> Self {
        Self {
            reply_port,
            target_port: 0,
            code: 0,
            flags: MSGF_DONE,
            txid,
        }
    }
}

/// The result of a synchronous send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendResult {
    /// The target completed synchronously with this code.
    Completed(i32),
    /// The target will complete asynchronously; the reply port has a waiter and the message is
    /// not queued to the reply port yet.
    Waiting,
}

/// A reply port: the completed-message queue plus the waiting/wake state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReplyPort {
    queue: VecDeque<u64>,
    flags: u32,
    woke: bool,
}

impl ReplyPort {
    /// A fresh reply port.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
            flags: 0,
            woke: false,
        }
    }

    /// Port flags (`MSGPORTF_*`).
    #[must_use]
    pub const fn flags(&self) -> u32 {
        self.flags
    }

    /// True when the port was woken by a synchronous reply.
    #[must_use]
    pub const fn woke(&self) -> bool {
        self.woke
    }

    /// Completed asynchronous messages waiting to be reaped.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    /// Pop one queued asynchronous completion.
    pub fn pop_completion(&mut self) -> Option<u64> {
        self.queue.pop_front()
    }

    /// Clear the one-shot wake latch.
    pub fn take_wake(&mut self) -> bool {
        core::mem::replace(&mut self.woke, false)
    }

    fn wait(&mut self) {
        self.flags |= MSGPORTF_WAITING;
        self.woke = false;
    }

    fn wake(&mut self) {
        self.flags &= !MSGPORTF_WAITING;
        self.woke = true;
    }
}

/// `lwkt_domsg`: request synchronous completion.
///
/// The target `begin` returns `Some(code)` when it completed in the caller's context, or `None`
/// when it will complete asynchronously. The asynchronous arm is returned as [`SendResult::Waiting`]
/// and the reply port records the waiter; [`reply`] completes it without enqueuing the message.
pub fn domsg(
    port: &mut ReplyPort,
    msg: &mut Message,
    begin: impl FnOnce(&mut Message) -> Option<i32>,
) -> Result<SendResult, MsgError> {
    if msg.reply_port == 0 {
        return Err(MsgError::InvalidReplyPort);
    }
    if msg.flags & (MSGF_DONE | MSGF_QUEUED) != MSGF_DONE {
        return Err(MsgError::NotDone);
    }
    msg.flags &= !(MSGF_REPLY | MSGF_DONE);
    msg.flags |= MSGF_SYNC;
    match begin(msg) {
        Some(code) => {
            msg.flags |= MSGF_DONE | MSGF_REPLY;
            msg.code = code;
            Ok(SendResult::Completed(code))
        }
        None => {
            port.wait();
            Ok(SendResult::Waiting)
        }
    }
}

/// `lwkt_thread_replyport`: complete one message exactly once.
///
/// A message with `MSGF_SYNC` is completed in place and wakes the reply port without a queue
/// entry. Any other message is marked `MSGF_QUEUED | MSGF_REPLY | MSGF_DONE` and returned to the
/// reply port's completion queue.
pub fn reply(port: &mut ReplyPort, msg: &mut Message, code: i32) -> Result<(), MsgError> {
    if msg.reply_port == 0 {
        return Err(MsgError::InvalidReplyPort);
    }
    if msg.flags & (MSGF_DONE | MSGF_REPLY) != 0 {
        return Err(MsgError::AlreadyComplete);
    }
    msg.code = code;
    if msg.flags & MSGF_SYNC != 0 {
        msg.flags |= MSGF_DONE | MSGF_REPLY;
        port.wake();
    } else {
        msg.flags |= MSGF_DONE | MSGF_REPLY | MSGF_QUEUED;
        port.queue.push_back(msg.txid);
        port.wake();
    }
    Ok(())
}

/// `lwkt_abortmsg`: abort only a pending, abortable message.
pub fn abort(msg: &mut Message) -> Result<(), MsgError> {
    if msg.flags & MSGF_ABORTABLE == 0 {
        return Err(MsgError::NotAbortable);
    }
    if msg.flags & (MSGF_DONE | MSGF_REPLY) != 0 {
        return Err(MsgError::NotPending);
    }
    msg.flags |= MSGF_DONE | MSGF_REPLY;
    Ok(())
}

#[cfg(test)]
#[path = "msgport_ut.rs"]
mod tests;
