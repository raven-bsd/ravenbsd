// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2003,2004 The DragonFly Project.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from DragonFlyBSD sys/kern/lwkt_msgport.c and sys/sys/msgport2.h at
// 94448f622a06:
//   lwkt_sendmsg()           lwkt_msgport.c:141
//   lwkt_domsg()             lwkt_msgport.c:184
//   lwkt_abortmsg()          lwkt_msgport.c:231
//   _lwkt_enqueue_reply()    lwkt_msgport.c:522
//   lwkt_thread_replyport()  lwkt_msgport.c:591
//   lwkt_thread_dropmsg()    lwkt_msgport.c:652
//   lwkt_thread_waitmsg()    lwkt_msgport.c:770
//   lwkt_replymsg()          msgport2.h:50
//   lwkt_dropmsg()           msgport2.h:98
// The DragonFly header is the three-clause BSD licence. The port keeps the
// synchronous and asynchronous completion rules, the single-waiter reply-port handshake,
// the dropable-message drop path and the abort callback contract; it does not schedule,
// IPI, or own kernel objects.

//! A bounded message-port completion model from DragonFlyBSD.
//!
//! This is the pure policy half of the msgport: one [`Message`] carries its reply port, target
//! port, flags and txid; [`sendmsg`] requests asynchronous completion, [`domsg`] synchronous
//! completion; [`reply`] returns exactly once; [`dropmsg`] cancels a queued dropable request;
//! [`abort`] invokes the message's abort callback; [`waitmsg`] is the single-waiter reply-port
//! handshake. It does not schedule or own kernel objects; the kernel integration remains outside
//! this crate.

use alloc::collections::VecDeque;

/// The message has completed.
pub const MSGF_DONE: u32 = 0x0001;
/// The message has been replied to.
pub const MSGF_REPLY: u32 = 0x0002;
/// The message is queued on a port.
pub const MSGF_QUEUED: u32 = 0x0004;
/// The caller waits for synchronous completion.
pub const MSGF_SYNC: u32 = 0x0008;
/// The message state is indeterminate (for example, being passed through an IPI).
pub const MSGF_INTRANSIT: u32 = 0x0010;
/// A synchronous message is being waited upon.
pub const MSGF_WAITING: u32 = 0x0020;
/// The message supports the drop path.
pub const MSGF_DROPABLE: u32 = 0x0040;
/// The message may be aborted while pending.
pub const MSGF_ABORTABLE: u32 = 0x0080;
/// The message is a priority message.
pub const MSGF_PRIORITY: u32 = 0x0100;
/// The message wants a receipt after the target put completes.
pub const MSGF_RECEIPT: u32 = 0x0200;
/// The reply port has a waiter.
pub const MSGPORTF_WAITING: u32 = 0x0001;

/// Why a send, reply, drop or abort was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MsgError {
    /// The message has no reply port.
    InvalidReplyPort,
    /// The message was not in the completed state expected by a send.
    NotDone,
    /// The message already completed; a second reply is a late reply.
    AlreadyComplete,
    /// The message state is indeterminate; the in-transit owner must complete it.
    InTransit,
    /// The message is not marked abortable.
    NotAbortable,
    /// The message is not pending (already done or replied).
    NotPending,
    /// The message is not marked dropable.
    NotDropable,
    /// The message is not queued on the target port.
    NotQueued,
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
    /// A fresh message in the completed state expected by [`sendmsg`] and [`domsg`].
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

    /// A fresh message whose abort callback is valid (`lwkt_initmsg_abortable`).
    #[must_use]
    pub const fn new_abortable(reply_port: u32, txid: u64) -> Self {
        Self {
            reply_port,
            target_port: 0,
            code: 0,
            flags: MSGF_DONE | MSGF_ABORTABLE,
            txid,
        }
    }
}

/// The result of a send.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SendResult {
    /// The target completed synchronously with this code; the reply is on the reply port.
    Completed(i32),
    /// The target accepted the message asynchronously; it is queued on the target port.
    Queued,
    /// A synchronous send is waiting for an asynchronous reply.
    Waiting,
}

/// A target port: requests queued for the port-owning thread to receive.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TargetPort {
    queue: VecDeque<Message>,
}

