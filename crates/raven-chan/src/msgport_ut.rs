// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the DragonFly msgport synchronous-reply port.

use super::*;

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
    abort(&mut msg).unwrap();
    assert_eq!(msg.flags & (MSGF_DONE | MSGF_REPLY), 0x3);

    let mut msg = Message::new(1, 6);
    msg.flags = 0;
    assert_eq!(abort(&mut msg), Err(MsgError::NotAbortable));

    let mut msg = Message::new(1, 7);
    msg.flags = MSGF_DONE | MSGF_ABORTABLE;
    assert_eq!(abort(&mut msg), Err(MsgError::NotPending));
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
