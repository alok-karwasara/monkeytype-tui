# monkeytype-tui

Offline typing practice in your terminal. Switch between words and formatted
code, with live time, WPM, accuracy, and error counts. The app uses your terminal's
colors and font; no custom theme, font, shell plugin, or account is needed.

## Install a release (no Rust required)

Release automation is prepared; no public release has been published yet.
Once published, open this repository's **Releases** page and download the
`.tar.gz` asset and its matching `.sha256` file. GitHub's **Source code** links
are for building the project, not the packaged app.

| Your computer | Target in the filename |
| --- | --- |
| Linux Intel/AMD 64-bit | `x86_64-unknown-linux-musl` |
| Linux ARM 64-bit | `aarch64-unknown-linux-musl` |
| macOS Intel | `x86_64-apple-darwin` |
| macOS Apple silicon | `aarch64-apple-darwin` |

For example, on Linux Intel/AMD (substitute the downloaded version):

```sh
sha256sum -c monkeytype-tui-0.1.0-x86_64-unknown-linux-musl.tar.gz.sha256
tar -xzf monkeytype-tui-0.1.0-x86_64-unknown-linux-musl.tar.gz
cd monkeytype-tui-0.1.0-x86_64-unknown-linux-musl
sh install.sh
monkeytype-tui
```

On macOS, verify the matching file with `shasum -a 256 -c FILE.tar.gz.sha256`,
then extract and run its installer the same way. Checksums detect changed or
damaged downloads; these packages are not currently signed or notarized.

The installer copies the executable into `~/.local/bin`, plus notices and
instructions into `~/.local/share/doc/monkeytype-tui`. It needs no administrator
access, performs no downloads, and does not read or edit shell configuration.
If your shell reports `command not found`, add the directory for that session:

```sh
export PATH="$HOME/.local/bin:$PATH"
monkeytype-tui
```

For future shells, add that PATH setting using your shell's normal setup.
Alternatively, choose an existing prefix with `sh install.sh --prefix /absolute/path`;
the executable goes in its `bin` directory. Replace an existing installation
with `sh install.sh --force`. To uninstall the default installation, remove
`~/.local/bin/monkeytype-tui` and `~/.local/share/doc/monkeytype-tui`.
This preserves your separate history database; reinstalling loads it again.

## Build and run from source (developers)

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
| Ready | F2 changes words/code; F3 cycles duration; F4 opens history; Esc opens settings. |
| Settings | Up/Down or Tab selects a field; Left/Right changes it; Enter applies; Esc cancels. |
| Typing | Type normally; Backspace corrects; Esc asks whether to abandon. |
| Abandon prompt | Enter/y abandons; Esc/n resumes. The timer keeps running. |
| Results | Enter starts a fresh ready test; F4 opens history; r retries an unsaved result; Esc opens settings. |
| History | F4 switches Recent/Progress; Up/Down scroll rows; Left/Right change page; Esc returns. |
| History filters | F2 mode; F3 duration; F5 7/30 days/all-time; F6 code scope; F7 speed/accuracy/activity. |
| Clear history | F8 opens confirmation; y clears all results/deck state; Enter/Esc/n cancels. |
| Any screen | Ctrl+C quits. `^C` in the compact footer means Ctrl+C. |

Code indentation, trailing spaces, and line breaks are automatic. **Spaces inside
each line must be typed**, along with brackets, colons, and other punctuation.
The timer starts on the first accepted character, including a wrong character.
Code tests end at the time limit or when all required positions have been
attempted; mistakes do not prevent completion. Word mode replenishes its prompt
and continues until time expires.

Exercises use ASCII text. A bundled pool of **1,598 common words** produces fresh
shuffled sequences. **60 original code snippets** (20 each of Rust, Python, and
JavaScript) use a shuffled deck that persists across launches when history works.
Each snippet appears once per deck; after exhaustion the deck reshuffles, avoiding
an immediate repeat. Code is never executed. No exercises are downloaded or read
from your files. See [content provenance](src/content/PROVENANCE.md).
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

## Local history and progress

Completed tests save aggregate metrics locally: completion time in UTC, mode,
configured and actual duration, exercise identity, WPM/raw WPM, accuracy, errors,
and counters. Abandoned/interrupted tests are excluded. **Typed text, snippets,
and individual keystrokes are never stored.** Results show Saved/Unsaved; a storage
failure lets you keep practicing, and `r` retries from Results with the same ID.

F4 opens recent sessions and a Progress view with speed/accuracy trends, a
10-test moving mean, daily practice minutes, and a latest-ten versus previous-ten
speed comparison once twenty comparable measurements exist. Filters keep word
pools, modes, durations, and metric versions separate. Mixed code snippets are
labeled; exercise-specific PBs require the same exercise/version and finish reason.

The database is `history.sqlite3` under:

- Linux: `$XDG_STATE_HOME/monkeytype-tui`, or `~/.local/state/monkeytype-tui`.
- macOS: `~/Library/Application Support/monkeytype-tui`.

Use `--history-dir /absolute/path` to choose a private directory. Newly created
directories/files use permissions 0700/0600. Existing directories must be private
and files writable/private; symlinked app directories/database/lock files are
rejected. Known parent directory aliases such as macOS `/tmp` are supported.
SQLite is bundled in the executable; users do not install a database server or
SQLite tools. No disk access occurs during active typing or rendering.

For a run with no history access or state creation:

```sh
monkeytype-tui --no-history
```

This also keeps content selection in memory, so cross-launch repeat prevention
is unavailable. History survives normal uninstall/update. To remove saved data
before uninstalling, run:

```sh
monkeytype-tui history purge
# For a custom location:
monkeytype-tui history purge --history-dir /absolute/path
```

Type `PURGE` to confirm; other input/EOF cancels. Close other history-enabled app
instances first. Purge removes only the app's database/journals; unrelated files
are preserved. It leaves an empty `history.lock` for safe coordination. Clear/purge
are ordinary deletion, not secure erasure of backups or filesystem snapshots.

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
The [history test catalog](history-test-cases.md) maps persistence, graphs,
content variation, purge, and installed-package scenarios to executable tests.

## Project boundaries

The app does not read environment files or secret stores, make network requests,
or log typed text. It reads only named nonsecret home/state path variables to find
its own local aggregate database. Dependencies need network access once during
setup unless already cached; application usage is offline.

Linux has automated validation in the current workspace. macOS remains a target
that needs its own verification. Full Unicode exercises, persistent settings,
and additional corpus controls are future work.

## Preparing releases (maintainers)

See [packaging/RELEASING.md](packaging/RELEASING.md) for package checks, the
four-platform workflow, and the remaining checks before publishing a draft.
