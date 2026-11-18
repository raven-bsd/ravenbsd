// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the getty retry and shell-handoff policy.

use std::collections::VecDeque;

use super::*;

struct MockSession {
    lines: VecDeque<String>,
    logins: VecDeque<Result<i32, SessionError>>,
    shell_status: Result<i32, SessionError>,
    console: Vec<u8>,
    login_users: Vec<String>,
    shell: Option<(String, Vec<String>, Vec<String>)>,
}

impl MockSession {
    fn new(
        lines: &[&str],
        logins: &[Result<i32, SessionError>],
        shell_status: Result<i32, SessionError>,
    ) -> Self {
        Self {
            lines: lines.iter().map(|line| String::from(*line)).collect(),
            logins: logins.iter().copied().collect(),
            shell_status,
            console: Vec::new(),
            login_users: Vec::new(),
            shell: None,
        }
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.console).into_owned()
    }
}

impl Session for MockSession {
    fn write(&mut self, bytes: &[u8]) -> Result<(), SessionError> {
        self.console.extend_from_slice(bytes);
        Ok(())
    }

    fn read_line(&mut self) -> Result<String, SessionError> {
        self.lines.pop_front().ok_or(())
    }

    fn run_login(&mut self, username: &str) -> Result<i32, SessionError> {
        self.login_users.push(String::from(username));
        self.logins.pop_front().unwrap_or(Ok(0))
    }

    fn run_shell(&mut self, shell: &str, args: &[&str], env: &[&str]) -> Result<i32, SessionError> {
        self.shell = Some((
            String::from(shell),
            args.iter().map(|arg| String::from(*arg)).collect(),
            env.iter().map(|value| String::from(*value)).collect(),
        ));
        self.shell_status
    }
}

fn profile() -> Profile {
    Profile {
        path: String::from("/bin:/sbin:/usr/bin:/usr/sbin"),
        home: String::from("/root"),
        term: String::from("vt100"),
        user: String::from("root"),
        shell: String::from("/bin/brush"),
    }
}

#[test]
fn a_wrong_password_re_prompts_and_only_a_success_starts_the_shell() {
    let mut session = MockSession::new(&["root", "root"], &[Ok(1), Ok(0)], Ok(0));
    let status = run_session(&mut session, &profile(), "/bin/brush");

    assert_eq!(status, 0);
    assert_eq!(session.login_users, ["root", "root"]);
    assert_eq!(session.text().matches("login: ").count(), 2);
    let (shell, args, env) = session.shell.expect("shell started");
    assert_eq!(shell, "/bin/brush");
    assert_eq!(args, ["brush", "-i", "-l"]);
    assert!(env.contains(&String::from("PATH=/bin:/sbin:/usr/bin:/usr/sbin")));
    assert!(env.contains(&String::from("HOME=/root")));
    assert!(env.contains(&String::from("TERM=vt100")));
    assert!(env.contains(&String::from("USER=root")));
    assert!(env.contains(&String::from("LOGNAME=root")));
    assert!(env.contains(&String::from("SHELL=/bin/brush")));
    assert!(env.contains(&String::from("PWD=/")));
}

#[test]
fn an_invalid_user_name_is_answered_without_spawning_login() {
    let mut session = MockSession::new(&["root/", "root"], &[Ok(0)], Ok(0));
    let status = run_session(&mut session, &profile(), "/bin/brush");

    assert_eq!(status, 0);
    assert_eq!(session.login_users, ["root"]);
    assert!(session.text().contains("Login incorrect"));
    assert_eq!(session.text().matches("login: ").count(), 2);
    assert!(session.shell.is_some());
}

#[test]
fn a_failed_login_spawn_retries_instead_of_exiting() {
    let mut session = MockSession::new(&["root", "root"], &[Err(SessionError), Ok(0)], Ok(0));
    let status = run_session(&mut session, &profile(), "/bin/brush");

    assert_eq!(status, 0);
    assert_eq!(session.login_users, ["root", "root"]);
    assert!(session.text().contains("unable to start login"));
    assert_eq!(session.text().matches("unable to start login").count(), 1);
    assert!(session.shell.is_some());
}

#[test]
fn the_loop_does_not_start_a_shell_before_login_succeeds() {
    let mut session =
        MockSession::new(&["root", "root", "root", ""], &[Ok(1), Ok(1), Ok(1)], Ok(0));
    let status = run_session(&mut session, &profile(), "/bin/brush");

    assert_eq!(status, 1);
    assert_eq!(session.login_users, ["root", "root", "root"]);
    assert!(session.shell.is_none());
    // Three rejected names, the empty line, then the prompt that sees end of input.
    assert_eq!(session.text().matches("login: ").count(), 5);
}

#[test]
fn a_non_brush_shell_keeps_an_empty_argument_vector() {
    assert!(shell_args("/bin/rsh").is_empty());
    assert_eq!(shell_args("/bin/brush"), ["brush", "-i", "-l"]);
    assert_eq!(shell_args("brush"), ["brush", "-i", "-l"]);
}

#[test]
fn the_shell_status_and_a_spawn_failure_are_returned() {
    let mut session = MockSession::new(&["root"], &[Ok(0)], Ok(7));
    assert_eq!(run_session(&mut session, &profile(), "/bin/brush"), 7);

    let mut session = MockSession::new(&["root"], &[Ok(0)], Err(SessionError));
    assert_eq!(run_session(&mut session, &profile(), "/bin/brush"), 1);
    assert!(session.text().contains("unable to start the session shell"));
}
