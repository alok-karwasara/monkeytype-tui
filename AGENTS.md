# Project guidance

## Purpose

Build a fast, keyboard-first terminal typing practice app inspired by Monkeytype.
Prioritize accurate typing measurements, responsive input, readable presentation,
and reliable terminal cleanup.

## Confirmed product decisions

- Use an Instrument layout with visible test metrics and a words/code toggle.
- Preserve the user's terminal theme, default colors, and font. Do not require
  custom fonts, shell plugins, or a particular terminal setup. The terminal owns
  font selection; the app should not inspect user configuration to reproduce it.
- Offer 30-second, 60-second, and 2-minute tests. The initial default is undecided;
  the implementation provisionally selects words / 60 seconds, not a confirmed
  permanent product default.
- Code practice displays short formatted snippets and requires punctuation and
  spaces within lines. Skip indentation and trailing whitespace on each line.
  The initial implementation advances across line breaks automatically; do not
  skip internal code spaces.
- Word separators remain normal typing input. Keep display formatting separate
  from the scored input stream so ignored code formatting cannot affect metrics.
- Allow progress through errors, mark them visibly, and report errors/accuracy.
  Code tests end on time expiry or snippet completion, whichever comes first.
- Target Linux and macOS first. History is deferred:
  `// TODO: add saved aggregate history after the first release.`

## Design artifacts and maintenance

- Keep `system-design.md` aligned with accepted requirements, architecture, metric
  contracts, and validation plans. Distinguish proposals from confirmed behavior
  and planned files/tests from implemented ones.
- Update this `AGENTS.md` when durable project decisions change. Keep it concise;
  place detailed rationale and alternatives in `system-design.md`.
- Vizard HTML previews, feedback, and skill drafts belong in temporary scratch
  space under `/tmp`, not in this repository, unless explicitly requested here.
  The requested `system-design.md` is a repository document.
- Use shared Vizard choices and comments as design context. Browsing a preview
  is not selecting a preference; design feedback does not authorize app code.
- Application implementation is now authorized. Build the agreed first version
  with engine, rendering, and PTY tests; keep documentation aligned with actual
  implementation and verification status.
- Keep `test-cases.md` and prepared test fixtures aligned with the design.
  Preserve test IDs when implementing executable tests. Label cases that depend
  on proposed behavior; fixture validation is not evidence that the app passes.

## Current implementation and commands

- One Cargo package: `src/engine` owns deterministic ASCII prompt/scoring logic;
  `src/app.rs` owns screen transitions; input, UI, terminal, and runtime are
  separate modules. Bundled exercises are original project text in `content.rs`.
- Run `cargo run --locked -- --mode code --duration 60`; use `--help` for usage.
  F2/F3 change mode/duration from ready; Esc opens settings or abandon confirmation.
- `cargo test --locked` runs engine, input, transition, and rendering tests.
- `cargo test --locked --features test-support --test pty` runs fast real-process
  scenarios; append `-- --slow` for the separate real 30-second timeout check.
- `test-support` enables a fixture binary and failure injection for test use.
  The production executable has no hidden fixture or timer-bypass arguments.
- PTY workers must clear environment inheritance before constructing a
  `portable-pty` command builder, which otherwise enumerates ambient variables.
  Use explicit nonsecret TERM/LANG/PATH/SHELL values; never inspect user values.
- Preserve ASCII input validation, 1 MiB prompt bounds, a 40×12 minimum layout,
  and deadline precedence. Recheck actual terminal dimensions before input;
  resize events can lag behind the renderer's observed dimensions.
- Linux automation is available here. Do not claim macOS or real-emulator
  theme/font validation without running those checks.
- Current Linux evidence: 25 Rust test functions (including 56 fixture examples),
  all 14 PTY scenario IDs including the real 30-second timeout, formatting, and
  Clippy with denied warnings pass. This is not 98 independently passing tests;
  manual/platform coverage in the catalog remains broader.

## Privacy boundary

- Never read, search, source, print, or modify environment files or secrets.
  This includes `.env`, `.env.*`, credentials, tokens, private keys, and secret
  stores, including `.aws`, regardless of whether tooling permits access.
- Never dump process environments or use commands such as `env`, `printenv`, or
  shell tracing that could disclose secret values.
- Scope searches to known source and documentation paths. Exclude secret paths
  from file discovery. If a task needs configuration, use documented placeholder
  values and ask for nonsecret requirements instead of inspecting secrets.
- Keep the app offline by default. Do not collect typed text, telemetry, or
  credentials. Persist aggregate results only when explicitly part of a feature.

## Distribution

- Ship compiled GitHub Release archives, not source-only installation. End users
  need no Rust toolchain. Target Linux x86_64/ARM64 static musl and macOS
  Intel/Apple silicon; local GNU packaging does not verify those other targets.
- Rust/Cargo and the linker are build-time tools on developer/CI machines. Cargo
  produces a native executable with the Rust library code linked in; installation
  only copies that executable and documents, and running it invokes no compiler,
  Cargo, interpreter, or separately installed Rust runtime.
- No Rust requirement does not mean no OS requirements: use an archive matching
  OS/CPU and a supported terminal/OS version. Keep Linux static-musl packaging
  distinct from GNU builds and macOS system-library dependencies; do not claim
  that one executable runs on every platform or that static linkage was verified
  merely because a musl target was selected.
