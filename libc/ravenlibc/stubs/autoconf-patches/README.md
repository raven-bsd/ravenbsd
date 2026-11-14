<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Autoconf triplet snippets

`config.sub.snippet` and `config.guess.snippet` are the RavenBSD cases of ticket C66 5.E,
written from `donor-map/14-headers-abi.md` 3.A.9. They contain only case labels and a one-line
`uname` test; no text is copied from GNU `config.sub` or `config.guess` (GPL with an exception).

Until the cases are in the official GNU files, every port carries them in its own copy.

# owner: send config.sub/config.guess snippets upstream

The upstream submission to the GNU `config-patches` list is a **manual owner action**. It is
not automated and this repository does not patch a port's `config.sub`/`config.guess` at build
time.
