<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Boot presentation

K191 made the release console quiet; K374 adds the styled boot screen before getty. The screen is
printed by `svcd` after the plan is up and before `getty` starts, so `login: ` is always the last
line and needs no keypress.

## Screen

The default release transcript is:

```
R A V E N   B S D
--------------------------------------------------------------------------------
000.00  vfsd                ok
001.25  netd                ok
--------------------------------------------------------------------------------

Welcome to RavenBSD 0.1 x86_64
login:
```

The exact sequence is:

1. title `R A V E N   B S D`;
2. a full-width rule (80 columns when the console size is unknown);
3. one `000.00  step  status` row per enabled non-getty boot step, sorted by completion time;
4. a closing rule;
5. one blank line;
6. `Welcome to RavenBSD 0.1 <arch>`;
7. getty's `login: ` prompt, last.

A required step that is not ready is `fail`; a non-required step is `warn`. Failure diagnostics
(`svcd: <name> failed: ...`) are never hidden.

## Colour

The default target screen uses ANSI: bold/cyan title, dim rules, green `ok`, yellow `warn`, red
`fail`. `NO_COLOR` or `TERM=dumb` selects the byte-identical plain transcript (the renders differ
only by SGR escapes; `boot_ut.rs` proves the equality after stripping them). The renderer keeps a
single 80-column fallback until a console-size query is added.

## Verbose diagnostics

The per-service `svcd: <name> timing: ...` lines remain opt-in:

* `profiles/release.d/05-boot-quiet.toml` stages `/etc/raven/boot-quiet`.
* To enable them, stage `/etc/raven/boot-verbose` (content `1`) in the initfs, or place it in a
  writable `/etc/raven` on a UFS root before boot. A non-zero file wins over the quiet marker.
* The screen table is compact and does not duplicate the verbose timing lines.

## `/root` and login

`profiles/shell.d/00-root.toml` stages `/root/.keep`; the release profile inherits it, so
`HOME=/root` and `cd` work without a UFS root. `getty` no longer prints its old
`RavenBSD 0.1 (arch) (tty:console)` banner: the init screen owns the title and `Welcome` line, and
getty owns only `login: `. Login then prints `/etc/motd` after authentication as before.

## Evidence

* `boot_ut.rs` byte-goldens the plain and ANSI screens and the plain/ANSI equality.
* `cargo nextest run -p init -p getty` covers the renderer and getty session policy.
* `boot-screen` qtest strips ANSI, asserts the seven ordered elements and waits for the final
  `login:` prompt with no input.
* `release-login` (x86_64/aarch64) still reaches the shell and exits without weakening the existing
  login/ping assertions.
