# Contributing to turboxml

Thanks for your interest in contributing to turboxml! This document provides guidelines and instructions for contributing.

## Getting Started

### Prerequisites

- Python 3.10+
- A stable [Rust toolchain](https://rustup.rs/)
- [Maturin](https://www.maturin.rs/) (`pip install maturin`)

### Development Setup

1. Fork and clone the repository:

   ```bash
   git clone https://github.com/<your-username>/turboxml.git
   cd turboxml
   ```

2. Create and activate a virtual environment:

   ```bash
   python -m venv .venv
   source .venv/bin/activate   # Linux/macOS
   .venv\Scripts\activate      # Windows
   ```

3. Install development dependencies:

   ```bash
   pip install maturin
   ```

4. Build the project in development mode:

   ```bash
   maturin develop
   ```

5. Run the tests:

   ```bash
   cargo test
   ```

## Making Changes

1. Create a new branch for your feature or fix:

   ```bash
   git checkout -b feature/your-feature-name
   ```

2. Make your changes. If you're modifying Rust code, rebuild with `maturin develop` before testing.

3. Ensure all tests pass:

   ```bash
   cargo test
   ```

4. Format your code:

   ```bash
   cargo fmt
   ```

5. Run the Rust linter:

   ```bash
   cargo clippy --all-targets -- -D warnings
   ```

## Submitting a Pull Request

1. Push your branch to your fork.
2. Open a pull request against the `main` branch.
3. Provide a clear description of your changes and the motivation behind them.
4. Ensure CI checks pass.

## CI and release builds

The workflow is [.github/workflows/ci.yml](.github/workflows/ci.yml).

- Normal CI runs for relevant code, test, packaging and CI configuration changes
  on `main` pushes and PRs targeting `main`. Documentation/benchmark-only PRs
  and `main` pushes skip it. Draft PR jobs wait until the PR is ready for review.
  GitHub evaluates PR path filters against the full PR diff, so a documentation
  update inside an existing code PR can still trigger CI.
- Normal runs check Rust formatting/lints/tests and install/test native wheels
  on CPython 3.12, CPython 3.15 and PyPy 3.12. Superseded PR/main runs are cancelled.
- Version tags matching `v[0-9]*` run the full platform wheel matrix and sdist,
  then publish after all builds and installed-wheel smoke tests pass. GitHub
  does not apply push path filters to tags.
- A manual run checks/tests only by default. Enable `build_wheels` for the full
  artifact matrix without publishing, or `publish` to build and publish.
  In-progress manual/tag release runs are not cancelled by newer CI runs.
- The PyPI Trusted Publisher must authorize the workflow filename `ci.yml`.
  A publisher still configured for `CI.yml` must be updated in PyPI's project
  Publishing settings; that account configuration is separate from this repo.
  See [PyPI's publisher troubleshooting](https://docs.pypi.org/trusted-publishers/troubleshooting/).

## Reporting Issues

If you encounter a bug or have a feature request, please [open an issue](https://github.com/RustedBytes/turboxml/issues/new) with as much detail as possible, including:

- Your Python and Rust versions
- Your operating system
- A minimal reproducible example (if applicable)

Clippy enables `pedantic` in `Cargo.toml` for local and CI runs. CI treats all
warnings as errors. Any lint exception must be scoped and explain its reason.
The Python 3.15 smoke job permits prereleases while 3.15 is in its release cycle.
