# Binary release process

GitHub Releases will host a compiled executable for each supported CPU/OS.
Users install the matching archive without Cargo, a compiler, Python, or Git.
This is separate from publishing a crate on crates.io; `publish = false` stays.

## What the workflow does

`.github/workflows/release.yml` runs manually or on a pushed `v*` tag:

1. Build on native Linux x86_64/ARM64 and macOS Intel/Apple silicon runners.
2. Run formatting, Clippy, engine/UI tests, and fast PTY tests on each runner.
   Run the separate real 30-second timeout on Linux x86_64.
3. Build only `monkeytype-tui`, in release mode with no optional features.
   Never distribute `fixture-app` or enable `test-support` for a release build.
4. Package the executable, offline installer, README, target marker, and
   collected third-party dependency license texts. Include `LICENSE` when one
   exists. Produce an individual SHA-256 file alongside each archive.
5. Check archive contents, install into a temporary prefix, and launch the
   installed executable with `--version` and `--help` on its native runner.
   Launch that installed executable in a PTY, type a character, quit with Ctrl+C,
   and verify restored terminal settings.
6. Manual runs upload Actions artifacts only. A tag matching the Cargo version
   creates a **draft** GitHub release after all four jobs pass. A person reviews
   and publishes it later. Existing releases are not overwritten automatically.

Linux packages use static musl targets to avoid requiring a particular glibc
version. macOS builds target macOS 11 or newer, consistent with Rust target
support; actual minimum-version testing is still required. The installer checks
OS/CPU, but cannot prove kernel/macOS version compatibility. macOS packages are
currently unsigned and not notarized. No Windows package is planned for v1.

Workflow actions are pinned to verified release commits. The build jobs have
read-only repository permissions; only the draft creation job has contents write.
The publishing job uses GitHub's short-lived automatic token, not a personal
token or locally inspected credentials. Checkout does not persist credentials.
No environment dump, shell tracing, secret inspection, or shell-config scanning
is part of packaging. Only named assets enter the archive; source, Cargo metadata,
credentials, and the rest of the working tree are not archived. In particular,
raw metadata containing local build paths stays outside the package.

## Local packaging check

Build on the host for the host. For Linux GNU, for example:

```sh
cargo build --locked --release --no-default-features --bin monkeytype-tui --target x86_64-unknown-linux-gnu
cargo metadata --locked --format-version 1 --filter-platform x86_64-unknown-linux-gnu > target/release-metadata.json
python3 scripts/package-release.py --binary target/x86_64-unknown-linux-gnu/release/monkeytype-tui --target x86_64-unknown-linux-gnu --metadata target/release-metadata.json
python3 tests/packaging.py
python3 tests/packaging.py --archive target/packages/monkeytype-tui-0.1.0-x86_64-unknown-linux-gnu.tar.gz
```

Packaging tools require Python 3.11+ (provided by the chosen CI images); end users
only need their normal POSIX shell and archive/checksum tools. A GNU test package
requires the host's compatible glibc and is not the broadly portable musl asset.
`--target` labels a previously built binary; the packager is not a cross compiler.
The workflow always passes the matching Cargo output and executes it natively.

## Before the first public release

- Choose the project's distribution license and add its `LICENSE` text. No
  license has been chosen on the author's behalf. Collecting dependency notices
  is not a license compatibility or security advisory audit; finish those reviews.
- Run the workflow in the intended GitHub repository and inspect all four jobs.
  Local Linux success does not establish musl, ARM64, or macOS success.
- Complete the manual terminal/theme and supported-platform checks in the test
  catalog, and review provisional defaults and scoring.
- Confirm version in Cargo.toml/Cargo.lock and use the exact corresponding tag
  (currently `v0.1.0`). Push a tag only when ready to create the draft. No tag,
  push, draft, or public release is performed by local packaging commands.
- Review archive assets, notices, release notes, checksums, and installation
  instructions, then publish the draft in GitHub's UI.

Homebrew, `.deb`/`.rpm`, a network installer, signatures/attestations, and automatic
updates can be added later. Plain release archives and an offline installer keep
the initial distribution small. Cargo-dist is a viable alternative when more
installer formats justify generated release tooling; its default shell installer
can alter shell profiles, which this installer deliberately leaves to the user.
