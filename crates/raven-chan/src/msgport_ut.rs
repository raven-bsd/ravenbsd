// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the DragonFly msgport reply-port lifecycle.

use super::*;

/// Local copy of `raven-abi` `Errno::ECANCELED` for the pure model's payload only.
const CANCELED: i32 = 125;

#[test]
fn synchronous_target_completes_without_queueing() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(7, 41);
    let result = domsg(&mut port, &mut msg, |message| {
        message.target_port = 9;
        Some(0)
    })
    .unwrap();
    assert_eq!(result, SendResult::Completed(0));
    assert_eq!(msg.flags & (MSGF_SYNC | MSGF_DONE | MSGF_REPLY), 0xb);
    assert_eq!(port.queued(), 0);
    assert!(!port.woke());
}

#[test]
fn synchronous_message_wakes_reply_port_without_enqueueing() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(7, 42);
    assert_eq!(
        domsg(&mut port, &mut msg, |_| None).unwrap(),
        SendResult::Waiting
    );
    assert_eq!(msg.flags & MSGF_DONE, 0);
    assert_eq!(port.flags() & MSGPORTF_WAITING, MSGPORTF_WAITING);
    reply(&mut port, &mut msg, 17).unwrap();
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY | MSGF_SYNC), 0xb);
    assert_eq!(msg.code, 17);
    assert_eq!(port.queued(), 0);
    assert!(port.woke());
    assert_eq!(port.flags() & MSGPORTF_WAITING, 0);
}

#[test]
fn asynchronous_reply_is_queued_once() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(3, 77);
    msg.flags = 0;
    msg.target_port = 4;
    reply(&mut port, &mut msg, 0).unwrap();
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY | MSGF_QUEUED), 0x7);
    assert_eq!(port.queued(), 1);
    assert_eq!(port.pop_completion(), Some(77));
    assert_eq!(port.queued(), 0);
}

#[test]
fn late_reply_is_rejected() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(3, 78);
    msg.flags = 0;
    reply(&mut port, &mut msg, 0).unwrap();
    assert_eq!(
        reply(&mut port, &mut msg, 1),
        Err(MsgError::AlreadyComplete)
    );
    assert_eq!(port.queued(), 1);
}

#[test]
fn abort_requires_abortable_and_pending() {
    let mut msg = Message::new(1, 5);
    msg.flags = MSGF_ABORTABLE;
    let mut called = 0;
    abort(&mut msg, |_| called += 1).unwrap();
    assert_eq!(called, 1);
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY), 0);

    let mut msg = Message::new(1, 6);
    msg.flags = 0;
    assert_eq!(abort(&mut msg, |_| {}), Err(MsgError::NotAbortable));

    let mut msg = Message::new(1, 7);
    msg.flags = MSGF_DONE | MSGF_ABORTABLE;
    assert_eq!(abort(&mut msg, |_| {}), Err(MsgError::NotPending));
}

#[test]
fn domsg_rejects_a_message_without_a_reply_port_or_done_state() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(0, 1);
    assert_eq!(
        domsg(&mut port, &mut msg, |_| Some(0)),
        Err(MsgError::InvalidReplyPort)
    );

    let mut msg = Message::new(1, 2);
    msg.flags = 0;
    assert_eq!(
        domsg(&mut port, &mut msg, |_| Some(0)),
        Err(MsgError::NotDone)
    );
}

#[test]
fn asynchronous_send_queues_the_request_on_the_target() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 100);
    assert_eq!(
        sendmsg(&mut target, &mut port, &mut msg, |message| {
            message.target_port = 9;
            None
        })
        .unwrap(),
        SendResult::Queued
    );
    assert_eq!(target.queued(), 1);
    assert_eq!(msg.target_port, 9);
    assert_eq!(msg.flags & MSGF_QUEUED, MSGF_QUEUED);
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY | MSGF_SYNC), 0);
    assert_eq!(port.queued(), 0);
}

#[test]
fn receive_pulls_the_queued_request_and_clears_queued() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 101);
    sendmsg(&mut target, &mut port, &mut msg, |_| None).unwrap();
    assert!(recv(&mut target, &mut msg));
    assert_eq!(target.queued(), 0);
    assert_eq!(msg.flags & MSGF_QUEUED, 0);
    assert!(!recv(&mut target, &mut msg));
}

