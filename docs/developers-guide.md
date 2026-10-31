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

## Lints and Whitaker

`just lint` runs the Whitaker Dylint suite and then Clippy over the whole
workspace. Install the suite once with `whitaker-installer`; the `lint-whitaker`
recipe then runs `whitaker --all` with warnings denied (`DYLINT_RUSTFLAGS`). Set
`WHITAKER` to use a binary outside `PATH`. The `Whitaker` job in
`.github/workflows/ci.yml` runs the same command after the `install-whitaker`
action from shared-actions, which is the only way CI provisions the suite.

Whitaker builds its driver under its own pinned nightly toolchain, not the
repository's `1.91.0`, so the recipe and the job remove the development flags
(`RUSTFLAGS`, the Cranelift backend and similar) from its environment. The
repository's `.cargo/config.toml` still applies to the build, which is why the
job installs `mold` like the others.

### Filesystem access

Whitaker's `no_std_fs_operations` lint rejects `std::fs`. Library and binary
code reaches files through `cupcake_core::fs_access`, which opens a path's
parent as a `cap_std::fs::Dir` and acts on the file name beneath it. It is the
one module that calls `ambient_authority`, because every path Cupcake handles
comes from the command line, the environment or an agent's tool call. It
follows a symlink in the final path component itself, as `std::fs` does, so
script inspection still reads a script that a symlink points at.

The symlink resolver cannot be capability-scoped: it has to learn where an
arbitrary path really points, and `cap-std` refuses any symlink whose target is
absolute or leaves the opened directory. Its three probes (`canonicalize`,
`is_symlink` and `read_link`) live in `cupcake_core::ambient_fs`, the only
module `dylint.toml` exempts. Add nothing to it without the same justification.

Tests stage their fixtures through `fs_access` too, so a test that needs a new
operation adds it there rather than calling `std::fs`.

### Module size and structure

Whitaker also limits modules to 400 lines (`module_max_lines`), requires every
module to open with a `//!` comment, and flags functions with several clusters
of nested conditionals (`bumpy_road_function`), conditions with more than two
branches, and `expect` or panicking `unwrap_or_else` outside tests. Split a
module into sibling files, as `engine` and `builtins` are, rather than raising
the limit.