impl TargetPort {
    /// A fresh empty target port.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Requests waiting for `recv`.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.queue.len()
    }

    fn remove(&mut self, txid: u64) -> bool {
        if let Some(index) = self.queue.iter().position(|queued| queued.txid == txid) {
            self.queue.remove(index);
            true
        } else {
            false
        }
    }
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

    /// True when the port was woken by a reply.
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

    fn remove_completion(&mut self, txid: u64) -> bool {
        if let Some(index) = self.queue.iter().position(|queued| *queued == txid) {
            self.queue.remove(index);
            true
        } else {
            false
        }
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

/// `lwkt_sendmsg`: request asynchronous completion.
///
/// `begin` is the target's `mp_putport`: `Some(code)` means the target completed in the caller's
/// context and the reply is queued on the reply port; `None` means the target accepted the
/// message asynchronously and it is queued on the target port for [`recv`].
pub fn sendmsg(
    target: &mut TargetPort,
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
    msg.flags &= !(MSGF_REPLY | MSGF_SYNC | MSGF_DONE);
    match begin(msg) {
        Some(code) => {
            // The target opted to execute the message synchronously; queue its reply.
            reply(port, msg, code)?;
            Ok(SendResult::Completed(code))
        }
        None => {
            msg.flags |= MSGF_QUEUED;
            target.queue.push_back(*msg);
            Ok(SendResult::Queued)
        }
    }
}

/// `lwkt_domsg`: request synchronous completion.
///
/// The target `begin` returns `Some(code)` when it completed in the caller's context, or `None`
/// when it will complete asynchronously. The asynchronous arm is returned as
/// [`SendResult::Waiting`] and the reply port records the waiter; [`reply`] completes it without
/// enqueuing the message.
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

/// `lwkt_getport`: receive the next request queued on `target`.
///
/// The queued message is matched by txid and its `MSGF_QUEUED` bit is cleared on `msg`, the
/// caller's authoritative copy.
pub fn recv(target: &mut TargetPort, msg: &mut Message) -> bool {
    if target.remove(msg.txid) {
        msg.flags &= !MSGF_QUEUED;
        true
    } else {
        false
    }
}

/// `lwkt_thread_replyport`: complete one message exactly once.
///
/// A message with `MSGF_SYNC` is completed in place and wakes the reply port without a queue
/// entry. Any other message is marked `MSGF_QUEUED | MSGF_REPLY | MSGF_DONE` and returned to the
/// reply port's completion queue. A message that is already done/replied, or is in transit, is
/// rejected: the donor's `lwkt_thread_replyport` asserts the same exact-once precondition.
pub fn reply(port: &mut ReplyPort, msg: &mut Message, code: i32) -> Result<(), MsgError> {
    if msg.reply_port == 0 {
        return Err(MsgError::InvalidReplyPort);
    }
    if msg.flags & MSGF_INTRANSIT != 0 {
        return Err(MsgError::InTransit);
    }
    if msg.flags & (MSGF_DONE | MSGF_REPLY) != 0 {
        return Err(MsgError::AlreadyComplete);
    }
    msg.code = code;
    if msg.flags & MSGF_SYNC != 0 {
        msg.flags |= MSGF_DONE | MSGF_REPLY;
        if port.flags & MSGPORTF_WAITING != 0 {
            port.wake();
        }
    } else {
        msg.flags |= MSGF_DONE | MSGF_REPLY | MSGF_QUEUED;
        port.queue.push_back(msg.txid);
        if port.flags & MSGPORTF_WAITING != 0 {
            port.wake();
        }
    }
    Ok(())
}

/// `lwkt_dropmsg`: drop a queued dropable message (`lwkt_thread_dropmsg`).
///
/// The message must be `MSGF_DROPABLE` and queued on the target port. The donor removes it from
/// the target queue and sets `MSGF_DONE` without setting `MSGF_REPLY`; a later reply is late.
pub fn dropmsg(target: &mut TargetPort, msg: &mut Message) -> Result<(), MsgError> {
    if msg.flags & MSGF_DROPABLE == 0 {
        return Err(MsgError::NotDropable);
    }
    if msg.flags & (MSGF_REPLY | MSGF_QUEUED) != MSGF_QUEUED {
        return Err(MsgError::NotQueued);
    }
    if !target.remove(msg.txid) {
        return Err(MsgError::NotQueued);
    }
    msg.flags &= !MSGF_QUEUED;
    msg.flags |= MSGF_DONE;
    Ok(())
}

/// Cancel a queued dropable message with `code` (the `lwkt_dropmsg` path).
///
/// The cancel code is the caller's completion payload; the kernel passes the native cancel errno.
pub fn cancel(target: &mut TargetPort, msg: &mut Message, code: i32) -> Result<(), MsgError> {
    msg.code = code;
    dropmsg(target, msg)
}

/// `lwkt_abortmsg`: invoke the abort callback for a pending, abortable message.
///
/// The donor does not set completion flags here: the callback typically sends an abort request to
/// the target, which may drop or complete the original message. The callback is not required to
/// abort; if it leaves the message pending, a later reply is valid.
pub fn abort(msg: &mut Message, abortfn: impl FnOnce(&mut Message)) -> Result<(), MsgError> {
    if msg.flags & MSGF_ABORTABLE == 0 {
        return Err(MsgError::NotAbortable);
    }
    if msg.flags & (MSGF_DONE | MSGF_REPLY) != 0 {
        return Err(MsgError::NotPending);
    }
    abortfn(msg);
    Ok(())
}

/// What a `waitmsg` call found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitOutcome {
    /// The message completed; `Message::code` holds the completion payload.
    Ready(i32),
    /// The reply port now has a waiter; the caller blocks until a reply wakes it.
    Blocked,
    /// Another thread already waits on this reply port (the donor thread port allows one).
    Busy,
    /// A dropable message must not be waited on (the donor asserts this).
    NotWaitable,
}

/// `lwkt_thread_waitmsg`: wait for one message's reply.
///
/// The first call on a pending message sets `MSGPORTF_WAITING` and returns [`WaitOutcome::Blocked`];
/// a second waiter is [`WaitOutcome::Busy`]. After the reply wakes the port, the next call pulls
/// the queued completion and returns [`WaitOutcome::Ready`] with the message code. A dropable
/// message must not be waited on (the donor asserts this), so it returns
/// [`WaitOutcome::NotWaitable`].
pub fn waitmsg(port: &mut ReplyPort, msg: &mut Message) -> WaitOutcome {
    if msg.flags & MSGF_DROPABLE != 0 {
        return WaitOutcome::NotWaitable;
    }
    if msg.flags & MSGF_DONE != 0 {
        if msg.flags & MSGF_QUEUED != 0 {
            port.remove_completion(msg.txid);
            msg.flags &= !MSGF_QUEUED;
        }
        port.flags &= !MSGPORTF_WAITING;
        port.woke = false;
        return WaitOutcome::Ready(msg.code);
    }
    if port.flags & MSGPORTF_WAITING != 0 {
        return WaitOutcome::Busy;
    }
    port.wait();
    WaitOutcome::Blocked
}

#[cfg(test)]
#[path = "msgport_ut.rs"]
mod tests;
