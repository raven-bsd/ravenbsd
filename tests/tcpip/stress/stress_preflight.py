#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
"""K525 stress preflight: one cached image, no build inside the measured window.

The K515 stress runs could not finish because the qtest rebuilt the image inside its own
cap. This runner separates the two costs:

  * preflight -- verify every input by name, then build the tcp-soak image once and cache
    it under build/qtest/stress-cache/<key>/, keyed by the tree hash plus the ssh and
    fixture digests;
  * measured run -- link the cached image to build/qtest/<case>.img and run the qtest
    with --no-build, so the cycle budget contains only QEMU.

The existing cases are the input. A 1-cycle smoke is rendered into build/ from the
unchanged 8-cycle case; the register under tests/qtest/cases/ is never edited.

Entry point:

    sh scripts/tcp-evidence.sh --stress --cycles 1 --mode kvm2
    python3 tests/tcpip/stress/stress_preflight.py --dry-run --mode kvm2
    python3 tests/tcpip/stress/stress_preflight.py --selftest
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PROFILE = "tcp-soak"
ARCH = "x86_64"
BOOT_CMDLINE = "console=ttyS0 loglevel=info test-exit=1"
CASE_DIR = ROOT / "tests/qtest/cases"
OUT_DIR = ROOT / "build/qtest"
CACHE_ROOT = OUT_DIR / "stress-cache"
CASE_IMG = "tcp-soak.img"
KEY_VERSION = "k525-v1"
MODES = {
    "kvm2": "tcp-soak-kvm2",
    "kvm4": "tcp-soak-kvm4",
    "tcg4": "tcp-soak-tcg4",
}
# The measured row is release-grade; refuse to start it without the release ssh add-on.
SSH_INPUT = Path(os.environ.get("STRESS_SSH_BIN", ROOT / "build/ssh/x86_64/bin/ssh"))
PROFILE_FILES = [
    ROOT / "profiles/tcp-soak.toml",
    ROOT / "profiles/shell.toml",
]
PROFILE_FILES += sorted((ROOT / "profiles/tcp-soak.d").glob("*.toml"))
PROFILE_FILES += sorted((ROOT / "profiles/shell.d").glob("*.toml"))


class PreflightError(Exception):
    """A missing or unusable preflight input; reported by name, never retried."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def compose_key(tree: str, dirty: bool, ssh_hash: str, fixture_hash: str) -> str:
    """The cache key: tree hash plus the ssh/fixture inputs, never the mode.

    All three modes use the same tcp-soak image, so the mode must not split the cache.
    """
    payload = "\n".join(
        [
            KEY_VERSION,
            f"profile={PROFILE}",
            f"arch={ARCH}",
            f"tree={tree}",
            f"dirty={1 if dirty else 0}",
            f"ssh={ssh_hash}",
            f"fixtures={fixture_hash}",
        ]
    )
    return hashlib.sha256(payload.encode()).hexdigest()[:32]


