# Developers' guide

This guide records conventions for people working on Cupcake's Rust workspace.
User-facing documentation lives in the site under `docs/docs`.

## The build standard

Development, test, lint and typecheck builds link with the `mold` linker on
Linux. This is a default in `.cargo/config.toml`, which Cargo discovers on its
own, so a bare `cargo build` gets it. The default compiler driver must be GCC
12.1 or newer, or clang: older GCC drivers reject `-fuse-ld=mold` even when
`mold` is installed, and a host with one sets `-Clinker=clang`. `mold` ships
for Linux only, so the flag lives in a Linux-only target table, and macOS and
Windows keep their platform linker. Cargo selects one `rustflags` source rather
than merging them, so a `[build] rustflags` entry would have to repeat every
flag the Linux table names; the file has none.

On Linux, install `mold` before building. The configuration names it, so a
build without it fails at link time. CI installs it through the "Install mold"
steps described below.

The repository pins the stable toolchain `1.91.0` in `rust-toolchain.toml`, so
the nightly-only parallel frontend flag (`-Zthreads=8`) is not used.

Release builds are excluded on purpose. The `build` job in
`.github/workflows/release.yml` sets `RUSTFLAGS` to an empty value, which
displaces the mold table: a shipped artefact stays on the platform linker, and
the `cross` containers do not carry `mold`. The two Nix builds in `flake.nix`
(`cupcake-cli` and `cupcake-py`) ship artefacts too and run in a sandbox without
`mold`, so each assigns `RUSTFLAGS = ""` for the same reason; the `nix` job in
`ci-full.yml` builds `cupcake-cli` on pushes to `main`.

### Build standard contract

`scripts/check_build_standard.py` holds the contracts, and
`scripts/test_check_build_standard.py` proves each clause bites. The
`Build standard contract` job in `.github/workflows/ci.yml` runs both on every
pull request that touches Rust or the workflows. They check that:

- the Cargo configuration names `mold` in the Linux-only target table and
  nowhere else, and carries no `[unstable]` table, which stable Cargo cannot
  read;
- every Linux job that compiles Rust runs on `ubuntu-24.04`, the image the
  pinned package belongs to, and runs the Linux-only "Install mold" step at
  that package version before the first compile;
- the release `build` job and both Nix builds assign an empty `RUSTFLAGS`;
- on a Linux host, a Cargo build of a one-file crate under this configuration
  passes `-C link-arg=-fuse-ld=mold` to rustc, and the same build with an empty
  `RUSTFLAGS` does not.

Each mutation of the configuration or the workflows (removing the flag, the
install step or the empty `RUSTFLAGS`, floating the runner label, unpinning the
package, or moving the install after the first compile) fails a test.

### Cranelift

Exception: Cranelift is not the development-profile backend. Cranelift requires
a nightly toolchain, and this repository pins the stable `1.91.0` (recorded
2026-09-29, with no measurement taken on it). Revisit if the repository moves
to a nightly pin; leynos/cupcake#1 asks for that decision.

### Installing mold in CI

Exception: the estate installs `mold` through the `install-mold` input of
shared-actions' `setup-rust`, but these workflows use `dtolnay/rust-toolchain`
instead. Every Linux job that compiles Rust therefore runs on `ubuntu-24.04`
(not a floating label) and runs an "Install mold" step that installs that
image's package at the pinned version `mold=2.30.0+dfsg-1build1`. Move the
workflows onto `setup-rust` with `install-mold: 'true'`, and drop these steps,
when leynos/cupcake#1 settles the toolchain.

## Integration tests

Integration tests under `cupcake-core/tests` and `cupcake-cli/tests` touch the
filesystem through `cupcake_core::fs_access` (cap-std directory handles), not
`std::fs`, so the Whitaker ambient-filesystem lint stays clean. Use
`fs_access::write`, `read_to_string`, `create_dir_all`, `read_dir`, `unix_mode`
and `set_unix_mode` for fixtures; `read_dir` returns paths, and a test that
filters them by name must skip names that are not valid UTF-8 instead of
panicking.

Shared helpers sit in `cupcake-core/tests/common` and are imported by path
(`#[path = "common/project.rs"] mod project;`) so each test binary compiles only
the helpers it names:

- `common/logging.rs`: `init_test_logging` starts test logging once.
- `common/project.rs`: `create_test_project_for_harness` lays out a project
  for one harness.
- `common/global_config.rs`: `create_test_global_config` writes a global
  configuration with test policies.

A test file that outgrows the 400-line module limit moves its fixture and
scenarios into a directory named after the file, as
`global_builtins_integration_test/support.rs` (`setup_test_global_config`) and
`global_builtins_integration_test/signal_tests.rs` do. Helpers that set up
fixtures, spawn processes or look up external tools return `Result` and leave
the `expect` to the test body that calls them; assertion helpers such as
`verify_routing` in `claude_code_routing_test.rs` may `expect`, because a
failed assertion there is the test failing.
