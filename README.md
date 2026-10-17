# monkeytype-tui

Offline typing practice in your terminal. Switch between words and formatted
code, with live time, WPM, accuracy, and error counts. The app uses your terminal's
colors and font; no custom theme, font, shell plugin, or account is needed.

## Run

Install the pinned Rust toolchain through rustup and your platform's normal
compiler/linker tools, then:

```sh
cargo run --locked
cargo run --locked -- --mode code --duration 60
```

For an optimized build:

```sh
cargo build --release --locked
./target/release/monkeytype-tui --mode words --duration 30
```

The available durations are **30, 60, and 120 seconds**. The initial selection
is provisionally words / 60 seconds; settings last for this process only.
Run `cargo run --locked -- --help` for command-line usage.

## Controls

| Screen | Keys |
| --- | --- |
| Ready | F2 changes words/code; F3 cycles duration; Esc opens settings. |
| Settings | Up/Down or Tab selects a field; Left/Right changes it; Enter applies; Esc cancels. |
| Typing | Type normally; Backspace corrects; Esc asks whether to abandon. |
| Abandon prompt | Enter/y abandons; Esc/n resumes. The timer keeps running. |
| Results | Enter starts a fresh ready test; Esc opens settings. |
| Any screen | Ctrl+C quits. `^C` in the compact footer means Ctrl+C. |

Code indentation, trailing spaces, and line breaks are automatic. **Spaces inside
each line must be typed**, along with brackets, colons, and other punctuation.
The timer starts on the first accepted character, including a wrong character.
Code tests end at the time limit or when all required positions have been
attempted; mistakes do not prevent completion. Word mode replenishes its prompt
and continues until time expires.

Exercises currently use ASCII text. Original bundled word exercises and Rust,
Python, and JavaScript snippets rotate between tests; code is never executed.
The minimum layout is 40 columns × 12 rows. Below that size, input is ignored
and a resize message appears; an active timer continues.

Errors are underlined as well as colored using the terminal palette. Reported
bracketed paste is ignored during practice; terminals that send paste as ordinary
keystrokes cannot be distinguished reliably. Ctrl+Z does not suspend this version.

## Scoring

- Accuracy counts historically correct attempts divided by all character attempts.
  Correcting an error does not remove it from that history.
- WPM counts correct positions currently retained, using five characters per word.
- Raw WPM counts all accepted character attempts, including mistakes and retyping.
- Backspace and automatic formatting do not count as character attempts. Internal
  spaces do. A wrong space must be corrected rather than silently realigning text.
- Rates are unavailable before input or at zero elapsed time. Final metrics freeze
  when the test ends.

These are this app's documented initial formulas, not a claim of exact scoring
compatibility with Monkeytype. See [system-design.md](system-design.md) for the
full contracts and tradeoffs.

## Tests

Fast engine, input, application, and rendering tests:

```sh
cargo test --locked
```

Real-process PTY tests on Linux/macOS, including mode selection, typing,
corrections, resizing, paste, restart, and terminal cleanup:

```sh
cargo test --locked --features test-support --test pty
```

The real 30-second timeout is a separate slower test:

```sh
cargo test --locked --features test-support --test pty -- --slow
```

Run one scenario while debugging, for example:

```sh
cargo test --locked --features test-support --test pty -- --case E2E07
```

Quality checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
```

`test-support` enables a separate fixture executable, never a production timer
bypass. The PTY driver launches an isolated worker with an explicit nonsecret
environment before constructing PTY commands; it never sends input to your shell.
Failure injection exercises the shared lifecycle wrapper and is identified as
lifecycle integration coverage. Test workers clean up only their own processes.

The [test catalog](test-cases.md) maps design cases to expected behavior and
prepared JSON data. A PTY cannot establish your emulator's exact font, theme,
or contrast; those still need manual checks in the terminal you use.

## Project boundaries

The app does not read environment files or secret stores, make network requests,
log typed text, or save results. Dependencies need network access once during
setup unless already cached; application usage is offline.

`// TODO: add saved aggregate history after the first release.`

Linux has automated validation in the current workspace. macOS remains a target
that needs its own verification. Full Unicode exercises, persistent settings,
and additional corpus controls are future work.
