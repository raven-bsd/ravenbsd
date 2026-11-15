// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn page(text: &str) -> Document {
    Document::parse(text).expect("page")
}

fn plain(text: &str) -> String {
    page(text).render_plain(80)
}

#[test]
fn metadata_and_name_line() {
    let output = plain(concat!(
        ".\\\" comment\n",
        ".Dd November 1, 2026\n",
        ".Dt FOO 1\n",
        ".Os RavenBSD\n",
        ".Sh NAME\n",
        ".Nm foo\n",
        ".Nd a test page\n",
    ));
    let mut lines = output.lines();
    assert_eq!(lines.next(), Some("FOO(1)  RavenBSD  November 1, 2026"));
    assert_eq!(lines.next(), Some(""));
    assert_eq!(lines.next(), Some("NAME"));
    assert!(output.contains("foo - a test page"));
}

#[test]
fn section_and_subsection() {
    let output = plain(concat!(
        ".Dt BAR 8\n",
        ".Os RavenBSD\n",
        ".Sh DESCRIPTION\n",
        "Text.\n",
        ".Ss Subsection\n",
        "More.\n",
    ));
    assert!(output.contains("\nDESCRIPTION\n"));
    assert!(output.contains("    Subsection\n"));
    assert!(output.contains("    Text.\n"));
}

#[test]
fn macro_matrix() {
    let output = plain(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh SYNOPSIS\n",
        ".Nm cmd\n",
        ".Op Fl abc\n",
        ".Op Ar file\n",
        ".Sh DESCRIPTION\n",
        ".Fl -json\n",
        ".Ar path\n",
        ".Pa /etc/x\n",
        ".Cm start\n",
        ".Ev VAR\n",
        ".Dv CONST\n",
        ".Er EINVAL\n",
        ".Fn fn \"int x\" \"char *y\"\n",
        ".Xr ls 1\n",
        ".Lk https://example.invalid\n",
        ".Mt root@example.invalid\n",
        ".In stdio.h\n",
        ".Lb c\n",
        ".Dq \"hello world\"\n",
        ".Sq x\n",
        ".Ql y\n",
    ));
    for expected in [
        "cmd [-abc] [file]",
        "--json path /etc/x start VAR CONST EINVAL",
        "fn(int x, char *y)",
        "ls(1)",
        "https://example.invalid",
        "root@example.invalid",
        "#include <stdio.h>",
        "-lc",
        "\"hello world\"",
        "'x'",
        "'y'",
    ] {
        assert!(
            output.contains(expected),
            "missing {expected:?} in:\n{output}"
        );
    }
}

#[test]
fn tag_list_and_literal_block() {
    let output = plain(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh OPTIONS\n",
        ".Bl -tag -width Ds\n",
        ".It Fl a\n",
        "Include dots.\n",
        ".It Pa /etc/x\n",
        "Configuration.\n",
        ".El\n",
        ".Bd -literal -offset indent\n",
        "line one\n",
        "  line two\n",
        ".Ed\n",
    ));
    for expected in [
        "-a",
        "Include dots.",
        "/etc/x",
        "Configuration.",
        "line one",
    ] {
        assert!(
            output.contains(expected),
            "missing {expected:?} in:\n{output}"
        );
    }
    assert!(output.contains("          line two"), "{output}");
}

#[test]
fn bullet_and_enum_lists() {
    let output = plain(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh LISTS\n",
        ".Bl -bullet\n",
        ".It\n",
        "one\n",
        ".It\n",
        "two\n",
        ".El\n",
        ".Bl -enum\n",
        ".It\n",
        "first\n",
        ".El\n",
    ));
    assert!(output.contains("    -\n        one\n"), "{output}");
    assert!(output.contains("    1.\n        first\n"), "{output}");
}

#[test]
fn unsupported_macro_renders_its_text() {
    let output = plain(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh DESCRIPTION\n",
        ".Zz hello world\n",
        "After.\n",
    ));
    assert!(output.contains("hello world"), "{output}");
    assert!(output.contains("After."), "{output}");
}

#[test]
fn wrapping_respects_width() {
    let source = concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh DESCRIPTION\n",
        "one two three four five six seven eight nine ten eleven twelve thirteen fourteen\n",
    );
    let lines = page(source).render(30);
    for line in lines.iter().skip(2) {
        assert!(line.text().chars().count() <= 30, "{:?}", line.text());
    }
    let words: Vec<&str> = lines
        .iter()
        .skip(2)
        .flat_map(|line| line.spans.iter())
        .flat_map(|span| span.text.split_whitespace())
        .filter(|word| *word != "DESCRIPTION")
        .collect();
    assert_eq!(
        words,
        [
            "one", "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten",
            "eleven", "twelve", "thirteen", "fourteen"
        ]
    );
}