- `.github/workflows/release.yml` tests/packages on native runners. Matching
  version tags create drafts only; manual runs produce Actions artifacts.
  Publishing, pushing, tagging, and choosing a project license are separate steps.
- Package only the production binary without optional features, offline
  `packaging/install.sh`, README, target marker, and dependency notices/license.
  Never ship fixture-app, test-support, raw Cargo metadata, or arbitrary tree files.
- The installer defaults to ~/.local/bin and installs notices under ~/.local/share.
  No sudo, downloads, or shell configuration inspection/editing. Existing binaries
  require --force; symbolic links are never replaced. Explain PATH setup explicitly.
- Run `python3 tests/packaging.py` and its `--archive PATH` smoke check for package
  changes. See `packaging/RELEASING.md`; project license, dependency advisory/license
  review, and full platform verification remain prerequisites for public release.
- Current local packaging evidence: PKG01–06 pass, and a GNU Linux x86_64 archive
  passes PKG07 including installed-binary typing/quit and terminal restoration.
  The GitHub workflow and musl/macOS/ARM64 builds have not run here.

## Stack and dependency policy

- Use stable Rust, edition 2024, with Ratatui and its compatible Crossterm backend.
  Use stable releases, never nightly features, prereleases, or Git dependencies
  without a concrete requirement and explicit discussion.
- Before initial scaffolding or upgrades, verify current stable versions and
  compatibility in official Rust, Ratatui, and crate documentation. Do not assume
  versions recorded in instructions remain current.
- Pin the chosen Rust toolchain in `rust-toolchain.toml`, declare `rust-version`
  in `Cargo.toml`, and commit `Cargo.lock` for reproducible application builds.
- Keep dependencies small and justified. Start with a synchronous event loop;
  add async runtimes or background workers only for demonstrated requirements.
- Forbid unsafe code in this application's crates. Handle recoverable failures
  with `Result`; avoid `unwrap` and `expect` in production paths.
- Check dependency advisories and licenses before shipping; do not claim that a
  memory-safe language alone guarantees security.

## Architecture

- Separate the deterministic typing engine, application state and transitions,
  terminal event handling, rendering, and optional persistence.
- Model screens and test phases explicitly with enums. Route events through a
  single state owner; render from state without changing typing logic.
- Use monotonic time (`std::time::Instant`) for elapsed measurements. Inject
  elapsed time into engine updates so tests need no real sleeps.
- Start the timer on the first accepted typing input. Specify and test WPM,
  accuracy, correction, and completion semantics before displaying statistics.
  Standard WPM uses five characters per word; label raw and corrected metrics.
- Define Unicode handling explicitly. Never treat byte offsets as character
  positions; account for grapheme boundaries and terminal display width when
  supporting Unicode text.
- Distinguish key presses, releases, and repeats. Define paste handling explicitly
  so pasted text cannot silently count as normal typing performance.
- Poll input with a bounded timeout and redraw on changes or scheduled timer
  updates. Avoid busy loops and unnecessary full-rate rendering.
- Restore raw mode, cursor visibility, and alternate-screen state on normal exit,
  errors, and panic using the supported terminal lifecycle APIs. Avoid aborting
  panics, which bypass cleanup.

## Interface design

- Keep the typing prompt central, with restrained status information and
  discoverable keyboard shortcuts. Avoid distracting animation while typing.
- Provide visible focus and distinguish correct, incorrect, and pending input
  with cues beyond color alone. Respect terminal themes and limited color support.
- Use adaptive layouts and an explicit small-terminal fallback. Handle resizing
  without losing test state or indexing outside layout bounds.
- Keep exit, restart, and help behavior consistent across screens. Destructive
  actions such as clearing saved history require deliberate user interaction.
- Use original branding and bundled text with a suitable license.

## Verification and workflow

- Implement in small, reviewable steps. Keep documentation aligned with actual
  behavior; do not describe planned features as implemented.
- Test the typing engine and state transitions, especially correction, timing
  boundaries, empty input, completion, and supported Unicode behavior.
- Use Ratatui's test backend for meaningful rendering checks, including small
  terminals and resize states. Manually verify interactive input and cleanup in
  a real terminal when available; report when manual verification is unavailable.
- Include PTY end-to-end tests that launch the actual binary, send keystrokes,
  resize the terminal, and check results and terminal cleanup. Keep fast timing
  boundary checks deterministic; do not add hidden production timer bypasses.
- Run relevant checks on Linux and macOS when available. Report skipped checks,
  proposed benchmarks, and unimplemented tests honestly.
- Once Rust code exists, run `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`, and
  `cargo test --locked` for relevant changes. Explain any checks not run.
- Do not read secrets to troubleshoot build failures. Request only the minimum
  nonsecret information needed.

## Official references

- Rust releases: https://blog.rust-lang.org/releases/
- Rust toolchains: https://rust-lang.github.io/rustup/concepts/toolchains.html
- Ratatui installation and backend compatibility: https://ratatui.rs/installation/
- Ratatui documentation: https://docs.rs/ratatui/latest/ratatui/
- Crossterm documentation: https://docs.rs/crossterm/latest/crossterm/
