#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""Resolve product data from the management tree.

One policy for every consumer: `RAVEN_MGMT_ROOT` when set, otherwise
`../ravenbsd-prjmgmt` relative to the product checkout. A missing file is a hard
error naming `RAVEN_MGMT_ROOT`; there is no fallback to the frozen product docs.
"""

from __future__ import annotations

import os
from pathlib import Path

DEFAULT_ROOT = "ravenbsd-prjmgmt"


def resolve_root(product_root: Path, env_value: str | None = None) -> Path:
    """Return the management root.

    `env_value=None` reads `RAVEN_MGMT_ROOT`; an empty string means "unset" so
    tests can exercise the default without mutating the process environment.
    """
    raw = os.environ.get("RAVEN_MGMT_ROOT") if env_value is None else env_value
    if raw:
        return Path(raw).expanduser().resolve()
    return (product_root.parent / DEFAULT_ROOT).resolve()


def require_root(product_root: Path, env_value: str | None = None) -> Path:
    """Return the management root, or exit naming RAVEN_MGMT_ROOT."""
    root = resolve_root(product_root, env_value)
    if not root.is_dir():
        raise SystemExit(
            f"release-evidence: RAVEN_MGMT_ROOT: management root {root} is not a directory; "
            "set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout "
            "(no product docs fallback)"
        )
    return root


def require_file(product_root: Path, relative: str, env_value: str | None = None) -> Path:
    """Return `relative` under the management root, or exit naming RAVEN_MGMT_ROOT."""
    root = require_root(product_root, env_value)
    path = root / relative
    if not path.is_file():
        raise SystemExit(
            f"release-evidence: management copy missing: {path}; "
            "set RAVEN_MGMT_ROOT to the ravenbsd-prjmgmt checkout "
            "(no product docs fallback)"
        )
    return path


def display(product_root: Path, path: Path) -> str:
    """Render a stable path: product-relative, else `ravenbsd-prjmgmt/...`, else absolute."""
    try:
        return str(path.relative_to(product_root))
    except ValueError:
        pass
    try:
        return str(Path("ravenbsd-prjmgmt") / path.relative_to(resolve_root(product_root)))
    except ValueError:
        return str(path)