#[test]
fn ls_page_is_a_clean_golden() {
    let output = Document::parse(include_str!("../../../docs/man1/ls.1"))
        .expect("ls.1")
        .render_plain(80);
    assert!(
        output.starts_with("LS(1)  RavenBSD  November 12, 2026\n"),
        "{output}"
    );
    for expected in [
        "\nNAME\n",
        "ls - list directory contents\n",
        "\nSYNOPSIS\n",
        "ls [-alh] [path ...]\n",
        "\nDESCRIPTION\n",
        "utility lists directory entries sorted by name.",
        "\nEXIT STATUS\n",
        "The utility exits 0 on success",
        "\nSEE ALSO\n",
        "cat(1), stat(1)",
    ] {
        assert!(
            output.contains(expected),
            "missing {expected:?} in:\n{output}"
        );
    }
    assert!(!output.contains("\n.Sh "));
    assert!(!output.contains("\n.Nm "));
    assert!(!output.contains('\u{1b}'));
}

#[test]
fn raven_and_init_pages_render_their_sections() {
    let raven = Document::parse(include_str!("../../../docs/man1/raven.1"))
        .expect("raven.1")
        .render_plain(80);
    assert!(raven.starts_with("RAVEN(1)  RavenBSD  November 14, 2026\n"));
    for expected in [
        "\nNAME\n",
        "raven - RavenBSD administration client\n",
        "\nGLOBAL OPTIONS\n",
        "NO_COLOR",
    ] {
        assert!(raven.contains(expected), "missing {expected:?}");
    }
    let init = Document::parse(include_str!("../../../docs/man8/init.8"))
        .expect("init.8")
        .render_plain(80);
    assert!(init.starts_with("INIT(8)  RavenBSD  November 11, 2026\n"));
    for expected in [
        "\nNAME\n",
        "init - RavenBSD PID 1 service manager\n",
        "\nFILES\n",
        "/etc/svcd.conf",
    ] {
        assert!(init.contains(expected), "missing {expected:?}");
    }
}

#[test]
fn fo_and_fc_form_a_function_call() {
    let output = plain(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh SYNOPSIS\n",
        ".Fo open\n",
        ".Fc\n",
    ));
    assert!(output.contains("open()"), "{output}");
}

#[test]
fn ls_page_exact_golden() {
    let expected = concat!(
        "LS(1)  RavenBSD  November 12, 2026\n",
        "\n",
        "NAME\n",
        "\n",
        "    ls - list directory contents\n",
        "\n",
        "SYNOPSIS\n",
        "\n",
        "    ls [-alh] [path ...]\n",
        "\n",
        "DESCRIPTION\n",
        "\n",
        "    The ls utility lists directory entries sorted by name. Without a path, the\n",
        "    current directory is listed. Directories get a trailing '/' in short\n",
        "    listings.\n",
        "\n",
        "    The options are:\n",
        "    -a\n",
        "        Include names beginning with a dot.\n",
        "    -h\n",
        "        Accepted for compatibility; sizes are printed in bytes.\n",
        "    -l\n",
        "        Print the mode string, size and name.\n",
        "\n",
        "EXIT STATUS\n",
        "\n",
        "    The utility exits 0 on success, and >0 if an error occurs.\n",
        "    A usage error exits with status 2.\n",
        "\n",
        "SEE ALSO\n",
        "\n",
        "    cat(1), stat(1)\n",
        "\n",
        "STANDARDS\n",
        "\n",
        "    RavenBSD.\n",
    );
    let output = Document::parse(include_str!("../../../docs/man1/ls.1"))
        .expect("ls.1")
        .render_plain(80);
    assert_eq!(output, expected);
}

#[test]
fn man_page_renders_its_own_macros() {
    let output = Document::parse(include_str!("../../../docs/man1/man.1"))
        .expect("man.1")
        .render_plain(80);
    for expected in [
        "MAN(1)  RavenBSD",
        "man - display RavenBSD manual pages",
        "-h, --help",
        "--color=auto|always|never",
        "MANPAGER, PAGER",
        "/usr/share/man",
    ] {
        assert!(
            output.contains(expected),
            "missing {expected:?} in:\n{output}"
        );
    }
}

#[test]
fn plain_lines_never_contain_escapes_but_painted_lines_do() {
    let document = page(concat!(
        ".Dt CMD 1\n",
        ".Os RavenBSD\n",
        ".Sh NAME\n",
        ".Nm cmd\n",
        ".Nd a page\n",
    ));
    let lines = document.render(80);
    let palette = Palette::raven_violet();
    for line in &lines {
        assert!(
            !line
                .painted(&palette, Depth::TrueColor, false)
                .contains('\u{1b}')
        );
    }
    assert!(lines.iter().any(|line| {
        line.painted(&palette, Depth::TrueColor, true)
            .contains('\u{1b}')
    }));
}

#[test]
fn invalid_pages_are_rejected() {
    assert_eq!(Document::parse(""), Err(Error::Empty));
    assert_eq!(
        Document::parse(".Sh NAME\nText.\n"),
        Err(Error::MissingTitle)
    );
    assert_eq!(
        Document::parse(".Dt CMD 0\n.Os RavenBSD\n"),
        Err(Error::InvalidSection)
    );
}
