# Releasing Htrace

The `Check and publish` workflow tests Rust 1.97.0 and 1.99.0 with default,
minimal and all features. It also runs Clippy, builds documentation and checks
the distributable package. Pushes to `master` and pull requests only run checks.
A manual run also defaults to checks and a publish dry run, with no upload.

The crates.io package name remains `Htrace`; its Rust library target is `htrace`.
Keep `[package].name`, the release guard and Trusted Publishing tied to `Htrace`.
Changing only the registered name's case is rejected by crates.io.

## Before the first release with this workflow

Configure crates.io Trusted Publishing for `Htrace` with:

- owner: `hyultis`;
- repository: `rust_Htrace`;
- workflow: `publish.yml`;
- environment: `release`.

In GitHub repository settings, protect the `release` environment with a required
reviewer and restrict its deployment branches to the default branch (`master`).
Protect changes to the default branch and release workflow through branch rules.
These settings are external to the YAML and must be checked in GitHub/crates.io;
committing the workflow does not create or verify those protections.

The workflow uses a temporary crates.io token through
[Trusted Publishing](https://crates.io/docs/trusted-publishing), rather than a
stored registry token. Only the publish job has `id-token: write`; tests and
package verification run without that permission. The publish job checks out
the same commit SHA and uses the same locked dependencies. It uses `--no-verify`
because the package was already verified by the unprivileged job, avoiding
execution of dependency build scripts in the job with publishing credentials.

Actions are pinned to verified upstream commits:
[checkout v7.0.1](https://github.com/actions/checkout/releases/tag/v7.0.1) and
[crates-io-auth-action v1.0.6](https://github.com/rust-lang/crates-io-auth-action/releases/tag/v1.0.6).
The check jobs also use pinned
[dtolnay/rust-toolchain](https://github.com/dtolnay/rust-toolchain) and
[Swatinem/rust-cache](https://github.com/Swatinem/rust-cache/releases/tag/v2.9.3).
The toolchain action uses rustup's minimal profile; a requested compiler version
that is not installed still needs downloading. Dependency caches are separated
by job, compiler and test feature set. Only default-branch runs save caches;
pull requests can restore them but do not save them. Cargo binaries are excluded
from the cache. The privileged publish job restores no cache and keeps its
minimal rustup setup, without adding either third-party action to that job.
Dependabot checks action updates monthly. Review those updates and periodically
update the tested release toolchain; do not remove the MSRV job when doing so.

## Publish a version

1. Finish reviewing the release commit and its successful checks.
2. At release time, replace `Unreleased` in that version's changelog heading
   with the publication date (`YYYY-MM-DD`). Keep the Cargo version in sync.
3. Run `Check and publish` manually from the default branch, with `publish`
   enabled and `version` set to the exact Cargo version, for example `3.0.0`.
4. Review and approve the `release` environment deployment when prompted.

Publishing is refused for a missing/mismatched version or an undated changelog.
The upload job only runs in `hyultis/rust_Htrace` from its default branch after
all test and package jobs succeed. Tokens are acquired immediately before the
upload and revoked by the authentication action when the job completes.

## Package and supported Rust versions

The crate includes source code, licenses, the README, changelog and runtime
documentation. Tests, benchmark tooling/results, workflow files and internal
project notes stay in GitHub. Inspect `cargo package --list` before release.
Cargo also generates its standard package metadata and includes a lockfile.

Rust 1.97 is the supported minimum validated for this release. This is a tested
support floor, not a claim that every earlier compiler fails. A lower version
can be supported after it is tested; Rust 1.88 could not be installed in the
local preparation environment because network access was unavailable.

Local packaging can be checked with `cargo package --locked`.
`cargo publish --dry-run --locked` also checks registry-facing publication
requirements and may need network access even though it does not upload.
