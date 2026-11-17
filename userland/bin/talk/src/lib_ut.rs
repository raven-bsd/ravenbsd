// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Protocol, parser and session-state tests for `talk`.

use super::*;

fn alice() -> Address {
    Address {
        user: "alice".to_string(),
        host: Some("example".to_string()),
    }
}

#[test]
fn parses_a_caller_address_and_terminal() {
    let options = parse_args(["alice@example", "tty1"].map(String::from)).unwrap();
    assert!(!options.listen);
    assert_eq!(options.address, Some(alice()));
    assert_eq!(options.terminal.as_deref(), Some("tty1"));
}

#[test]
fn parses_a_listener_with_and_without_a_terminal() {
    let options = parse_args(["-l"].map(String::from)).unwrap();
    assert!(options.listen);
    assert_eq!(options.address, None);
    assert_eq!(options.terminal, None);

    let options = parse_args(["--listen", "tty:console"].map(String::from)).unwrap();
    assert!(options.listen);
    assert_eq!(options.terminal.as_deref(), Some("tty:console"));
}

#[test]
fn address_text_keeps_the_optional_host() {
    assert_eq!(alice().text(), "alice@example");
    assert_eq!(
        Address {
            user: "bob".to_string(),
            host: None
        }
        .text(),
        "bob"
    );
}

#[test]
fn rejects_bad_addresses_and_terminals() {
    assert_eq!(
        parse_args(std::iter::empty::<String>()),
        Err(UsageError::MissingAddress)
    );
    assert_eq!(
        parse_args(["-x"].map(String::from)),
        Err(UsageError::UnknownOption('x'))
    );
    assert_eq!(
        parse_args(["a", "b", "c"].map(String::from)),
        Err(UsageError::ExtraOperand)
    );
    assert_eq!(
        parse_args(["-l", "a", "b"].map(String::from)),
        Err(UsageError::ExtraOperand)
    );
    assert_eq!(
        parse_args([""].map(String::from)),
        Err(UsageError::BadAddress)
    );
    assert_eq!(
        parse_args(["a@@b"].map(String::from)),
        Err(UsageError::BadAddress)
    );
    assert_eq!(
        parse_args(["a b"].map(String::from)),
        Err(UsageError::BadAddress)
    );
    assert_eq!(
        parse_args(["alice", "tty 1"].map(String::from)),
        Err(UsageError::BadTerminal)
    );
}

#[test]
fn frames_round_trip_on_the_wire() {
    let frames = [
        Frame::Invite {
            caller: alice(),
            terminal: None,
        },
        Frame::Invite {
            caller: alice(),
            terminal: Some("tty1".to_string()),
        },
        Frame::Accept,
        Frame::Decline {
            reason: String::new(),
        },
        Frame::Decline {
            reason: "busy: 100%".to_string(),
        },
        Frame::Data {
            text: "hello world".to_string(),
        },
        Frame::Data {
            text: "line\nnext\tend%".to_string(),
        },
        Frame::Data {
            text: "h\u{e9}llo".to_string(),
        },
        Frame::Alert,
        Frame::End,
        Frame::Bye,
    ];
    for frame in frames {
        let line = encode_frame(&frame).unwrap();
        assert!(!line.contains('\n'), "{line}");
        assert_eq!(decode_frame(&line), Ok(frame));
    }
}

#[test]
fn malformed_frames_are_rejected() {
    assert_eq!(decode_frame(""), Err(ProtocolError::UnknownFrame));
    assert_eq!(
        decode_frame("NOTALK INVITE alice"),
        Err(ProtocolError::BadPrefix)
    );
    assert_eq!(
        decode_frame("RVTALK/1 NOPE"),
        Err(ProtocolError::UnknownFrame)
    );
    assert_eq!(
        decode_frame("RVTALK/1 ACCEPT extra"),
        Err(ProtocolError::UnknownFrame)
    );
    assert_eq!(
        decode_frame("RVTALK/1 INVITE a@@b"),
        Err(ProtocolError::BadAddress)
    );
    assert_eq!(
        decode_frame("RVTALK/1 DATA %zz"),
        Err(ProtocolError::BadText)
    );
    assert_eq!(decode_frame("RVTALK/1 ACCEPT"), Ok(Frame::Accept));
    assert_eq!(
        decode_frame(&("RVTALK/1 DATA ".to_string() + &"x".repeat(MAX_FRAME))),
        Err(ProtocolError::TooLong)
    );
}

#[test]
fn invitation_text_matches_the_posix_shape() {
    let text = invitation_text(&alice(), Some("tty1"));
    assert!(text.contains("Message from alice@example on tty1"));
    assert!(text.contains("talk: connection requested by alice@example"));
    assert!(text.contains("talk: respond with: talk alice@example"));
    assert!(text.ends_with("\r\n"));
}

#[test]
fn split_screen_labels_both_panes() {
    let screen = split_screen("me", "peer");
    assert!(screen.contains("== me =="));
    assert!(screen.contains("== peer =="));
    assert!(screen.contains(&"-".repeat(SCREEN_WIDTH)));
    assert!(refresh_screen().starts_with("\x1b["));
}

#[test]
fn remote_text_is_sanitized() {
    assert_eq!(sanitize_display("ok\r\n\x07no\t"), "okno");
    assert_eq!(sanitize_display("h\u{e9}llo"), "hllo");
}

