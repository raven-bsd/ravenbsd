// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Header parity (C67 5.E and 7(d)): the installed Solaris headers and `consts.rs` agree.
//!
//! The test parses `#define NAME <integer>` rows and the typedef/visibility lines. It is a host
//! test; the header compile matrix itself is `raven-libc-hdrcheck`.

use std::path::{Path, PathBuf};

use raven_libc_personality_solaris::consts::{
    PORT_SOURCE_ALERT, PORT_SOURCE_FD, PORT_SOURCE_TIMER, PORT_SOURCE_USER, PORT_SOURCES,
    THR_BOUND, THR_DAEMON, THR_DETACHED, THR_FLAGS, THR_NEW_LWP, THR_SUSPENDED, USYNC_PROCESS,
    USYNC_THREAD, USYNC_TYPES,
};

fn include(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("ravenlibc root")
        .join("include")
        .join(name)
}

fn text(name: &str) -> String {
    let path = include(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// Parse `#define NAME <integer expression>` from a header.
fn define(source: &str, name: &str) -> Option<i64> {
    for line in source.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("#define") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(value) = rest.strip_prefix(name) else {
            continue;
        };
        if !value.starts_with(char::is_whitespace) {
            continue;
        }
        let value = value.trim();
        let value = value.split("/*").next().unwrap_or(value);
        let value = value.split("//").next().unwrap_or(value);
        let expression: String = value
            .chars()
            .filter(|character| {
                !character.is_whitespace() && *character != '(' && *character != ')'
            })
            .filter(|character| !matches!(character, 'u' | 'U' | 'l' | 'L'))
            .collect();
        let (negative, body) = match expression.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, expression.as_str()),
        };
        let number = if let Some(hex) = body.strip_prefix("0x").or_else(|| body.strip_prefix("0X"))
        {
            i64::from_str_radix(hex, 16).ok()?
        } else {
            body.parse::<i64>().ok()?
        };
        return Some(if negative { -number } else { number });
    }
    None
}

#[test]
fn port_sources_match_the_header() {
    let source = text("port.h");
    assert_eq!(
        define(&source, "PORT_SOURCE_TIMER"),
        Some(i64::from(PORT_SOURCE_TIMER))
    );
    assert_eq!(
        define(&source, "PORT_SOURCE_USER"),
        Some(i64::from(PORT_SOURCE_USER))
    );
    assert_eq!(
        define(&source, "PORT_SOURCE_FD"),
        Some(i64::from(PORT_SOURCE_FD))
    );
    assert_eq!(
        define(&source, "PORT_SOURCE_ALERT"),
        Some(i64::from(PORT_SOURCE_ALERT))
    );
}

#[test]
fn thr_flags_match_the_header() {
    let source = text("thread.h");
    assert_eq!(define(&source, "THR_BOUND"), Some(THR_BOUND));
    assert_eq!(define(&source, "THR_NEW_LWP"), Some(THR_NEW_LWP));
    assert_eq!(define(&source, "THR_DETACHED"), Some(THR_DETACHED));
    assert_eq!(define(&source, "THR_SUSPENDED"), Some(THR_SUSPENDED));
    assert_eq!(define(&source, "THR_DAEMON"), Some(THR_DAEMON));
}

#[test]
fn usync_values_match_the_header() {
    let source = text("thread.h");
    assert_eq!(
        define(&source, "USYNC_THREAD"),
        Some(i64::from(USYNC_THREAD))
    );
    assert_eq!(
        define(&source, "USYNC_PROCESS"),
        Some(i64::from(USYNC_PROCESS))
    );
}

#[test]
fn every_table_row_is_in_the_header() {
    let ports = text("port.h");
    for (name, value) in PORT_SOURCES {
        assert_eq!(define(&ports, name), Some(i64::from(*value)), "{name}");
    }
    let threads = text("thread.h");
    for (name, value) in THR_FLAGS {
        assert_eq!(define(&threads, name), Some(*value), "{name}");
    }
    for (name, value) in USYNC_TYPES {
        assert_eq!(define(&threads, name), Some(i64::from(*value)), "{name}");
    }
}

#[test]
fn port_event_is_pinned_to_twenty_four_bytes() {
    let source = text("port.h");
    assert!(source.contains("_Static_assert(sizeof(port_event_t) == 24"));
    assert_eq!(
        core::mem::size_of::<raven_libc_personality_solaris::consts::PortEvent>(),
        24
    );
}

