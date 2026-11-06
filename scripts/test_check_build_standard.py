"""Prove the build standard contracts hold, and that each clause bites.

Each mutation edits a private copy of the repository's own configuration the
way a later change could and asserts the clause meant to catch it does. The
Cargo-boundary tests run the pinned Cargo on a one-file binary crate that
carries the repository's Cargo configuration and read the rustc command line,
so the linker selection and the release override are shown on a real build.
"""

from __future__ import annotations

import copy
import sys
import tomllib
from pathlib import Path

import pytest
import yaml

sys.path.insert(0, str(Path(__file__).resolve().parent))

import check_build_standard as cbs  # noqa: E402

ROOT = cbs.ROOT


def _config() -> dict:
    """Return the repository's parsed Cargo configuration."""
    return tomllib.loads((ROOT / ".cargo" / "config.toml").read_text("utf-8"))


def _workflow(name: str) -> dict:
    """Return one parsed repository workflow."""
    return yaml.safe_load((ROOT / ".github" / "workflows" / name).read_text("utf-8"))


def test_the_repository_meets_every_contract() -> None:
    """The repository as it stands has no complaint."""
    assert cbs.all_problems(ROOT) == []


def test_the_configuration_must_name_mold_for_linux() -> None:
    """Removing the Linux flag is caught."""
    config = _config()
    config["target"][cbs.LINUX_TABLE]["rustflags"] = []
    assert any("must carry" in p for p in cbs.config_problems(config))


@pytest.mark.parametrize("table", ["build", "other"])
def test_the_configuration_must_not_name_mold_beyond_linux(table: str) -> None:
    """Naming mold in `[build]` or in a non-Linux table is caught."""
    config = _config()
    if table == "build":
        config.setdefault("build", {})["rustflags"] = [cbs.MOLD_FLAG]
    else:
        config["target"]["cfg(unix)"] = {"rustflags": [cbs.MOLD_FLAG]}
    assert any("ships for Linux only" in p for p in cbs.config_problems(config))


def test_a_stable_pin_cannot_carry_an_unstable_table() -> None:
    """An `[unstable]` table is caught, because stable Cargo cannot read it."""
    config = _config()
    config["unstable"] = {"codegen-backend": True}
    assert any("[unstable]" in p for p in cbs.config_problems(config))


def _ci_job() -> dict:
    """Return a private copy of the pull-request `clippy` job."""
    return copy.deepcopy(_workflow("ci.yml")["jobs"]["clippy"])


def test_a_linux_compile_job_without_install_mold_is_caught() -> None:
    """Deleting the Install mold step is caught."""
    job = _ci_job()
    job["steps"] = [s for s in job["steps"] if s.get("name") != "Install mold"]
    assert any("without an Install mold" in p for p in cbs.job_problems("ci.yml", "x", job, {}))


def test_a_floating_runner_label_is_caught() -> None:
    """A floating Ubuntu label next to the pinned package is caught."""
    job = _ci_job()
    job["runs-on"] = "ubuntu-latest"
    assert any("floating label" in p for p in cbs.job_problems("ci.yml", "x", job, {}))


def test_an_unpinned_package_is_caught() -> None:
    """Dropping the package version pin is caught."""
    job = _ci_job()
    for step in job["steps"]:
        if step.get("name") == "Install mold":
            step["run"] = step["run"].replace(cbs.MOLD_PACKAGE, "mold")
    assert any("must install" in p for p in cbs.job_problems("ci.yml", "x", job, {}))


def test_install_mold_must_precede_the_first_compile() -> None:
    """Moving the install after the toolchain step is caught."""
    job = _ci_job()
    job["steps"].append(job["steps"].pop(1))
    assert any("before the first compile" in p for p in cbs.job_problems("ci.yml", "x", job, {}))


def test_install_mold_must_be_linux_only() -> None:
    """Dropping the Linux condition is caught."""
    job = _ci_job()
    for step in job["steps"]:
        if step.get("name") == "Install mold":
            step.pop("if")
    assert any("Linux only" in p for p in cbs.job_problems("ci.yml", "x", job, {}))


def test_an_empty_rustflags_exempts_a_shipped_artefact_build() -> None:
    """A job that assigns an empty `RUSTFLAGS` needs no mold."""
    job = _ci_job()
    job["steps"] = [s for s in job["steps"] if s.get("name") != "Install mold"]
    assert cbs.job_problems("release.yml", "build", job, {"RUSTFLAGS": ""}) == []


def test_the_release_build_must_assign_an_empty_rustflags(tmp_path: Path) -> None:
    """Removing the empty `RUSTFLAGS` from the release build is caught."""
    directory = ROOT / ".github" / "workflows"
    assert cbs.release_problems(directory) == []
    document = yaml.safe_load((directory / "release.yml").read_text("utf-8"))
    document["jobs"]["build"]["env"].pop("RUSTFLAGS")
    (tmp_path / "release.yml").write_text(yaml.safe_dump(document), "utf-8")
    assert cbs.release_problems(tmp_path)


def test_the_nix_builds_must_assign_an_empty_rustflags() -> None:
    """Removing the empty `RUSTFLAGS` from either Nix build is caught."""
    flake = (ROOT / "flake.nix").read_text("utf-8")
    assert cbs.flake_problems(flake) == []
    assert cbs.flake_problems(flake.replace('RUSTFLAGS = "";', "", 1))
