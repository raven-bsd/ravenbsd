// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn ls_colors_256_snapshot() {
    assert_eq!(
        ls_colors(&Palette::raven_violet(), Depth::Ansi256),
        "di=38;5;183:ln=38;5;117:ex=38;5;115:fi=38;5;145:or=38;5;211:mi=38;5;211:*.tar=38;5;222:*.gz=38;5;222"
    );
}

#[test]
fn ls_colors_16_snapshot() {
    assert_eq!(
        ls_colors(&Palette::raven_violet(), Depth::Ansi16),
        "di=95:ln=94:ex=92:fi=90:or=91:mi=91:*.tar=93:*.gz=93"
    );
}

#[test]
fn ls_colors_truecolor_snapshot() {
    assert_eq!(
        ls_colors(&Palette::raven_violet(), Depth::TrueColor),
        "di=38;2;196;167;255:ln=38;2;98;182;255:ex=38;2;123;216;143:fi=38;2;139;139;158:or=38;2;255;107;129:mi=38;2;255;107;129:*.tar=38;2;242;193;78:*.gz=38;2;242;193;78"
    );
}

#[test]
fn less_termcap_256_snapshot() {
    assert_eq!(
        less_termcap(&Palette::raven_violet(), Depth::Ansi256),
        concat!(
            "export LESS_TERMCAP_md=$'\\033[38;5;141m'\n",
            "export LESS_TERMCAP_me=$'\\033[0m'\n",
            "export LESS_TERMCAP_so=$'\\033[38;5;117m'\n",
            "export LESS_TERMCAP_se=$'\\033[0m'\n",
            "export LESS_TERMCAP_us=$'\\033[38;5;183m'\n",
            "export LESS_TERMCAP_ue=$'\\033[0m'\n",
        )
    );
}

#[test]
fn the_adapters_never_contain_a_raw_escape() {
    let palette = Palette::raven_violet();
    for depth in [Depth::Ansi16, Depth::Ansi256, Depth::TrueColor] {
        assert!(!ls_colors(&palette, depth).contains('\x1b'));
        assert!(!less_termcap(&palette, depth).contains('\x1b'));
    }
}

#[test]
fn shell_fragment_has_both_adapters() {
    let value = shell_fragment(&Palette::mono(), Depth::Ansi16);
    assert!(value.contains("LS_COLORS="));
    assert!(value.contains("LESS_TERMCAP_md="));
}