#[test]
fn asynchronous_reply_queues_and_wakes_a_waiter() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 102);
    sendmsg(&mut target, &mut port, &mut msg, |_| None).unwrap();
    assert!(recv(&mut target, &mut msg));
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Blocked);
    assert_eq!(port.flags() & MSGPORTF_WAITING, MSGPORTF_WAITING);
    reply(&mut port, &mut msg, 0).unwrap();
    assert!(port.take_wake());
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Ready(0));
    assert_eq!(port.queued(), 0);
    assert_eq!(msg.flags & MSGF_QUEUED, 0);
}

#[test]
fn waitmsg_allows_one_waiter_per_reply_port() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(7, 103);
    msg.flags = 0;
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Blocked);
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Busy);
}

#[test]
fn sync_in_caller_path_queues_a_reply_and_wait_returns_the_code() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 104);
    assert_eq!(
        sendmsg(&mut target, &mut port, &mut msg, |_| Some(3)).unwrap(),
        SendResult::Completed(3)
    );
    assert_eq!(target.queued(), 0);
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY | MSGF_QUEUED), 0x7);
    assert_eq!(port.queued(), 1);
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Ready(3));
    assert_eq!(port.queued(), 0);
}

#[test]
fn drop_cancels_a_queued_message_and_the_reply_is_late() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 105);
    msg.flags |= MSGF_DROPABLE;
    sendmsg(&mut target, &mut port, &mut msg, |_| None).unwrap();
    cancel(&mut target, &mut msg, CANCELED).unwrap();
    assert_eq!(target.queued(), 0);
    assert_eq!(
        msg.flags & (MSGF_DONE | MSGF_REPLY | MSGF_QUEUED),
        MSGF_DONE
    );
    assert_eq!(msg.code, CANCELED);
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::NotWaitable);
    assert_eq!(
        reply(&mut port, &mut msg, 0),
        Err(MsgError::AlreadyComplete)
    );
}

#[test]
fn abort_callback_can_cancel_the_queued_message() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 106);
    msg.flags |= MSGF_ABORTABLE | MSGF_DROPABLE;
    sendmsg(&mut target, &mut port, &mut msg, |_| None).unwrap();
    abort(&mut msg, |message| {
        cancel(&mut target, message, CANCELED).unwrap();
    })
    .unwrap();
    assert_eq!(target.queued(), 0);
    assert_eq!(msg.code, CANCELED);
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::NotWaitable);
    assert_eq!(
        reply(&mut port, &mut msg, 0),
        Err(MsgError::AlreadyComplete)
    );
}

#[test]
fn abort_callback_may_leave_the_message_pending() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 107);
    msg.flags |= MSGF_ABORTABLE;
    sendmsg(&mut target, &mut port, &mut msg, |_| None).unwrap();
    abort(&mut msg, |_| {}).unwrap();
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY), 0);
    assert!(recv(&mut target, &mut msg));
    reply(&mut port, &mut msg, 0).unwrap();
    assert_eq!(waitmsg(&mut port, &mut msg), WaitOutcome::Ready(0));
}

#[test]
fn reply_on_an_in_transit_message_is_rejected() {
    let mut port = ReplyPort::new();
    let mut msg = Message::new(7, 108);
    msg.flags = MSGF_INTRANSIT;
    assert_eq!(reply(&mut port, &mut msg, 0), Err(MsgError::InTransit));
    msg.flags = 0;
    reply(&mut port, &mut msg, 0).unwrap();
    assert_eq!(port.queued(), 1);
}

#[test]
fn drop_rejects_a_message_that_is_not_queued_or_not_dropable() {
    let mut target = TargetPort::new();
    let mut msg = Message::new(7, 109);
    assert_eq!(dropmsg(&mut target, &mut msg), Err(MsgError::NotDropable));

    let mut msg = Message::new(7, 110);
    msg.flags = MSGF_DROPABLE;
    assert_eq!(dropmsg(&mut target, &mut msg), Err(MsgError::NotQueued));
}

#[test]
fn sendmsg_rejects_a_message_without_a_reply_port_or_done_state() {
    let mut port = ReplyPort::new();
    let mut target = TargetPort::new();
    let mut msg = Message::new(0, 111);
    assert_eq!(
        sendmsg(&mut target, &mut port, &mut msg, |_| None),
        Err(MsgError::InvalidReplyPort)
    );

    let mut msg = Message::new(7, 112);
    msg.flags = 0;
    assert_eq!(
        sendmsg(&mut target, &mut port, &mut msg, |_| None),
        Err(MsgError::NotDone)
    );
}
