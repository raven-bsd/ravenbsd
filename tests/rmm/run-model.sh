#!/bin/sh
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#
# RM1 bounded long adversarial model run. The model itself is the integration test
# crates/raven-rmm/tests/rmm_model.rs; this wrapper only fixes the corpus size and the cap.

set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd -P)
cd "$root"

ops=${RMM_MODEL_OPS:-50000}
echo "rmm-model: ops_per_seed=$ops corpora=3 seeds x 2 bases x x86/aarch64"
exec timeout 30 env RMM_MODEL_OPS="$ops" \
    cargo nextest run -p raven-rmm --test rmm_model --no-capture
