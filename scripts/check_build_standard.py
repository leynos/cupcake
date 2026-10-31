#!/usr/bin/env python3
"""Check the repository's Rust build standard contracts.

The build standard (concordat rule ``rust-build-defaults``) makes the ``mold``
linker the Linux default through ``.cargo/config.toml``. The pin is stable, so
the nightly-only parallel frontend and Cranelift are out. Three things keep
the standard honest, and each is checked here so that removing or inverting
one fails CI:

* the Cargo configuration names ``mold`` in a Linux-only target table and
  nowhere else;
* every workflow job that compiles Rust on Linux runs on a pinned Ubuntu image
  and installs the pinned ``mold`` package before it compiles;
* the builds that ship artefacts (the release workflow and both Nix builds)
  assign an empty ``RUSTFLAGS``, which displaces the configured linker flag.

Run ``python scripts/check_build_standard.py`` from the repository root.
"""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]
MOLD_FLAG = "-Clink-arg=-fuse-ld=mold"
LINUX_TABLE = 'cfg(target_os = "linux")'
MOLD_PACKAGE = "mold=2.30.0+dfsg-1build1"
PINNED_IMAGE = "ubuntu-24.04"
COMPILES = re.compile(
    r"\b(cargo|cross)\s+(build|check|clippy|test|nextest|doc|run|llvm-cov|bench)\b|\bmaturin\b"
)


def config_problems(config: dict) -> list[str]:
    """Return the complaints about a parsed ``.cargo/config.toml``."""
    problems = []
    targets = config.get("target", {})
    linux = targets.get(LINUX_TABLE, {})
    if MOLD_FLAG not in linux.get("rustflags", []):
        problems.append(f"the [target.'{LINUX_TABLE}'] table must carry {MOLD_FLAG}")
    sources = {"[build]": config.get("build", {}).get("rustflags", [])}
    sources.update({
        f"[target.'{name}']": table.get("rustflags", [])
        for name, table in targets.items()
        if name != LINUX_TABLE
    })
    problems.extend(
        f"{name} must not name mold, which ships for Linux only"
        for name, flags in sources.items()
        if any("mold" in str(flag) for flag in flags)
    )
    if "unstable" in config:
        problems.append("a stable pin cannot read an [unstable] table")
    return problems


def _steps(job: dict) -> list[dict]:
    """Return a job's steps."""
    return list(job.get("steps", []))


def _labels(job: dict) -> list[str]:
    """Return every runner label a job can run on."""
    runs_on = job.get("runs-on", "")
    if "${{" not in str(runs_on):
        return [str(runs_on)]
    matrix = job.get("strategy", {}).get("matrix", {})
    labels = [str(item) for item in matrix.get("os", [])]
    labels += [
        str(entry["os"]) for entry in matrix.get("include", []) if "os" in entry
    ]
    return labels


def _compiles_rust(job: dict) -> bool:
    """Return whether a job compiles Rust (a formatter or a probe does not)."""
    return any(COMPILES.search(str(step.get("run", ""))) for step in _steps(job))


def job_problems(workflow: str, name: str, job: dict, workflow_env: dict) -> list[str]:
    """Return the complaints about one job of a parsed workflow."""
    labels = [label for label in _labels(job) if "ubuntu" in label]
    if not labels or not _compiles_rust(job):
        return []
    env = {**workflow_env, **job.get("env", {})}
    where = f"{workflow}: job {name}"
    if env.get("RUSTFLAGS") == "":
        return []  # a shipped artefact stays on the platform linker
    problems = [
        f"{where} runs on the floating label {label}; pin {PINNED_IMAGE}"
        for label in labels
        if label != PINNED_IMAGE
    ]
    steps = _steps(job)
    install = next(
        (i for i, step in enumerate(steps) if step.get("name") == "Install mold"), None
    )
    first_compile = next(
        (i for i, step in enumerate(steps) if COMPILES.search(str(step.get("run", "")))),
        0,
    )
    if install is None:
        problems.append(f"{where} compiles Rust on Linux without an Install mold step")
    else:
        step = steps[install]
        if MOLD_PACKAGE not in str(step.get("run", "")):
            problems.append(f"{where}: the Install mold step must install {MOLD_PACKAGE}")
        if str(step.get("if", "")).replace(" ", "") != "runner.os=='Linux'":
            problems.append(f"{where}: the Install mold step must run on Linux only")
        if install > first_compile:
            problems.append(f"{where}: mold must be installed before the first compile")
    return problems


def workflow_problems(directory: Path) -> list[str]:
    """Return the complaints about every workflow under *directory*."""
    problems = []
    for path in sorted(directory.glob("*.yml")):
        document = yaml.safe_load(path.read_text("utf-8"))
        workflow_env = document.get("env", {}) or {}
        for name, job in (document.get("jobs", {}) or {}).items():
            problems.extend(job_problems(path.name, name, job, workflow_env))
    return problems


def release_problems(directory: Path) -> list[str]:
    """Return the complaints about the release build's empty ``RUSTFLAGS``."""
    document = yaml.safe_load((directory / "release.yml").read_text("utf-8"))
    env = document["jobs"]["build"].get("env", {}) or {}
    if env.get("RUSTFLAGS") != "":
        return ["release.yml: the build job must assign RUSTFLAGS: \"\""]
    return []


def flake_problems(flake: str) -> list[str]:
    """Return the complaints about the Nix builds, which ship artefacts."""
    assigned = len(re.findall(r'(?m)^\s*RUSTFLAGS\s*=\s*"";', flake))
    if assigned < 2:
        return [
            "flake.nix must assign RUSTFLAGS = \"\"; for the cupcake-cli and "
            f"cupcake-py builds (found {assigned})"
        ]
    return []


def all_problems(root: Path) -> list[str]:
    """Return every complaint about the repository at *root*."""
    config = tomllib.loads((root / ".cargo" / "config.toml").read_text("utf-8"))
    workflows = root / ".github" / "workflows"
    return [
        *config_problems(config),
        *workflow_problems(workflows),
        *release_problems(workflows),
        *flake_problems((root / "flake.nix").read_text("utf-8")),
    ]


def main() -> int:
    """Print every complaint and return a failing status when there is one."""
    problems = all_problems(ROOT)
    for problem in problems:
        print(problem, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
