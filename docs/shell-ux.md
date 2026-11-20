<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Shell UX: bobthefish prompt, completion, highlighting and coloured ls (K192)

Status: OBSERVED on the `release` login. The prompt keeps a stable `raven# ` last line; the
segments above it carry the bobthefish information. `ed` is shipped, but its console/stdin I/O
belongs to K186.

## Prompt

`etc/brush/bobtheshell.sh` is the interactive prompt. Its segment line shows:

* the exit status, only when nonzero, in bold red (`127`);
* the folded current path: `~/w/r/ravenbsd-demos` for
  `/root/work/ravenos-workspace/ravenbsd-demos`, `~` for `HOME`;
* the git branch in green when inside a work tree (`git:main*`), with a bounded
  `git status --porcelain` for the dirty marker;
* the virtualenv basename and background-job count when present.

The last line is `raven# ` (root) or `raven$ ` (non-root), so `release-login` and the other
serial gates keep a stable marker. The interactive line editor draws only the last line; the
segment line is shown once above it.

Plain modes are deterministic and contain no escapes:

| Mode | Selection | Prompt |
|---|---|---|
| stable plain | `RAVEN_PROMPT=plain` or `RAVEN_PLAIN_PROMPT=1` | `raven# ` |
| colour disabled | `NO_COLOR` or `TERM=dumb` | `root@raven:~/w/r/ravenbsd-demos# ` |

## Completion and highlighting

On RavenBSD the default backend is still `Minimal`, but for a terminal it now delegates to the
Vt line editor **without** the basic style. That editor uses the same semantic highlighter and
completion engine as Reedline:

* `ec<TAB>` inserts `echo` (a unique candidate);
* `c<TAB><TAB>` opens the candidate menu in columns (`caller cat command ...`);
* a command that is not found is painted bold red after the cursor leaves the word, before
  Enter;
* builtins, aliases, keywords, quoted strings, parameters and operators use the shared palette.

### History autosuggestions

After the cursor, the Vt editor shows the newest matching history suffix dimmed. Right or End
accepts the whole hint; any other key keeps the typed text. The match is local history only,
newest first, and multi-line entries are skipped.

`NO_COLOR`, `TERM=dumb`, `RAVEN_PLAIN_PROMPT=1` and `RAVEN_AUTOSUGGEST=0` disable the hint
entirely; plain mode draws no extra text or escape sequence. The shell environment is consulted
per prompt, so an `export RAVEN_AUTOSUGGEST=0` takes effect on the next line.

`NO_COLOR`, `TERM=dumb` and `RAVEN_PLAIN_PROMPT=1` keep the drawn line plain. Reedline itself
still panics on RavenBSD's TLS teardown (`cannot access a Thread Local Storage value during or
after destruction`), so it is not the selected backend; the Vt editor delivers the same
visible menu and semantic colours without that runtime path.

## Coloured listings

`etc/profile` defines the aliases only for interactive login shells:

```sh
alias l='ls -CF --color=auto'
alias ll='ls -lah --color=auto'
alias ls='ls --color=auto'
```

`brush -c` never reads `/etc/profile`, and the block is guarded by `$-`, so scripts keep the
base `PATH` and `/bin/ls` remains script-safe. `LS_COLORS` supplies the basic directory,
symlink and executable colours.

## Evidence

* `tests/qtest/cases/shell-prompt.toml`: folded path, red status, green branch, plain modes.
* `tests/qtest/cases/shell-completion.toml`: unique completion and the column menu.
* `tests/qtest/cases/shell-highlight.toml`: bold-red not-found command with `keep_ansi`, and a
  plain nested `NO_COLOR`/`RAVEN_PLAIN_PROMPT` session.
* `tests/qtest/cases/shell-alias.toml`: the three aliases and the non-interactive miss.
* `release-login`, `base-tools`: unchanged stable output.

## Donor reconciliation (DR-52)

| Topic | Illumos | BSD | Linux | Raven decision |
|---|---|---|---|---|
| Prompt layout | shell configuration, no system prompt requirement | BSD `/etc/profile` only; no bobthefish code | fish/bobthefish is user config; behaviour only | first-party prompt function with folded path, status and branch; no theme code copied |
| Completion menu | shell/readline behaviour only | NetBSD libedit menu completion; behaviour only | zsh/fish menu outcomes | use brush's existing completion engine and Vt menu; no donor code |
| Syntax colour | no shell syntax-colour contract | no copied code | zsh-syntax-highlighting outcome only | use brush's existing semantic highlighter and palette |
| Coloured ls | `ls` colour is user configuration | BSD `LSCOLORS`/`CLICOLOR` outcomes | GNU `--color=auto` outcome | interactive aliases plus `LS_COLORS`; `/bin/ls` stays script-safe |
| Line editor input | terminal/STDIN semantics; CDDL behaviour only | NetBSD `bin/ed` reads stdin; BSD-3-Clause reference | GNU ed reads stdin; outcome only | not part of K192; K186 owns the shipped editor's `tty:console` input |
