<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# S71 fault fixtures

The baseline kill/restart is driven from the release login shell through the existing `svcctl`
and `ps` utilities, so no target fixture binary is required for the resident servers (`netd` and
`vfsd`). A guest driver that SIGKILLs a server by pid (to exercise the on-failure restart path,
and to cover `blockd`/`fsd-ufs`) will be added here when a profile owns the staging; the
generation/reconnect scenario belongs to S70.
