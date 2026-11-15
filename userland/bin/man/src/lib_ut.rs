// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn parses_name_and_section_forms() {
    let first = parse(&["ls"]).expect("name");
    assert_eq!(first.action, Action::Render);
    assert_eq!(first.name.as_deref(), Some("ls"));
    assert_eq!(first.section, None);
    let second = parse(&["1", "ls"]).expect("section");
    assert_eq!(second.name.as_deref(), Some("ls"));
    assert_eq!(second.section, Some(1));
    let ninth = parse(&["9", "nope"]).expect("nine is a section");
    assert_eq!(ninth.section, Some(9));
}

#[test]
fn help_and_version_win() {
    assert_eq!(parse(&["-h"]).unwrap().action, Action::Help);
    assert_eq!(parse(&["--help"]).unwrap().action, Action::Help);
    assert_eq!(parse(&["-V"]).unwrap().action, Action::Version);
    assert_eq!(parse(&["--version"]).unwrap().action, Action::Version);
}

#[test]
fn colour_policy_forms() {
    assert_eq!(
        parse(&["--color=always", "ls"]).unwrap().color,
        Decision::Always
    );
    assert_eq!(
        parse(&["--color", "never", "ls"]).unwrap().color,
        Decision::Never
    );
    assert!(parse(&["--color=purple", "ls"]).is_err());
    assert!(parse(&["--color"]).is_err());
}

#[test]
fn bad_arguments_are_usage_errors() {
    assert!(parse(&[]).is_err());
    assert!(parse(&["0", "ls"]).is_err());
    assert!(parse(&["wat", "ls", "extra"]).is_err());
    assert!(parse(&["-x", "ls"]).is_err());
    assert!(parse(&["../ls"]).is_err());
}

#[test]
fn roots_include_default_config_and_manpath() {
    let roots = roots(
        "# comment\n/opt/man\n\n/usr/share/man\n",
        Some("/tmp/man:/opt/man"),
    );
    assert_eq!(
        roots,
        ["/usr/share/man", "/opt/man", "/tmp/man"]
            .iter()
            .map(|root| root.to_string())
            .collect::<Vec<_>>()
    );
}

#[test]
fn candidates_follow_section_then_root() {
    let roots = [String::from("/usr/share/man"), String::from("/opt/man")];
    let paths = candidates(&roots, "ls", None);
    assert_eq!(paths[0], (1, "/usr/share/man/man1/ls.1".to_string()));
    assert_eq!(paths[1], (1, "/opt/man/man1/ls.1".to_string()));
    assert_eq!(paths[2].0, 2);
    assert_eq!(
        candidates(&roots, "init", Some(8))[0],
        (8, "/usr/share/man/man8/init.8".to_string())
    );
}

#[test]
fn find_page_uses_the_first_existing_candidate() {
    let roots = [String::from("/usr/share/man")];
    let found = find_page(&roots, "ls", None, |path| path.ends_with("man1/ls.1")).expect("page");
    assert_eq!(found.0, 1);
    assert!(found.1.ends_with("man1/ls.1"));
    assert!(find_page(&roots, "nope", Some(9), |_| false).is_none());
}

#[test]
fn width_prefers_manwidth_and_bounds_values() {
    assert_eq!(width(Some("40"), Some("100")), 40);
    assert_eq!(width(Some("auto"), Some("100")), 100);
    assert_eq!(width(None, None), 80);
    assert_eq!(width(Some("0"), Some("12")), 80);
    assert_eq!(width(Some("500"), None), 80);
}

#[test]
fn pager_splits_commands_and_resolves_bare_names() {
    assert_eq!(
        pager(Some("cat -R")),
        Some(vec!["cat".to_string(), "-R".to_string()])
    );
    assert_eq!(pager(Some("  ")), None);
    let argv = vec!["cat".to_string()];
    assert_eq!(
        resolve_pager(&argv, |path| path == "/bin/cat"),
        Some("/bin/cat".to_string())
    );
    assert_eq!(resolve_pager(&["less".to_string()], |_| false), None);
}

#[test]
fn colour_needs_a_tty_for_auto_but_always_overrides() {
    assert!(color_enabled(Decision::Auto, true, false, false));
    assert!(!color_enabled(Decision::Auto, true, true, false));
    assert!(!color_enabled(Decision::Auto, true, false, true));
    assert!(!color_enabled(Decision::Auto, false, false, false));
    assert!(color_enabled(Decision::Always, false, true, true));
    assert!(!color_enabled(Decision::Never, true, false, false));
}

#[test]
fn rendering_is_plain_or_colour_as_requested() {
    let source = concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh NAME\n",
        ".Nm cmd\n",
        ".Nd a page\n",
    );
    let palette = Palette::raven_violet();
    let plain = render_page(source, 80, &palette, false, None, None).expect("plain");
    assert!(!plain.contains('\u{1b}'));
    assert!(plain.contains("cmd - a page"));
    let colour =
        render_page(source, 80, &palette, true, Some("truecolor"), Some("xterm")).expect("colour");
    assert!(colour.contains("\u{1b}["));
}

#[test]
fn a_custom_runtime_palette_reaches_the_renderer() {
    let palette = Palette::from_env_spec(
        "#ff0000:#6272a4:#44475a:#50fa7b:#ffb86c:#ff5555:#8be9fd:#f8f8f2:#ffb86c:#50fa7b:#ff79c6",
    )
    .expect("spec");
    let source = concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh NAME\n",
        ".Nm cmd\n",
        ".Nd a page\n",
    );
    let output =
        render_page(source, 80, &palette, true, Some("truecolor"), Some("xterm")).expect("colour");
    assert!(output.contains("\u{1b}[38;2;255;0;0m"), "{output}");
}

#[test]
fn an_invalid_spec_falls_back_to_the_compiled_palette() {
    assert_eq!(
        palette_from_env_spec(Some("not:a:palette")),
        Palette::current()
    );
    assert_eq!(palette_from_env_spec(None), Palette::current());
}

#[test]
fn the_version_line_names_the_tool() {
    assert!(version_line().starts_with("man 0.1.0 "));
}
