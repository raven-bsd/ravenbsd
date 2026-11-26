#!/usr/bin/env python3
# SPDX-License-Identifier: BSD-3-Clause
# Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

"""Measure the release login cold start over a QEMU serial console.

One sample boots the release image, waits for `login:`, sends `root`, times the
`login:` -> `Password:` step, sends `raven`, times the `Password:` -> `raven# ` step,
then exits the guest cleanly. The wrapper is `scripts/login-cold-start.sh`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import os
import shutil
import signal
import statistics
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT_IMAGE = ROOT / "build" / "login-cold-start" / "release.img"
DEFAULT_OVMF_CODE = "/usr/share/OVMF/OVMF_CODE_4M.fd"
DEFAULT_OVMF_VARS = "/usr/share/OVMF/OVMF_VARS_4M.fd"
CHECK_LIMIT_MS = 1000.0


class HarnessError(Exception):
    """A configuration, image or host problem that stops the run."""


def utc_now() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def git_commit() -> str:
    result = subprocess.run(
        ["git", "rev-parse", "--short=8", "HEAD"],
        cwd=ROOT,
        check=True,
        text=True,
        capture_output=True,
    )
    return result.stdout.strip()


def kvm_usable() -> bool:
    path = Path("/dev/kvm")
    return path.exists() and os.access(path, os.R_OK | os.W_OK)


def load1() -> float:
    return round(os.getloadavg()[0], 2)


def run_checked(command: list[str], cwd: Path = ROOT) -> None:
    print("+ " + " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def prepare_image(image: Path, build: bool) -> Path:
    image = image if image.is_absolute() else ROOT / image
    if not build:
        if not image.is_file():
            raise HarnessError(f"image not found: {image}")
        return image
    stamp = image.with_suffix(image.suffix + ".commit")
    commit = git_commit()
    if image.is_file() and stamp.is_file() and stamp.read_text().strip() == commit:
        print(f"image: reusing {image} (commit {commit})", flush=True)
        return image
    image.parent.mkdir(parents=True, exist_ok=True)
    print(f"image: building release image at {image} (commit {commit})", flush=True)
    run_checked(["sh", "tests/release/check-loader.sh"])
    run_checked(
        [
            "cargo",
            "xtask",
            "image",
            "--arch",
            "x86_64",
            "--profile",
            "release",
            "--cmdline",
            "console=ttyS0 test-exit=1",
            "--output",
            str(image),
        ]
    )
    if not image.is_file():
        raise HarnessError(f"image build produced no file: {image}")
    stamp.write_text(commit + "\n")
    return image


def qemu_argv(accel: str, smp: int, image: Path, vars_path: Path) -> list[str]:
    code = Path(os.environ.get("RAVEN_OVMF_CODE", DEFAULT_OVMF_CODE))
    if not code.is_file():
        raise HarnessError(f"OVMF code not found: {code}")
    argv = [
        "qemu-system-x86_64",
        "-machine",
        "q35",
        "-m",
        "512M",
        "-smp",
        str(smp),
        "-serial",
        "stdio",
        "-display",
        "none",
        "-monitor",
        "none",
        "-no-reboot",
        "-drive",
        f"if=pflash,format=raw,readonly=on,file={code}",
        "-drive",
        f"if=pflash,format=raw,file={vars_path}",
        "-drive",
        f"format=raw,file={image}",
        "-device",
        "isa-debug-exit,iobase=0xf4,iosize=0x04",
        "-nic",
        "none",
        "-netdev",
        "user,id=n0",
        "-device",
        "virtio-net-pci,netdev=n0,disable-legacy=on",
    ]
    if accel == "kvm":
        argv.extend(["-enable-kvm", "-cpu", "host"])
    else:
        argv.extend(["-accel", "tcg"])
    return argv


def kill_child(child) -> None:
    try:
        if child.isalive():
            child.kill(signal.SIGKILL)
    except Exception:
        pass
    try:
        child.close(force=True)
    except Exception:
        pass


def run_sample(
    accel: str,
    smp: int,
    sample: int,
    image: Path,
    run_dir: Path,
    commit: str,
    boot_timeout: float,
    step_timeout: float,
) -> dict:
    try:
        import pexpect
    except ImportError as error:  # pragma: no cover - host dependency
        raise HarnessError("python3 pexpect is required") from error

    vars_source = Path(os.environ.get("RAVEN_OVMF_VARS", DEFAULT_OVMF_VARS))
    if not vars_source.is_file():
        raise HarnessError(f"OVMF vars not found: {vars_source}")
    vars_path = run_dir / f"vars-{accel}-smp{smp}-{sample}.fd"
    shutil.copyfile(vars_source, vars_path)
    log_path = run_dir / f"{accel}-smp{smp}-sample{sample}.log"

    argv = qemu_argv(accel, smp, image, vars_path)
    record = {
        "sample": sample,
        "accel": accel,
        "smp": smp,
        "commit": commit,
        "load1": load1(),
        "login_ms": None,
        "password_ms": None,
        "error": None,
        "log": str(log_path),
    }
    log_file = log_path.open("w", encoding="utf-8", errors="replace")
    child = None
    try:
        child = pexpect.spawn(
            argv[0],
            argv[1:],
            cwd=str(ROOT),
            encoding="utf-8",
            codec_errors="replace",
            echo=False,
            timeout=step_timeout,
            logfile=log_file,
        )
        child.expect(r"(?i)login:\s*", timeout=boot_timeout)
        login_start = time.monotonic()
        child.sendline("root")
        child.expect(r"(?i)password:\s*", timeout=step_timeout)
        password_at = time.monotonic()
        child.sendline("raven")
        child.expect(r"raven#\s", timeout=step_timeout)
        shell_at = time.monotonic()
        record["login_ms"] = round((password_at - login_start) * 1000.0, 1)
        record["password_ms"] = round((shell_at - password_at) * 1000.0, 1)
        try:
            child.sendline("exit 0")
            child.expect(r"init exited \(0\)", timeout=10)
        except Exception:
            # The measured sample is still valid; the final exit is only housekeeping.
            pass
    except Exception as error:
        record["error"] = str(error).splitlines()[0][:300]
    finally:
        if child is not None:
            kill_child(child)
        log_file.flush()
        log_file.close()
        vars_path.unlink(missing_ok=True)
    return record


def summary(values: list[float]) -> dict:
    ordered = sorted(values)
    count = len(ordered)
    if count == 0:
        return {"n": 0}
    p90_index = max(0, math.ceil(0.9 * count) - 1)
    return {
        "n": count,
        "min": round(ordered[0], 1),
        "median": round(statistics.median(ordered), 1),
        "p90": round(ordered[p90_index], 1),
        "max": round(ordered[-1], 1),
    }


def print_table(samples: list[dict]) -> None:
    print("sample\taccel\tsmp\tcommit\tload1\tlogin_ms\tpassword_ms\terror", flush=True)
    for sample in samples:
        print(
            "{sample}\t{accel}\t{smp}\t{commit}\t{load1}\t{login}\t{password}\t{error}".format(
                sample=sample["sample"],
                accel=sample["accel"],
                smp=sample["smp"],
                commit=sample["commit"],
                load1=sample["load1"],
                login="" if sample["login_ms"] is None else sample["login_ms"],
                password=""
                if sample["password_ms"] is None
                else sample["password_ms"],
                error=sample["error"] or "",
            ),
            flush=True,
        )
    print(flush=True)


def config_label(accel: str, smp: int) -> str:
    return f"{accel}-smp{smp}"


def summarize_samples(samples: list[dict]) -> dict:
    summaries = {}
    for accel, smp in sorted({(s["accel"], s["smp"]) for s in samples}):
        group = [s for s in samples if s["accel"] == accel and s["smp"] == smp]
        summaries[config_label(accel, smp)] = {
            "login_ms": summary(
                [s["login_ms"] for s in group if s["login_ms"] is not None]
            ),
            "password_ms": summary(
                [s["password_ms"] for s in group if s["password_ms"] is not None]
            ),
            "errors": sum(1 for s in group if s["error"] is not None),
        }
    return summaries


def print_summary(summaries: dict) -> None:
    for label, values in summaries.items():
        for step in ("login_ms", "password_ms"):
            metrics = values[step]
            if metrics["n"] == 0:
                print(f"summary {label} {step}: n=0", flush=True)
                continue
            print(
                "summary {label} {step}: n={n} min={min} median={median} "
                "p90={p90} max={max}".format(label=label, step=step, **metrics),
                flush=True,
            )
        if values["errors"]:
            print(f"summary {label}: errors={values['errors']}", flush=True)


def check_failures(summaries: dict, requested: int) -> list[str]:
    failures = []
    for label, values in summaries.items():
        for step in ("login_ms", "password_ms"):
            metrics = values[step]
            if metrics["n"] < requested:
                failures.append(f"{label} {step}: only {metrics['n']}/{requested} samples")
            if metrics["n"] and metrics["median"] > CHECK_LIMIT_MS:
                failures.append(
                    f"{label} {step}: median {metrics['median']} ms > {CHECK_LIMIT_MS:.0f} ms"
                )
            if metrics["n"] and metrics["p90"] > CHECK_LIMIT_MS:
                failures.append(
                    f"{label} {step}: p90 {metrics['p90']} ms > {CHECK_LIMIT_MS:.0f} ms"
                )
        if values["errors"]:
            failures.append(f"{label}: {values['errors']} failed sample(s)")
    return failures


def compare_baseline(path: Path, current: dict) -> None:
    if not path.is_file():
        return
    try:
        previous = json.loads(path.read_text())
    except (OSError, json.JSONDecodeError) as error:
        print(f"baseline: cannot read {path}: {error}", file=sys.stderr)
        return
    print(f"baseline: previous {path} from {previous.get('generated_utc', 'unknown')}")
    for label, values in current["summary"].items():
        prior = previous.get("summary", {}).get(label)
        if not prior:
            continue
        for step in ("login_ms", "password_ms"):
            old = prior.get(step, {})
            new = values.get(step, {})
            if old.get("median") is None or new.get("median") is None:
                continue
            delta = new["median"] - old["median"]
            print(
                f"baseline: {label} {step} median {old['median']} -> "
                f"{new['median']} ms ({delta:+.1f})"
            )


def parse_configs(args) -> list[tuple[str, int]]:
    if args.accel and args.smp:
        return [(args.accel, args.smp)]
    if args.accel == "kvm":
        return [("kvm", 2), ("kvm", 4)]
    if args.accel == "tcg":
        return [("tcg", args.smp or 4)]
    if args.smp:
        return [("kvm", args.smp), ("tcg", args.smp)]
    return [("kvm", 2), ("kvm", 4), ("tcg", 4)]


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--accel", choices=("kvm", "tcg"))
    parser.add_argument("--smp", type=int)
    parser.add_argument("--samples", type=int, default=20)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--baseline", type=Path)
    parser.add_argument("--image", type=Path, default=DEFAULT_IMAGE)
    parser.add_argument("--no-build", action="store_true")
    parser.add_argument("--boot-timeout", type=float, default=60.0)
    parser.add_argument("--step-timeout", type=float, default=15.0)
    parser.add_argument("--out", type=Path, default=ROOT / "build" / "login-cold-start")
    args = parser.parse_args(argv)

    if args.samples < 1:
        raise HarnessError("--samples must be at least 1")
    if args.smp is not None and args.smp < 1:
        raise HarnessError("--smp must be at least 1")

    configs = parse_configs(args)
    if any(accel == "kvm" for accel, _ in configs) and not kvm_usable():
        print("config kvm: refused ( /dev/kvm is not readable/writable )", flush=True)
        configs = [(accel, smp) for accel, smp in configs if accel != "kvm"]
        if not configs:
            raise HarnessError("KVM was requested but /dev/kvm is not usable")

    image = prepare_image(args.image, build=not args.no_build)
    image_hash = sha256_file(image)
    commit = git_commit()
    stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    run_dir = args.out / stamp
    run_dir.mkdir(parents=True, exist_ok=True)
    print(
        f"host: load1={load1()} nproc={os.cpu_count()} kvm={kvm_usable()} "
        f"commit={commit} image={image} sha256={image_hash}",
        flush=True,
    )

    samples = []
    for accel, smp in configs:
        print(f"config: accel={accel} smp={smp} samples={args.samples}", flush=True)
        for sample in range(1, args.samples + 1):
            record = run_sample(
                accel,
                smp,
                sample,
                image,
                run_dir,
                commit,
                args.boot_timeout,
                args.step_timeout,
            )
            samples.append(record)
            state = "ok" if record["error"] is None else "fail: " + record["error"]
            print(
                f"sample {sample}/{args.samples} {accel}-smp{smp}: "
                f"login_ms={record['login_ms']} password_ms={record['password_ms']} "
                f"load1={record['load1']} {state}",
                flush=True,
            )

    summaries = summarize_samples(samples)
    print_table(samples)
    print_summary(summaries)

    result = {
        "generated_utc": utc_now(),
        "commit": commit,
        "image": str(image),
        "image_sha256": image_hash,
        "host": {
            "load1": load1(),
            "nproc": os.cpu_count(),
            "kvm_usable": kvm_usable(),
        },
        "samples": samples,
        "summary": summaries,
    }
    if args.baseline:
        baseline = args.baseline if args.baseline.is_absolute() else ROOT / args.baseline
        compare_baseline(baseline, result)
        baseline.parent.mkdir(parents=True, exist_ok=True)
        baseline.write_text(json.dumps(result, indent=2) + "\n")
        print(f"baseline: wrote {baseline}", flush=True)

    failures = check_failures(summaries, args.samples)
    if failures:
        for failure in failures:
            print(f"check: FAIL {failure}", file=sys.stderr)
        return 1 if args.check else 0
    print("check: PASS (all medians and p90s <= 1000 ms)", flush=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1:]))
    except HarnessError as error:
        print(f"login-cold-start: {error}", file=sys.stderr)
        sys.exit(2)
