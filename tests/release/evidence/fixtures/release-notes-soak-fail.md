<!-- SPDX-License-Identifier: BSD-3-Clause -->
<!-- Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors -->

# Release notes (fixture, soak failure)

| Area | Current evidence | Status |
|---|---|---|
| KVM/TCG boot soak | KVM `-smp 1` **20/20**, KVM `-smp 2` **20/20**, KVM `-smp 4` **50/50** in the latest S107 round-5 campaign. TCG `-smp 4` is **4/10** in that same campaign: run 5 reached `svcd: all services up` but printed no login banner before the 30 s timeout. S126 independently hit a TCG4 service-readiness timeout (`nulld`/`ramfsd` readiness timeout). | **BLOCKED** |