#[test]
fn full_handshake_forwards_both_ways_and_closes_explicitly() {
    let mut caller = Session::caller(alice(), Some("tty1".to_string()));
    let mut listener = Session::listener();

    let invite = match caller.start() {
        Event::Send(Frame::Invite { caller, terminal }) => (caller, terminal),
        other => panic!("unexpected caller start: {other:?}"),
    };
    assert_eq!(caller.state(), State::Inviting);
    let shown = listener.receive(Frame::Invite {
        caller: invite.0,
        terminal: invite.1,
    });
    assert!(matches!(shown, Event::ShowInvitation(ref text) if text.contains("alice@example")));
    assert_eq!(listener.state(), State::Invited);

    assert_eq!(listener.accept(), Event::Send(Frame::Accept));
    assert_eq!(listener.state(), State::Active);
    assert_eq!(caller.receive(Frame::Accept), Event::Start);
    assert_eq!(caller.state(), State::Active);

    assert_eq!(
        caller.local_input("hello"),
        Event::Send(Frame::Data {
            text: "hello".to_string()
        })
    );
    assert_eq!(
        listener.receive(Frame::Data {
            text: "hello".to_string()
        }),
        Event::Show("hello".to_string())
    );
    assert_eq!(
        listener.local_char('y'),
        Event::Send(Frame::Data {
            text: "y".to_string()
        })
    );
    assert_eq!(
        caller.receive(Frame::Data {
            text: "y".to_string()
        }),
        Event::Show("y".to_string())
    );
    assert_eq!(caller.local_char('\x0c'), Event::Refresh);
    assert_eq!(caller.local_char('\x07'), Event::Send(Frame::Alert));
    assert_eq!(listener.receive(Frame::Alert), Event::Alert);
    assert_eq!(caller.local_char('\x01'), Event::Ignore);

    assert_eq!(caller.local_char('\x04'), Event::Send(Frame::End));
    assert_eq!(caller.state(), State::LocalClosed);
    assert_eq!(listener.receive(Frame::End), Event::RemoteEnded);
    assert_eq!(listener.state(), State::RemoteClosed);
    assert_eq!(listener.local_input("ignored"), Event::Ignore);
    assert_eq!(listener.local_char('x'), Event::Ignore);
    assert_eq!(listener.local_char('\x03'), Event::Send(Frame::Bye));
    assert_eq!(listener.state(), State::Closed);
    assert_eq!(caller.receive(Frame::Bye), Event::Closed);
    assert_eq!(caller.state(), State::Closed);
    assert_eq!(caller.exit_status(), 0);
    assert_eq!(listener.exit_status(), 0);
}

#[test]
fn decline_closes_the_caller_with_status_one() {
    let mut caller = Session::caller(alice(), None);
    let mut listener = Session::listener();
    let Event::Send(Frame::Invite {
        caller: address,
        terminal,
    }) = caller.start()
    else {
        panic!("caller did not send an invitation");
    };
    listener.receive(Frame::Invite {
        caller: address,
        terminal,
    });
    assert_eq!(
        listener.decline("busy"),
        Event::Send(Frame::Decline {
            reason: "busy".to_string()
        })
    );
    assert_eq!(listener.state(), State::Closed);
    assert_eq!(listener.exit_status(), 1);
    assert_eq!(
        caller.receive(Frame::Decline {
            reason: "busy".to_string()
        }),
        Event::Declined("busy".to_string())
    );
    assert_eq!(caller.state(), State::Closed);
    assert_eq!(caller.exit_status(), 1);
}

#[test]
fn remote_end_leaves_only_the_escape_exit() {
    let mut caller = Session::caller(alice(), None);
    caller.receive(Frame::Accept);
    assert_eq!(caller.receive(Frame::End), Event::RemoteEnded);
    assert_eq!(caller.state(), State::RemoteClosed);
    assert_eq!(caller.local_input("ignored"), Event::Ignore);
    assert_eq!(caller.local_char('\x04'), Event::Ignore);
    assert_eq!(caller.local_char('\x03'), Event::Send(Frame::Bye));
    assert_eq!(caller.state(), State::Closed);
}

#[test]
fn out_of_order_events_are_ignored() {
    let mut caller = Session::caller(alice(), None);
    let mut listener = Session::listener();
    assert_eq!(listener.start(), Event::Ignore);
    assert_eq!(caller.accept(), Event::Ignore);
    assert_eq!(
        caller.receive(Frame::Invite {
            caller: alice(),
            terminal: None
        }),
        Event::Ignore
    );
    assert_eq!(listener.receive(Frame::Accept), Event::Ignore);
    assert_eq!(listener.decline("nope"), Event::Ignore);
    assert_eq!(caller.local_char('x'), Event::Ignore);
    assert_eq!(caller.escape(), Event::Send(Frame::Bye));
    assert_eq!(caller.escape(), Event::Ignore);
}

#[test]
fn loopback_transcript_covers_both_directions() {
    let lines = loopback_transcript().expect("local loopback");
    assert!(lines.iter().any(|line| line == "caller> hello"));
    assert!(lines.iter().any(|line| line == "peer< hello"));
    assert!(lines.iter().any(|line| line == "peer> hi"));
    assert!(lines.iter().any(|line| line == "caller< hi"));
    assert!(lines.iter().any(|line| line == "caller: eof"));
    assert!(lines.iter().any(|line| line == "peer: remote ended"));
    assert!(
        lines
            .last()
            .is_some_and(|line| line.contains("closed (caller=0 peer=0)"))
    );
}

#[test]
fn listener_escape_before_invitation_closes_cleanly() {
    let mut listener = Session::listener();
    assert_eq!(listener.escape(), Event::Send(Frame::Bye));
    assert_eq!(listener.state(), State::Closed);
    assert_eq!(listener.exit_status(), 0);
}