def git_tree() -> tuple[str, bool]:
    try:
        head = subprocess.run(
            ["git", "rev-parse", "HEAD"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        status = subprocess.run(
            ["git", "status", "--porcelain"],
            cwd=ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
    except (OSError, subprocess.CalledProcessError) as error:
        raise PreflightError(f"cannot read the git tree: {error}") from error
    return head, bool(status)


def declared_file_sources() -> list[str]:
    """Every non-generated initfs file source in the tcp-soak profile set."""
    sources: list[str] = []
    for path in PROFILE_FILES:
        if not path.is_file():
            continue
        for block in path.read_text().split("[[initfs.files]]")[1:]:
            block = block.split("[[", 1)[0]
            if re.search(r"(?m)^\s*generated\s*=\s*true\s*$", block):
                continue
            match = re.search(r'(?m)^\s*src\s*=\s*"([^"]+)"', block)
            if match:
                sources.append(match.group(1).replace("{arch}", ARCH))
    return sources


def fixture_hash() -> str:
    """Digest the profile manifests, their file sources and the three cases."""
    paths = [p for p in PROFILE_FILES if p.is_file()]
    for source in declared_file_sources():
        candidate = ROOT / source
        if candidate.is_file():
            paths.append(candidate)
    paths += [CASE_DIR / f"{name}.toml" for name in MODES.values()]
    lines = []
    for path in sorted({p.resolve() for p in paths if p.is_file()}):
        lines.append(f"{path.relative_to(ROOT).as_posix()}={sha256_file(path)}")
    return hashlib.sha256("\n".join(lines).encode()).hexdigest()


def cache_key() -> str:
    if not SSH_INPUT.is_file():
        raise PreflightError(f"missing ssh binary: {SSH_INPUT}")
    tree, dirty = git_tree()
    return compose_key(tree, dirty, sha256_file(SSH_INPUT), fixture_hash())


def require_clean_tree() -> None:
    _, dirty = git_tree()
    if dirty:
        raise PreflightError(
            "worktree is dirty; a release image must embed the kernel from a clean HEAD"
        )


def require_tool(name: str) -> str:
    found = shutil.which(name)
    if not found:
        raise PreflightError(f"missing tool: {name}")
    return found


def check_inputs(mode: str, ssh_input: Path | None = None) -> None:
    """Fail before any build or QEMU, naming the exact missing artefact."""
    ssh = ssh_input or SSH_INPUT
    require_tool("cargo")
    require_tool("git")
    require_tool("timeout")
    require_tool("qemu-system-x86_64")
    if not ssh.is_file():
        raise PreflightError(f"missing ssh binary: {ssh}")
    case = CASE_DIR / f"{MODES[mode]}.toml"
    if not case.is_file():
        raise PreflightError(f"missing stress case: {case}")
    if not (ROOT / f"profiles/{PROFILE}.toml").is_file():
        raise PreflightError(f"missing profile: {ROOT / f'profiles/{PROFILE}.toml'}")
    for source in declared_file_sources():
        path = ROOT / source
        if not path.is_file():
            raise PreflightError(f"missing fixture: {path}")


def cache_paths(cache_root: Path, key: str) -> tuple[Path, Path, Path]:
    directory = cache_root / key
    return directory, directory / "meta.json", directory / CASE_IMG


def cache_lookup(cache_root: Path, key: str) -> tuple[Path, dict] | None:
    _, meta_path, image = cache_paths(cache_root, key)
    if not meta_path.is_file() or not image.is_file():
        return None
    try:
        meta = json.loads(meta_path.read_text())
    except (OSError, ValueError):
        return None
    if meta.get("key") != key or meta.get("size") != image.stat().st_size:
        return None
    if meta.get("sha256") != sha256_file(image):
        return None
    return image, meta


def cache_decision(cache_root: Path, key: str, refresh: bool) -> tuple[Path, dict] | None:
    """A warm entry is used unless --refresh asks for a measured rebuild."""
    if refresh:
        return None
    return cache_lookup(cache_root, key)


def write_cache(cache_root: Path, key: str, image: Path, build_seconds: float) -> dict:
    directory, meta_path, cached = cache_paths(cache_root, key)
    directory.mkdir(parents=True, exist_ok=True)
    if image.resolve() != cached.resolve():
        shutil.copyfile(image, cached)
    meta = {
        "key": key,
        "profile": PROFILE,
        "arch": ARCH,
        "image": CASE_IMG,
        "size": cached.stat().st_size,
        "sha256": sha256_file(cached),
        "build_seconds": round(build_seconds, 3),
        "ssh": str(SSH_INPUT),
        "built_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    }
    meta_path.write_text(json.dumps(meta, indent=2, sort_keys=True) + "\n")
    return meta


def build_image(key: str) -> tuple[Path, float]:
    require_tool("cargo")
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    output = OUT_DIR / CASE_IMG
    if output.exists():
        output.unlink()
    command = [
        "cargo",
        "xtask",
        "image",
        "--arch",
        ARCH,
        "--profile",
        PROFILE,
        "--cmdline",
        BOOT_CMDLINE,
        "--output",
        str(output),
    ]
    started = time.monotonic()
    result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True)
    elapsed = time.monotonic() - started
    if result.returncode != 0 or not output.is_file() or output.stat().st_size == 0:
        tail = (result.stderr or result.stdout).strip().splitlines()[-5:]
        detail = " | ".join(tail) or "no output"
        raise PreflightError(f"image build failed (exit {result.returncode}): {detail}")
    return output, elapsed


def preflight(key: str, refresh: bool) -> tuple[Path, float, bool]:
    """Return (image, build_seconds, cached). Builds only on a cold cache or --refresh."""
    hit = cache_decision(CACHE_ROOT, key, refresh)
    if hit is not None:
        return hit[0], 0.0, True
    image, elapsed = build_image(key)
    write_cache(CACHE_ROOT, key, image, elapsed)
    return image, elapsed, False


def render_case(mode: str, cycles: int, out: Path) -> Path:
    """Render a bounded smoke case from the unchanged 8-cycle register case."""
    if cycles < 1 or cycles > 8:
        raise PreflightError(f"cycles must be 1..8: {cycles}")
    source = CASE_DIR / f"{MODES[mode]}.toml"
    text = source.read_text()
    workload = re.search(r"(/etc/tcp-soak-workload\.sh )8( 5 7000)", text)
    if not workload:
        raise PreflightError(f"case has no 8-cycle workload line: {source}")
    text = text[: workload.start()] + f"{workload.group(1)}{cycles}{workload.group(2)}" + text[workload.end() :]

    heartbeat = "wait = '^tcp-cycle-done: 8$'"
    if heartbeat not in text:
        raise PreflightError(f"case has no 8-cycle stall heartbeat: {source}")
    text = text.replace(heartbeat, f"wait = '^tcp-cycle-done: {cycles}$'", 1)

    start_marker = "[[expect]]\nregex = '^tcp-cycle: 1$'"
    end_marker = "[[expect]]\nregex = '^tcp-cycle-done: 8$'"
    start = text.index(start_marker)
    end = text.index(end_marker) + len(end_marker)
    block = "\n\n".join(
        f"[[expect]]\nregex = '^tcp-cycle: {index}$'\n\n"
        f"[[expect]]\nregex = '^tcp-cycle-done: {index}$'"
        for index in range(1, cycles + 1)
    )
    text = text[:start] + block + text[end:]

    if cycles < 8 and re.search(rf"\^tcp-cycle: {cycles + 1}\$", text):
        raise PreflightError(f"rendered case still expects cycle {cycles + 1}")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(text)
    return out


def link_image(image: Path, case: str) -> Path:
    target = OUT_DIR / f"{case}.img"
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        target.unlink()
    try:
        os.link(image, target)
    except OSError:
        shutil.copyfile(image, target)
    return target


def run_case(mode: str, cycles: int, timeout: int, refresh: bool = False) -> int:
    check_inputs(mode)
    require_clean_tree()
    key = cache_key()
    print(f"stress-preflight: mode={mode} case={MODES[mode]} cycles={cycles}")
    print(f"stress-preflight: key={key} cache={CACHE_ROOT / key}")
    image, build_seconds, cached = preflight(key, refresh=refresh)
    if cached:
        print(f"stress-preflight: build={build_seconds:.2f}s cached=yes image={image}")
    else:
        print(f"stress-preflight: build={build_seconds:.2f}s cached=no image={image}")

    generated = render_case(mode, cycles, OUT_DIR / "stress" / f"{MODES[mode]}-{cycles}.toml")
    linked = link_image(image, MODES[mode])
    serial = OUT_DIR / f"{MODES[mode]}.log"
    stamp = time.strftime("%Y%m%dT%H%M%SZ", time.gmtime())
    run_log = OUT_DIR / "stress" / f"{MODES[mode]}-{cycles}-{stamp}.stdout.log"
    run_log.parent.mkdir(parents=True, exist_ok=True)

    command = [
        "cargo",
        "xtask",
        "qtest",
        "--case",
        str(generated.relative_to(ROOT)),
        "--arch",
        ARCH,
        "--no-build",
    ]
    started = time.monotonic()
    try:
        result = subprocess.run(
            command,
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as error:
        elapsed = time.monotonic() - started
        run_log.write_text(f"{error.stdout or ''}\n{error.stderr or ''}")
        print(f"stress-preflight: cycle={elapsed:.2f}s exit=TIMEOUT result=timeout")
        return 124
    elapsed = time.monotonic() - started
    run_log.write_text((result.stdout or "") + (result.stderr or ""))
    verdict = "pass" if result.returncode == 0 else "fail"
    print(f"stress-preflight: image={linked} case={generated}")
    print(f"stress-preflight: cycle={elapsed:.2f}s exit={result.returncode} result={verdict}")
    print(f"stress-preflight: log={run_log}")
    print(f"stress-preflight: serial={serial}")
    if result.returncode != 0:
        tail = (result.stdout or "").strip().splitlines()[-12:]
        for line in tail:
            print(f"stress-preflight: {line}")
    return result.returncode


def selftest() -> int:
    import tempfile as temp

    with temp.TemporaryDirectory(prefix="k525-selftest-") as directory:
        base = Path(directory)
        key_a = compose_key("aaaa", False, "ssh1", "fix1")
        if key_a != compose_key("aaaa", False, "ssh1", "fix1"):
            print("selftest: key is not deterministic", file=sys.stderr)
            return 1
        mutations = {
            "tree": compose_key("bbbb", False, "ssh1", "fix1"),
            "dirty": compose_key("aaaa", True, "ssh1", "fix1"),
            "ssh": compose_key("aaaa", False, "ssh2", "fix1"),
            "fixtures": compose_key("aaaa", False, "ssh1", "fix2"),
        }
        for name, mutated in mutations.items():
            if mutated == key_a:
                print(f"selftest: {name} does not change the key", file=sys.stderr)
                return 1

        for cycles in (1, 3):
            out = base / f"case-{cycles}.toml"
            render_case("kvm2", cycles, out)
            text = out.read_text()
            if f"/etc/tcp-soak-workload.sh {cycles} 5 7000" not in text:
                print(f"selftest: workload not set to {cycles}", file=sys.stderr)
                return 1
            if f"wait = '^tcp-cycle-done: {cycles}$'" not in text:
                print(f"selftest: heartbeat not set to {cycles}", file=sys.stderr)
                return 1
            if len(re.findall(r"regex = '\^tcp-cycle: \d+\$'", text)) != cycles:
                print(f"selftest: expected {cycles} cycles", file=sys.stderr)
                return 1
            if re.search(rf"\^tcp-cycle: {cycles + 1}\$", text):
                print(f"selftest: leaked cycle {cycles + 1}", file=sys.stderr)
                return 1

        missing = base / "no-such-ssh"
        try:
            check_inputs("kvm2", ssh_input=missing)
        except PreflightError as error:
            if str(missing) not in str(error):
                print(f"selftest: error does not name {missing}: {error}", file=sys.stderr)
                return 1
        else:
            print("selftest: missing ssh was accepted", file=sys.stderr)
            return 1

        key = "selftest"
        if cache_lookup(base / "cache", key) is not None:
            print("selftest: empty cache is not a miss", file=sys.stderr)
            return 1
        image = base / "fake.img"
        image.write_bytes(b"k525")
        write_cache(base / "cache", key, image, 1.5)
        hit = cache_lookup(base / "cache", key)
        if hit is None or hit[0].read_bytes() != b"k525":
            print("selftest: warm cache is not a hit", file=sys.stderr)
            return 1
        if cache_decision(base / "cache", key, True) is not None:
            print("selftest: --refresh still used the cache", file=sys.stderr)
            return 1

    print("stress-preflight: selftest PASS")
    return 0


def selftest_cache(directory: str, mode: str) -> int:
    root = Path(directory)
    root.mkdir(parents=True, exist_ok=True)
    _ = mode
    key = "cache-proof"
    image = root / "fake.img"
    image.write_bytes(b"k525-cache")
    write_cache(root, key, image, 2.0)
    hit = cache_lookup(root, key)
    if hit is None:
        print("selftest-cache: warm entry is a miss", file=sys.stderr)
        return 1
    meta = hit[1]
    if meta["build_seconds"] != 2.0:
        print("selftest-cache: build time lost", file=sys.stderr)
        return 1
    print("stress-preflight: cache selftest PASS (hit, build=0.00s cached=yes)")
    return 0


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("--mode", choices=sorted(MODES), default="kvm2")
    parser.add_argument("--cycles", type=int)
    parser.add_argument("--refresh", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--key", action="store_true")
    parser.add_argument("--render-case", action="store_true")
    parser.add_argument("--out")
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--selftest-cache")
    parser.add_argument("--timeout", type=int)
    return parser.parse_args(argv)


def main(argv: list[str]) -> int:
    args = parse_args(argv)
    if args.selftest:
        return selftest()
    if args.selftest_cache:
        return selftest_cache(args.selftest_cache, args.mode)
    if args.key:
        print(cache_key())
        return 0
    if args.render_case:
        if args.cycles is None:
            print("stress-preflight: --render-case needs --cycles", file=sys.stderr)
            return 2
        out = Path(args.out) if args.out else OUT_DIR / "stress" / f"{MODES[args.mode]}-{args.cycles}.toml"
        try:
            print(render_case(args.mode, args.cycles, out))
        except PreflightError as error:
            print(f"stress-preflight: {error}", file=sys.stderr)
            return 2
        return 0
    if args.dry_run:
        try:
            check_inputs(args.mode)
            require_clean_tree()
            key = cache_key()
        except PreflightError as error:
            print(f"stress-preflight: {error}", file=sys.stderr)
            return 2
        state = "cached" if cache_lookup(CACHE_ROOT, key) else "needed"
        print(f"stress-preflight: key={key} build={state} cache={CACHE_ROOT / key}")
        return 0
    cycles = 1 if args.cycles is None else args.cycles
    timeout = args.timeout or 1200
    try:
        return run_case(args.mode, cycles, timeout, refresh=args.refresh)
    except PreflightError as error:
        print(f"stress-preflight: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
