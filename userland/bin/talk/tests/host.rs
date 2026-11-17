// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host checks for `talk`: CLI, local loopback and a two-peer pty transcript.

use std::process::Command;

use raven_abi::errno::Errno;
use raven_proto_tty::{Mode, Settings};
use raven_ptyd::{Pairs, Pty, SignalSink};
use raven_ttyd::Peers;
use talk::{Address, Event, Frame, Session, decode_frame, encode_frame, opposite_endpoint};

#[derive(Default)]
struct Sink;

impl SignalSink for Sink {
    fn signal(&mut self, _pgrp: i32, _signo: u32) -> Result<(), Errno> {
        Ok(())
    }
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_talk")
}

fn line(frame: &Frame) -> Vec<u8> {
    let mut line = encode_frame(frame).expect("encode frame");
    line.push('\n');
    line.into_bytes()
}

fn read(pair: &mut Pty<Sink>, master: bool) -> Frame {
    let mut buffer = [0u8; 256];
    let count = if master {
        pair.read_master(&mut buffer).expect("master read")
    } else {
        pair.read_slave(&mut buffer).expect("slave read")
    };
    let text = std::str::from_utf8(&buffer[..count]).expect("utf8 frame");
    decode_frame(text).expect("decode frame")
}

#[test]
fn two_peer_registry_and_pty_transcript() {
    let mut peers = Peers::new();
    peers.register("alice", "pty:0/master", 10, true);
    peers.register("bob", "pty:1/slave", 11, true);

    let bob = peers.lookup("bob", None).expect("bob registered");
    assert_eq!(bob.tty, "pty:1/slave");
    assert!(bob.writable);
    assert_eq!(bob.session, 11);
    let caller_endpoint = opposite_endpoint(&bob.tty).expect("caller endpoint");
    assert_eq!(caller_endpoint, "pty:1/master");
    assert_eq!(
        peers
            .lookup("alice", Some("pty:0/master"))
            .map(|peer| peer.session),
        Some(10)
    );
    peers.set_writable_for_session(11, false);
    assert!(
        !peers
            .lookup("bob", None)
            .expect("bob still registered")
            .writable
    );
    peers.set_writable_for_session(11, true);

    let mut pairs = Pairs::<Sink>::new();
    pairs.open(1).set_settings(Settings {
        mode: Mode::Raw,
        echo: false,
    });

    let mut caller = Session::caller(
        Address {
            user: "alice".to_string(),
            host: None,
        },
        None,
    );
    let mut listener = Session::listener();

    let Event::Send(invite) = caller.start() else {
        panic!("caller did not invite");
    };
    pairs.open(1).write_master(&line(&invite));
    let received = read(pairs.open(1), false);
    assert!(matches!(received, Frame::Invite { .. }));
    assert!(matches!(
        listener.receive(received),
        Event::ShowInvitation(_)
    ));

    let Event::Send(accept) = listener.accept() else {
        panic!("listener did not accept");
    };
    pairs.open(1).write_slave(&line(&accept));
    let received = read(pairs.open(1), true);
    assert_eq!(caller.receive(received), Event::Start);

    let Event::Send(data) = caller.local_input("hello") else {
        panic!("caller did not send text");
    };
    pairs.open(1).write_master(&line(&data));
    let received = read(pairs.open(1), false);
    assert_eq!(listener.receive(received), Event::Show("hello".to_string()));

    let Event::Send(data) = listener.local_input("hi") else {
        panic!("listener did not send text");
    };
    pairs.open(1).write_slave(&line(&data));
    let received = read(pairs.open(1), true);
    assert_eq!(caller.receive(received), Event::Show("hi".to_string()));

    let Event::Send(end) = caller.local_eof() else {
        panic!("caller did not end");
    };
    pairs.open(1).write_master(&line(&end));
    let received = read(pairs.open(1), false);
    assert_eq!(listener.receive(received), Event::RemoteEnded);

    let Event::Send(bye) = listener.escape() else {
        panic!("listener did not terminate");
    };
    pairs.open(1).write_slave(&line(&bye));
    let received = read(pairs.open(1), true);
    assert_eq!(caller.receive(received), Event::Closed);
    assert_eq!(pairs.len(), 1);
}

#[test]
fn loopback_runs_both_directions() {
    let output = Command::new(binary()).arg("--loopback").output().unwrap();
    assert!(output.status.success(), "{output:?}");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("invitation: Message from local"));
    assert!(stdout.contains("screen: split"));
    assert!(stdout.contains("caller> hello"));
    assert!(stdout.contains("peer< hello"));
    assert!(stdout.contains("peer> hi"));
    assert!(stdout.contains("caller< hi"));
    assert!(stdout.contains("local loopback closed (caller=0 peer=0)"));
}

#[test]
fn missing_address_is_a_usage_error() {
    let output = Command::new(binary()).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("usage: talk"));
}

#[test]
fn network_address_is_documented_as_unsupported() {
    let output = Command::new(binary()).arg("alice@remote").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("network talk to alice@remote is not implemented")
    );
}

#[test]
fn local_address_reports_the_missing_registry() {
    let output = Command::new(binary()).arg("alice").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr).contains("not reachable"));
}

#[test]
fn listener_reports_the_missing_transport() {
    let output = Command::new(binary())
        .args(["-l", "alice"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no peer transport for a local listener")
    );
}
