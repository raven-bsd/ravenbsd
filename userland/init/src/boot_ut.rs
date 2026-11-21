// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const RULE: &str =
    "--------------------------------------------------------------------------------";

fn rows() -> Vec<Row> {
    vec![
        Row {
            elapsed_ms: 1_250,
            name: "vfsd".into(),
            status: Status::Ok,
        },
        Row {
            elapsed_ms: 0,
            name: "svcd".into(),
            status: Status::Ok,
        },
        Row {
            elapsed_ms: 2_000,
            name: "optional".into(),
            status: Status::Warn,
        },
    ]
}

#[test]
fn no_color_or_dumb_selects_the_plain_transcript() {
    for (no_color, term_dumb) in [(true, false), (false, true), (true, true)] {
        crate::configure_boot_plain(no_color, term_dumb);
        assert!(
            plain_requested(),
            "NO_COLOR={no_color} TERM=dumb={term_dumb}"
        );
    }
    crate::configure_boot_plain(false, false);
    assert!(!plain_requested(), "a colour terminal selects ANSI");
}

#[test]
fn plain_screen_has_the_exact_golden_bytes() {
    let expected = format!(
        "R A V E N   B S D\n{RULE}\n000.00  svcd                ok\n001.25  vfsd                ok\n002.00  optional            warn\n{RULE}\n\nWelcome to RavenBSD 0.1 x86_64\n"
    );
    assert_eq!(render("x86_64", &rows(), false), expected);
}

#[test]
fn ansi_screen_has_the_exact_golden_bytes() {
    let expected = format!(
        "\x1b[1;36mR A V E N   B S D\x1b[0m\n\
         \x1b[2m{RULE}\x1b[0m\n\
         000.00  svcd                \x1b[32mok\x1b[0m\n\
         001.25  vfsd                \x1b[32mok\x1b[0m\n\
         002.00  optional            \x1b[33mwarn\x1b[0m\n\
         \x1b[2m{RULE}\x1b[0m\n\
         \n\
         Welcome to RavenBSD 0.1 x86_64\n"
    );
    assert_eq!(render("x86_64", &rows(), true), expected);
}

#[test]
fn ansi_screen_strips_to_the_plain_screen() {
    let ansi = render("aarch64", &rows(), true);
    let plain = render("aarch64", &rows(), false);
    assert_eq!(strip_ansi(&ansi), plain);
    assert!(ansi.contains("\x1b[1;36mR A V E N   B S D\x1b[0m"));
    assert!(ansi.contains("\x1b[32mok\x1b[0m"));
    assert!(ansi.contains("\x1b[33mwarn\x1b[0m"));
}

#[test]
fn rules_are_full_width_and_rows_are_sorted_by_completion() {
    let screen = render("x86_64", &rows(), false);
    let rules: Vec<&str> = screen
        .lines()
        .filter(|line| !line.is_empty() && line.chars().all(|c| c == '-'))
        .collect();
    assert_eq!(rules.len(), 2);
    assert!(rules.iter().all(|rule| rule.len() == WIDTH));
    let first_svcd = screen.find("svcd").expect("svcd row");
    let first_vfsd = screen.find("vfsd").expect("vfsd row");
    assert!(first_svcd < first_vfsd);
}

#[test]
fn welcome_names_the_arch_and_is_the_last_line() {
    for arch in ["x86_64", "aarch64"] {
        let screen = render(arch, &rows(), false);
        assert!(screen.ends_with(&format!("Welcome to RavenBSD 0.1 {arch}\n")));
    }
}
