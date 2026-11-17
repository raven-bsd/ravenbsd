// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the write delivery model.

use alloc::string::String;

use super::{Directory, Error, Options, Terminal, deliver, normalize, parse_args};

struct Users {
    known: bool,
}

impl Directory for Users {
    fn known(&self, _user: &str) -> bool {
        self.known
    }
}

struct Term {
    logged: bool,
    mesg: bool,
    sent: String,
}

impl Terminal for Term {
    fn logged_in(&mut self, _user: &str, _tty: Option<&str>) -> bool {
        self.logged
    }
    fn mesg_enabled(&mut self, _user: &str, _tty: Option<&str>) -> bool {
        self.mesg
    }
    fn send(&mut self, _user: &str, _tty: Option<&str>, message: &str) -> bool {
        self.sent.push_str(message);
        true
    }
}

fn target() -> Options {
    Options {
        user: "alice".into(),
        tty: None,
    }
}

#[test]
fn parses_user_and_tty() {
    assert_eq!(
        parse_args(["alice".into(), "tty1".into()]).unwrap(),
        Options {
            user: "alice".into(),
            tty: Some("tty1".into()),
        }
    );
    assert!(parse_args(Vec::<String>::new()).is_err());
    assert!(parse_args(["a".into(), "b".into(), "c".into()]).is_err());
}

#[test]
fn delivers_a_normalized_message() {
    let mut terminal = Term {
        logged: true,
        mesg: true,
        sent: String::new(),
    };
    assert_eq!(
        deliver(
            &target(),
            "hello\nthere\n",
            &Users { known: true },
            &mut terminal
        ),
        Ok(14)
    );
    assert_eq!(terminal.sent, "hello\r\nthere\r\n");
    assert_eq!(normalize("one\ntwo\n"), "one\r\ntwo\r\n");
}

#[test]
fn reports_unknown_logged_out_and_mesg_off() {
    let mut terminal = Term {
        logged: true,
        mesg: true,
        sent: String::new(),
    };
    assert_eq!(
        deliver(&target(), "x", &Users { known: false }, &mut terminal),
        Err(Error::UnknownUser)
    );
    let mut terminal = Term {
        logged: false,
        mesg: true,
        sent: String::new(),
    };
    assert_eq!(
        deliver(&target(), "x", &Users { known: true }, &mut terminal),
        Err(Error::NotLoggedIn)
    );
    let mut terminal = Term {
        logged: true,
        mesg: false,
        sent: String::new(),
    };
    assert_eq!(
        deliver(&target(), "x", &Users { known: true }, &mut terminal),
        Err(Error::MesgOff)
    );
}
