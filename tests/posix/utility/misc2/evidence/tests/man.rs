// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Behaviour cases for `man`.

use man::{UsageError, find_page, palette_from_env_spec, parse, render_page};

const PAGE: &str = concat!(
    ".Dt CMD 1\n",
    ".Os RavenBSD\n",
    ".Sh NAME\n",
    ".Nm cmd\n",
    ".Nd a fixture page\n",
    ".Sh SYNOPSIS\n",
    ".Nm\n",
    ".Op Fl n\n",
);

#[test]
fn a_small_fixture_page_renders_plainly() {
    let palette = palette_from_env_spec(None);
    let output = render_page(PAGE, 72, &palette, false, None, None).expect("fixture page");
    assert!(output.contains("cmd - a fixture page"), "{output}");
    assert!(output.contains("SYNOPSIS"), "{output}");
    assert!(!output.contains('\u{1b}'), "plain rendering has no escapes");
}

#[test]
fn an_unknown_page_has_no_candidate() {
    let roots = [String::from("/usr/share/man")];
    assert!(find_page(&roots, "no-such-page", None, |_| false).is_none());
    assert!(find_page(&roots, "no-such-page", Some(9), |_| false).is_none());
}

#[test]
fn section_and_name_lookup_are_validated() {
    let invocation = parse(&["1", "cmd"]).expect("valid invocation");
    assert_eq!(invocation.section, Some(1));
    assert_eq!(invocation.name.as_deref(), Some("cmd"));
    assert_eq!(parse(&["0", "cmd"]), Err(UsageError));
    assert_eq!(parse(&["../escape"]), Err(UsageError));
}
