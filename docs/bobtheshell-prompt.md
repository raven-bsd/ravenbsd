<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# bobtheshell prompt (K177)

`etc/brush/bobtheshell.sh` is the default interactive prompt for brush. It is a bash/brush
rewrite of the owner's zsh `bobtheshell` theme, not a copy: no `%F`/`%K`, no `vcs_info`, no
oh-my-zsh or oh-my-bash framework.

## Modes

| Mode | Selection | Prompt |
|---|---|---|
| stable plain | `RAVEN_PROMPT=plain` | `raven# `, no ANSI |
| no colour | `NO_COLOR` or `TERM=dumb` | `user@host:path# `, no ANSI |
| default | otherwise | user@host, shortened path, git, virtualenv, root, `!status`, jobs |
| powerline | `RAVEN_PROMPT_POWERLINE=1` | U+E0B0 separators; flat `|` otherwise |

The Pi 4 release loads the theme through `/etc/brush/brushrc`. The x86_64 release keeps the
profile-only plain `raven# ` marker; images that do not stage the theme remain unchanged.

## Segments

* user@host in Raven Violet (xterm 183), path in 231;
* shortened path (`~`, `~/basename`, or the current basename);
* git branch and `*` dirty marker only inside a work tree, with one `git status --porcelain`
  call per prompt; no `git cherry`;
* `VIRTUAL_ENV` basename;
* `#` for root, `$` otherwise;
* `!N` for the last non-zero status;
* `jobs:N` while background jobs exist.

The prompt is built in `PROMPT_COMMAND`; `RAVEN_PROMPT=plain` returns immediately to the stable
marker. No boot-log styling, history hints, Reedline backend or completion-engine change is in
K177.

## Test

`tests/qtest/cases/shell-prompt.toml` boots the shell-test image, enters a plain brush, then the
default theme, checks the root/path/error/git/powerline segments and exits. The boot-to-login
path is unchanged; the prompt setup is only sourced by the interactive login rc.