#[test]
fn port_header_declares_the_seven_functions() {
    let source = text("port.h");
    for symbol in [
        "port_create",
        "port_associate",
        "port_dissociate",
        "port_get",
        "port_getn",
        "port_send",
        "port_alert",
    ] {
        assert!(source.contains(symbol), "{symbol}");
    }
}

#[test]
fn thread_header_declares_type_aliases() {
    let source = text("thread.h");
    for alias in [
        "typedef pthread_t\t\tthread_t;",
        "typedef pthread_mutex_t\t\tmutex_t;",
        "typedef pthread_cond_t\t\tcond_t;",
        "typedef pthread_rwlock_t\trwlock_t;",
        "typedef pthread_key_t\t\tthread_key_t;",
        "typedef struct timespec\t\ttimestruc_t;",
    ] {
        assert!(source.contains(alias), "{alias}");
    }
}

#[test]
fn thread_header_declares_the_eighteen_functions() {
    let source = text("thread.h");
    for symbol in [
        "thr_create",
        "thr_exit",
        "thr_join",
        "thr_self",
        "thr_yield",
        "thr_kill",
        "thr_sigsetmask",
        "thr_keycreate",
        "thr_setspecific",
        "thr_getspecific",
        "thr_min_stack",
        "thr_main",
        "thr_getprio",
        "thr_setprio",
        "thr_setconcurrency",
        "thr_getconcurrency",
        "thr_suspend",
        "thr_continue",
    ] {
        assert!(source.contains(symbol), "{symbol}");
    }
}

#[test]
fn synch_header_declares_every_synchronisation_function() {
    let source = text("synch.h");
    for symbol in [
        "mutex_init",
        "mutex_destroy",
        "mutex_lock",
        "mutex_trylock",
        "mutex_unlock",
        "cond_init",
        "cond_destroy",
        "cond_wait",
        "cond_timedwait",
        "cond_signal",
        "cond_broadcast",
        "rwlock_init",
        "rwlock_destroy",
        "rw_rdlock",
        "rw_wrlock",
        "rw_tryrdlock",
        "rw_trywrlock",
        "rw_unlock",
        "sema_init",
        "sema_destroy",
        "sema_wait",
        "sema_trywait",
        "sema_post",
    ] {
        assert!(source.contains(symbol), "{symbol}");
    }
}

#[test]
fn every_header_has_a_named_include_guard() {
    assert!(text("port.h").contains("#ifndef\t_PORT_H_"));
    assert!(text("thread.h").contains("#ifndef\t_THREAD_H_"));
    assert!(text("synch.h").contains("#ifndef\t_SYNCH_H_"));
    assert!(text("sys/port.h").contains("#ifndef\t_SYS_PORT_H_"));
}

#[test]
fn sys_port_forwards_to_port() {
    let source = text("sys/port.h");
    assert!(source.contains("#include <port.h>"));
}

#[test]
fn every_header_guards_its_body_with_solaris_visibility() {
    assert!(text("port.h").contains("#if __SOLARIS_VISIBLE"));
    assert!(text("thread.h").contains("#if __SOLARIS_VISIBLE"));
    assert!(text("synch.h").contains("#if __SOLARIS_VISIBLE"));
}

#[test]
fn no_solaris_identity_macro_is_defined() {
    for header in ["port.h", "thread.h", "synch.h", "sys/port.h"] {
        let source = text(header);
        assert!(!source.contains("#define __sun"), "{header}");
        assert!(!source.contains("#define __SVR4"), "{header}");
    }
}

#[test]
fn no_absent_facility_is_declared() {
    for header in ["port.h", "thread.h", "synch.h", "sys/port.h"] {
        let source = text(header);
        for absent in [
            "door_create",
            "kstat_open",
            "SIGEV_PORT",
            "lwp_create",
            "port_sendn",
        ] {
            assert!(!source.contains(absent), "{header} declares {absent}");
        }
    }
}

#[test]
fn headers_do_not_cite_a_donor_path() {
    for header in ["port.h", "thread.h", "synch.h", "sys/port.h"] {
        let source = text(header);
        assert!(!source.contains("illumos-gate"), "{header}");
    }
}
