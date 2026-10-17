# monkeytype-tui: system design

Status: first implementation exists and is automated-tested on Linux. Product
requirements below reflect the design discussion; formulas, the provisional
60-second default, ASCII scope, and UX policies remain open to product review.
Performance figures below are targets, not measured guarantees.

Current evidence: 64 named Rust tests (including the original
56 fixture examples and generated invariant checks), 24 fast PTY scenarios,
and a separate real 30-second timeout scenario pass in this workspace. Formatting
and Clippy with warnings denied also pass. macOS and real-emulator theme/font
checks have not been run. See [README.md](README.md) for commands.

Local aggregate history, Recent/Progress views, purge, and randomized bundled
content are implemented. Sections 15–16 describe the actual storage/UI design.
Tests were written first; execution evidence is in history-test-cases.md.

## 1. Interview opening: explain the system in one minute

We are building an offline terminal typing trainer with word and code modes.
The user chooses a 30-second, 60-second, or 2-minute test. Code remains visually
formatted, but indentation and trailing whitespace are skipped; spaces inside
lines and punctuation must be typed. A code test ends when time expires or the
snippet has been attempted, whichever happens first.

The design is a single Rust process with one event loop and one state owner.
Terminal input becomes application actions, a deterministic engine updates the
test, and Ratatui renders the resulting state. The engine does not depend on the
terminal, clock hardware, filesystem, or network. This makes the scoring rules
easy to test and avoids concurrency in the latency-sensitive path.

The hardest parts are accurate timing, mapping formatted code to scored input,
fair correction rules, and restoring the terminal after errors. Those deserve
more attention than distributed infrastructure for this product.

## 2. Requirements and boundaries

### Confirmed functional requirements

| Area | Required behavior |
| --- | --- |
| Modes | Switch between words and short formatted code snippets. |
| Layout | Instrument: visible time, WPM, accuracy, errors, and prompt. Exact placement remains adjustable. |
| Appearance | Use the user's terminal theme, colors, and font; no special terminal setup. |
| Duration | Offer 30 seconds, 60 seconds, and 2 minutes. Implementation provisionally selects 60 seconds. |
| Code whitespace | Skip indentation and trailing whitespace on each line; require internal spaces. |
| Code symbols | Type punctuation and symbols such as colons, braces, and brackets normally. |
| Mistakes | Allow forward progress and mark errors; report error count and accuracy. |
| Code termination | End on time expiry or snippet completion, whichever comes first. |
| Platforms | Linux and macOS first. |
| History | Save completed aggregates locally, show recent results and progress graphs, preserve data on normal uninstall, and clear/purge only explicitly. See sections 15–16. |

Automatic movement across code line breaks is the proposed interpretation of
"like word typing, but looking like code." Enter is not required to reproduce
formatting. The exact metric and correction rules in section 7 are proposed,
not a claim of matching Monkeytype's implementation.

### Nonfunctional requirements

- Correctness: ignored formatting cannot affect cursor progression or scoring.
- Responsiveness: target input-to-visible-feedback below 50 ms in a local
  terminal under ordinary load. This is a target to measure, not a benchmark.
- Efficiency: block while idle; avoid busy loops, unbounded queues, and input-path
  I/O beyond terminal access.
- Reliability: restore terminal modes on normal exit, recoverable errors, and
  unwinding panics. Forced termination and terminal disconnection have limits.
- Privacy: no network requests, telemetry, raw-input logs, environment-file reads,
  or secret inspection. Typed snippets are never executed.
- Accessibility: use underlines/text cues as well as palette colors; support a
  readable small-terminal fallback.
- Reproducibility: stable toolchain, application lockfile, small dependency set.

### First-release exclusions

Accounts, cloud synchronization, leaderboards, code execution, full editor
navigation, and plugins remain outside the initial design. Saved aggregate
history is implemented; its design is in section 15.
Persistent settings can still wait: settings live in memory for the process lifetime.

### Interview sizing

This is one interactive user per process. Ten thousand installations mean ten
thousand independent local processes, not ten thousand requests to a shared
service. There is no server capacity, availability quorum, or database sharding
problem in the first release.

Let `N` be scored prompt positions and `S = columns × rows` terminal cells.
Prompt preparation is `O(N)` for a bounded input; code typing and backspace are
`O(1)` per accepted character after preparation. Word-mode replenishment is
amortized across bounded chunks; worst-case redraw is `O(S)`. Source,
position mapping, and the current attempt buffer take `O(N)` memory. Corpus size
is an additional fixed cost. Set explicit corpus/prompt size bounds before
supporting user-imported material; do not retain every historical keystroke.

## 3. Technology choices

| Layer | Proposed choice | Why |
| --- | --- | --- |
| Language | Stable Rust, edition 2024 | Ownership and typed state transitions support a small, memory-safe application. Forbid unsafe in application crates. |
| UI | Ratatui | Cell-based layout and rendering; testable without a real terminal. |
| Terminal backend | Compatible Crossterm | Keyboard events, resize, polling, and terminal control. |
| Execution | Synchronous event loop | Work is local and bounded; no concurrency is needed for typing and drawing. |
| Timing | `std::time::Instant` at the boundary | Monotonic elapsed time, converted to injected durations for engine tests. |
| Content | Bundled curated words and original snippets; rand selection | Offline, varied exercises without downloads or code execution; provenance tracked in src/content/PROVENANCE.md. |
| History | Bundled SQLite through rusqlite | Local transactions, filtered aggregates, code-deck reservation; no user database installation. |
| Tests | Rust unit tests, Ratatui TestBackend, PTY E2E harness | Separate logic correctness, rendering, and real process behavior. |

The verified stack snapshot from this discussion is Rust 1.99.1, Ratatui 0.30.2,
and Crossterm 0.29. Recheck stable patch releases before scaffolding; then pin
the selected toolchain and commit `Cargo.lock`. Ratatui documents the compatible
backend pair in its [installation guide](https://ratatui.rs/installation/).
The Rust version comes from the [official 1.99.1 announcement](https://blog.rust-lang.org/2026/10/15/Rust-1.99.1/).

Rust reduces memory-safety risks; it does not establish that dependencies,
terminal handling, or application logic are secure. Review dependency advisories
and licenses before release.

## 4. High-level design

```mermaid
flowchart LR
    User[User keystrokes and resize] --> Terminal[Crossterm adapter]
    Clock[Monotonic clock] --> Runtime[Runtime event loop]
    Terminal --> Runtime
    Runtime --> Actions[Normalize events into actions]
    Actions --> App[App state and transitions]
    Corpus[Bundled content] --> Prepare[Prompt compiler]
    Prepare --> Engine[Typing engine]
    App --> Engine
    Engine --> App
    App --> Completed[One-shot completion summary]
    Completed --> Runtime
    Runtime --> History[SQLite repository outside active typing]
    History --> App
    App --> View[Read-only view model]
    View --> Renderer[Ratatui rendering]
    Renderer --> Screen[User terminal]
```

The runtime owns effects: polling, clock sampling, and terminal drawing. The
application owns the selected mode, duration, screen, and active test. The engine
owns expected positions, attempted characters, counters, and finish rules.
The renderer only reads a view of those values.

This is a modular monolith using Model–Update–View principles. Modules provide
boundaries without introducing services or a framework. Ratatui describes this
separation in its [Elm architecture guide](https://ratatui.rs/concepts/application-patterns/the-elm-architecture/).

### Main execution workflow

```mermaid
sequenceDiagram
    participant Main
    participant Runtime
    participant Input
    participant App
    participant Engine
    participant UI
    participant History
    Main->>App: Prepare mode, duration, and prompt
    App->>Engine: Compile displayed source and scored positions
    Main->>Runtime: Initialize terminal and monotonic clock
    Runtime->>UI: Draw ready screen
    loop Until exit
        Runtime->>Input: Poll until event, redraw, or deadline
        Input-->>Runtime: Event or timeout
        Runtime->>App: Advance time using fresh clock sample
        App->>Engine: Expire active test if deadline reached
        Runtime->>App: Dispatch normalized action, if present
        App->>Engine: Apply accepted input if still running
        Engine-->>App: Progress or terminal outcome
        opt Completion effect or inactive history navigation
            Runtime->>History: Save or query aggregate state
            History-->>App: Snapshot or visible failure status
        end
        Runtime->>UI: Draw latest state when due
    end
    Runtime->>Main: Return outcome
    Main->>Main: Restore terminal, then report any error
```

Expiration is checked again using a fresh time sample before accepting input.
An event received exactly at the deadline cannot extend the session. Input time
means application processing time; ordinary terminal events do not give us a
portable hardware keypress timestamp.

### Screen and session transitions

```mermaid
stateDiagram-v2
    [*] --> Ready
    Ready --> Settings: Esc
    Settings --> Ready: Apply or cancel
    Ready --> Running: First accepted text character
    Running --> Running: Character or backspace
    Running --> AbandonPrompt: Esc
    AbandonPrompt --> Running: Cancel
    AbandonPrompt --> Ready: Confirm abandon
    Running --> Results: Time expires or code positions exhausted
    AbandonPrompt --> Results: Time expires
    Results --> Ready: Enter for a fresh test
    Results --> Settings: Esc
    Ready --> Exit: Ctrl+C
    Running --> Exit: Ctrl+C
    Settings --> Exit: Ctrl+C
    AbandonPrompt --> Exit: Ctrl+C
    Results --> Exit: Ctrl+C
    Exit --> [*]
```

Settings apply to a fresh test. Abandoning is not completing a test. While an
abandon prompt is open, the active timer continues; canceling cannot provide
free thinking time. These shortcuts and interruption rules are proposed UX
details. Printable characters such as `q`, `:`, and `[` must remain typing input
rather than global shortcuts during practice.

## 5. Low-level design: modules and contracts

Implemented repository structure:

```text
src/
  main.rs                 startup, terminal scope, error exit
  lib.rs                  module exports and integration-test entry points
  runtime.rs              synchronous loop and clock/event adapters
  terminal.rs             terminal lifecycle, cleanup, capability boundary
  input.rs                terminal events -> domain actions
  app.rs                  screen transitions, settings, session ownership
  content.rs              original bundled word/snippet exercises and selection
  engine/
    mod.rs                prepared/active sessions and outcomes
    prompt.rs             formatted display -> scored positions
    metrics.rs            pure calculations from counters and elapsed time
  ui.rs                   Instrument prompt, results, settings, and overlays
tests/
  engine.rs               JSON fixture runner and domain/invariant tests
  transitions.rs          application behavior without a terminal
  rendering.rs            TestBackend checks
  pty.rs                  custom real-process runner and ANSI screen assertions
  support/fixture_app.rs  feature-gated deterministic/failure-injection binary
  e2e/scenarios.json      scenario specifications mapped to the PTY runner
  fixtures/typing-cases.json  scored prompt, session, timing, and metric examples
```

There is one Cargo package exposing the application library and production
binary. The fixture binary requires the `test-support` feature. Keep UI in one
module until screen complexity justifies splitting it. The core engine imports no Ratatui or Crossterm types.

### Conceptual data model

These are design sketches, not copy-paste complete Rust definitions:

```rust
enum Mode { Words, Code }
enum TestDuration { Seconds30, Seconds60, Seconds120 }

struct Settings {
    mode: Mode,
    duration: TestDuration,
}

struct ExpectedUnit {
    character: char,
    display_offset: usize, // Offset in validated, normalized display source
    row: usize,
    column: usize,
}

struct Prompt {
    display: String,             // Includes original visual formatting
    expected: Vec<ExpectedUnit>, // Only positions that must be typed
    scored_at: Vec<Option<usize>>,
    lines: Vec<Range<usize>>,
}

struct Counters {
    attempts: u64,
    correct_attempts: u64,
    backspaces: u64,
    current_correct: usize,
}

enum Phase {
    Ready,
    Running { started_at: Duration },
    Finished { elapsed: Duration, reason: FinishReason },
}

struct Session {
    prompt: Prompt,
    typed: Vec<char>, // Current prefix; next index is typed.len()
    counters: Counters,
    settings: Settings,
    phase: Phase,
}

enum Screen {
    Practice(Session),
    Settings { draft: Settings, field: usize, original: Box<Session> },
    Results(Report),
}

enum Action { Text(char), Backspace, Confirm, Cancel, Quit /* plus menu/resize actions */ }
enum FinishReason { Timeout, SnippetExhausted }
```

`App` owns `Screen` plus optional presentation state such as an abandon dialog.
Only the runtime reads `Instant`. It passes durations measured from a process
clock origin into application/engine methods. A session stores its start offset
and derives elapsed time from those values. Deterministic tests supply the same
offsets directly. There is no second independently mutable countdown.

Useful contracts:

| Operation | Contract |
| --- | --- |
| `Prompt::new(source, mode)` | Validate source and build display/scored mapping; reject an empty scored prompt. |
| `Session::new(prompt, duration)` | Create a ready test. The first accepted character in `apply` starts timing and is processed once. |
| `Session::advance_time(now)` | Freeze a timeout outcome when elapsed reaches the limit. |
| `Session::apply(input, now)` | Check time, apply one input, then check completion. `report()` exposes a final outcome. |
| `Metrics::calculate(counters, elapsed)` | Calculate labeled metrics without I/O or hidden time reads. |
| `ui::draw(frame, app, now)` | Read application state and dimensions; draw without changing scored input. |

An engine session freezes on completion. `App::tick` replaces it with its report
in `Screen::Results`, preventing further mutation of a completed test. The final
report is immutable. A state transition must not duplicate the session.

## 6. The key algorithm: formatted code versus required input

### Example

Displayed snippet:

```rust
fn main() {
    let count = 1;
    println!("count: {}", count);
}
```

Required input, shown as a flattened explanation only:

```text
fn main() {let count = 1;println!("count: {}", count);}
```

The screen still shows lines and indentation. Completing `{` moves the caret
to the `l` on the next line automatically. Spaces in `fn main`, `let count`,
around `=`, and inside `"count: {}"` must be typed. No separator is invented
between lines. Blank lines have no scored positions.

### Prompt compilation

```mermaid
flowchart TD
    Source[Bundled source] --> Validate[Validate characters and source limits]
    Validate --> Normalize[Normalize CRLF and expand display tabs]
    Normalize --> Display[Preserve formatted display text]
    Display --> Lines[For each physical source line]
    Lines --> Edges[Find leading and trailing formatting whitespace]
    Edges --> Units[Map every remaining character, including internal spaces]
    Units --> Mapping[Scored index to displayed source position]
    Mapping --> Session[Typing session consumes scored positions]
```

Algorithm:

1. Validate source before entering a test. Never emit untrusted escape/control
   sequences as raw terminal output. Normalize CRLF to LF.
2. Preserve line structure. Expand tabs into deterministic display spaces before
   constructing position mappings; a four-column tab stop is the proposed rule.
3. For each code line, skip leading/trailing spaces and expanded tabs in the
   scoring stream. Preserve every character between those edges.
4. Map each required character to its position in the normalized display string.
   Newlines remain display formatting and have no scored entry.
5. Reject prompts containing no required characters.

This is a physical-line whitespace rule, not a language parser. Brackets,
comments, string contents, and operators do not require an AST. Spaces inside
a line remain required even inside string literals.

For word mode, single spaces between generated words are scored normally.
Generate words in bounded chunks, retain the attempted prefix, and append the
next chunk before the caret reaches the end. Word mode ends at the time limit;
it does not end accidentally because an initial text buffer was exhausted.

### Character and display-width policy

The initial implementation uses printable ASCII text plus formatting newlines/tabs in
bundled snippets. Normalize tabs as above; reject ESC, NUL, DEL, and unsupported
control characters. ASCII provides a simple one-column character model, while
still using the terminal's chosen font. Non-ASCII user input can produce a
visible unsupported-input notice without advancing or scoring the attempt.
This is the initial implemented scope, not a permanent product limit.

If Unicode text is included, implement grapheme segmentation and terminal width
mapping before shipping it. Do not silently extend byte indexing to Unicode:
byte offsets, graphemes, and terminal columns are different units. Even in the
ASCII design, display offsets never serve as session cursor positions.

### Correction behavior and invariants

- A printable input character fills the next required position, correct or not.
- A wrong character advances and receives an error mark at that expected
  position. A subsequent character cannot silently realign the target.
- Backspace removes the last attempted position and moves to its mapped display
  location, including across a line boundary. At position zero it is a no-op.
- Historical mistakes remain counted after correction; current error styling
  disappears when that position is corrected.
- Enter/Tab used only as formatting do not advance, count, or start the timer.
  Internal expanded-tab spaces must be typed as spaces under the proposed rule.
- Completion means all required positions were attempted, not necessarily
  attempted correctly. Results still expose remaining incorrect positions.
- `typed.len() <= expected.len()`, and each typed position has exactly one
  expected position. Every mapping refers to valid normalized display source.

The no-insertion/no-deletion alignment model is deliberately simple. Missing an
internal space shifts subsequent attempts until the user backspaces; it is not
automatically forgiven by an edit-distance algorithm.

## 7. Timing and metric contracts

### Timing

The timer starts on the first accepted text input, not on launch, mode selection,
paste, or a formatting key. Use a monotonic start time and compute:

```text
elapsed   = now - started_at
remaining = max(0, selected_limit - elapsed)
```

Before applying any action to an active test, expire it if `elapsed >= limit`.
If an accepted character before the deadline consumes the final code position,
finish immediately with `SnippetExhausted`. Timeout wins at the exact boundary.
Freeze elapsed time and counters on completion. For timeout, report the selected
limit rather than an overshoot caused by redraw scheduling.

### Initial formulas (implemented; open to product review)

Let:

- `A`: total accepted character attempts, including required internal spaces and
  retyped positions; excludes skipped formatting, backspace, and control keys.
- `K`: accepted attempts that matched the expected character at their position
  when typed. Backspace does not erase this historical count.
- `C`: correctly matched positions in the current typed prefix after corrections.
- `U`: incorrectly matched positions in the current prefix.
- `t`: effective elapsed time in minutes.

| Label | Formula | Interpretation |
| --- | --- | --- |
| Accuracy | `100 × K / A` | Historical correctness; mistakes remain visible after correction. |
| Errors | `A - K` | All incorrect attempts, including subsequently corrected errors. |
| Uncorrected | `U` | Incorrect positions remaining in the current buffer. |
| Raw WPM | `(A / 5) / t` | Input throughput including corrections and mistakes. |
| WPM | `(C / 5) / t` | Correct current progress; retyping does not inflate correct progress. |

Use five characters as the word-equivalent unit in both modes. Code WPM includes
required internal spaces and punctuation, while excluding auto-skipped formatting;
label this convention clearly. Backspace counts can be reported separately.
Do not subtract errors a second time from an already correct-progress WPM.

Show an em dash before any attempt or while elapsed time is zero; never show NaN,
infinity, or a fabricated positive elapsed time. Format floats only at the view
boundary, and keep sufficient precision in calculations.

Example: target `abc`; input `a`, `x`, Backspace, `b`; elapsed 10 seconds,
while the session is still running with `c` pending.
`A=3`, `K=2`, `C=2`, `U=0`: accuracy is 66.7%, errors 1, raw WPM 3.6, WPM 2.4.
At completion the error is corrected visually, but it remains in accuracy.

## 8. Terminal input, rendering, and lifecycle

### Event loop scheduling

1. Sample time and settle due expiration before doing other work.
2. Poll until the earlier of the next live redraw and active deadline. Idle
   screens can use a longer bounded wait because input wakes polling immediately.
3. Read available input, normalize it, then sample time again before updating.
4. Update state once per action. Check deadlines during input bursts rather than
   draining an unlimited batch before processing timers.
5. Render changed state at a bounded rate; approximately 30 frames/second during
   a test is a starting target. Render final results promptly.

No fixed sleep follows every keypress. Elapsed time is derived from the clock,
not from a tick counter. All Crossterm `poll`/`read` calls stay on the same thread,
as required by its [event API](https://docs.rs/crossterm/latest/crossterm/event/index.html).

Normalize press events once, ignore releases, and define repeat behavior
explicitly. Implemented behavior: accept text/backspace repeat as input, ignore
repeated menu commands. Some terminal protocols report held keys as ordinary
presses, so portable repeat detection is limited. Support documented basic key
sequences without requiring an enhanced keyboard protocol.

Enable bracketed paste where supported and ignore `Paste` events during a test.
Without terminal paste reporting, ordinary text can be indistinguishable from
typing; do not claim perfect paste detection or anti-cheat protection.

### Appearance and layout

Keep default foreground and background rather than hardcoding an RGB theme.
Use the terminal's ANSI palette sparingly for semantic accents, plus underlines,
labels, and cursor styling. Ratatui exposes default colors through
[`Color::Reset`](https://docs.rs/ratatui/latest/ratatui/style/enum.Color.html).
The terminal controls the font, font size, ligatures, and its background effects;
the app does not load fonts or change terminal configuration.

The Instrument layout contains mode/duration status, time/WPM/accuracy/errors,
the formatted prompt, and shortcut hints. Center the caret's logical position
within a scrolling prompt viewport. Wrapping and resizing affect presentation,
never the expected sequence or scoring. Cache line-to-position information so
redraws do not scan the entire corpus.

The implemented minimum is 40 columns × 12 rows. Below that size, show a resize
message and exit shortcut where space permits. Time continues; text input is
ignored and not scored. Query the actual terminal size before dispatching input:
PTY testing exposed that a draw can observe resized dimensions before a Resize
event reaches the input loop. Real-terminal usability still needs manual review.

### Startup, errors, and cleanup

Validate bundled content and settings before modifying the terminal. Check for
an interactive terminal and give a plain error when invoked through unsupported
pipes. Use the current supported Ratatui lifecycle APIs; its
[terminal documentation](https://docs.rs/ratatui/latest/ratatui/struct.Terminal.html)
recommends the provided lifecycle helpers for the default fullscreen backend.

Pair terminal entry with a cleanup scope, including partial-initialization
failure. Prefer the library's panic-restoration behavior over competing custom
hooks. Restore raw mode, alternate screen, cursor state, and any paste/keyboard
features the app enabled before reporting an error. Cleanup should be idempotent
and avoid panicking while handling another error.

Ctrl+C in raw mode is an application action; OS termination signals are a
different boundary. `signal-hook` sets an atomic shutdown flag for SIGTERM and
external SIGINT; terminal cleanup runs on the normal thread. The locked signal-hook
version is 0.4.5; Crossterm also has its own transitive 0.3 dependency. For v1, Ctrl+Z is handled as an
unsupported suspend command rather than leaving a raw terminal behind.
No program can run cleanup after SIGKILL or power loss. Do not promise otherwise.

## 9. Failure analysis

| Failure or edge case | Intended behavior |
| --- | --- |
| Empty/formatting-only snippet | Reject during preparation, before timing begins. |
| Invalid/control-bearing bundled source | Reject content; never execute or emit raw escape sequences from it. |
| Wrong character or missing internal space | Mark its expected position; continue; allow backspace correction. |
| Backspace over a visual newline | Return to the previous required character, skipping formatting automatically. |
| Input at/after deadline | Freeze timeout result; do not accept the character. |
| Final code character is wrong | Finish when all positions are consumed; show error and accuracy consequences. |
| Time expires during abandon prompt | Finalize results; cannot resume the expired session. |
| Resize or very small terminal | Preserve session state; apply the documented small-window policy. |
| Paste | Ignore reported paste during tests; note terminal protocol limitations. |
| Terminal draw/read failure | Exit through cleanup, then report the error outside raw mode. |
| Panic | Restore through supported panic handling; report the original failure. |
| SIGKILL or terminal disappears | Cleanup cannot be guaranteed; no false recovery promise. |

## 10. Testing strategy and concrete cases

The detailed catalog is in [test-cases.md](test-cases.md). Machine-readable
prompt and session examples are in `tests/fixtures/typing-cases.json`; PTY
scenario specifications are in `tests/e2e/scenarios.json`. The implemented
fixture runner is `tests/engine.rs`; `tests/pty.rs` implements the real-process
scenarios. Verification evidence and limits are recorded below.

### Unit tests: deterministic domain behavior

Inject elapsed durations; no wall-clock sleeps. Keep corpus selection seeded or
fixture-based. Test observable contracts rather than private field names.

| Case | Expected assertion |
| --- | --- |
| `    let n = 1;   ` | Required stream is `let n = 1;`; inner spaces remain. |
| CRLF, blank lines, indentation tabs | Stable normalized display, valid mapping, no phantom scored characters. |
| String containing spaces | Spaces inside the line remain scored. |
| Unsupported controls / empty scored prompt | Preparation fails clearly. |
| Wrong internal space | Advances one position, increments errors, lowers accuracy. |
| Backspace and retype | Restores cursor position; historical errors remain counted. |
| Backspace at start / across line boundary | Safe no-op / correct mapped previous position. |
| First wrong character | Starts the clock and counts exactly once. |
| Tab, Enter, release, or paste in ready state | Does not accidentally start or advance the test. |
| `limit - epsilon`, `limit`, `limit + epsilon` | Accept before deadline; timeout at/after it. |
| Final position consumed before deadline | Finishes immediately, even if incorrect. |
| Input after completion | Frozen report cannot change. |
| Zero attempts or zero elapsed | No NaN, division by zero, or misleading accuracy. |
| Correction example from section 7 | Exactly the documented counters and displayed rounded metrics. |
| Word-buffer replenishment | No lost prefix, inserted separator errors, or premature completion. |

Property checks can generate small prompts and action sequences: cursor bounds,
valid display mappings, `K <= A`, `C + U == typed.len()`, bounded progress, and
immutable finished reports. Introduce a property-testing dependency only if it
adds useful coverage beyond a deterministic case table.

### Application and rendering tests

Exercise mode/duration changes, cancel/apply semantics, abandoning, restart,
and deadline processing while an overlay is open. Use Ratatui TestBackend to
assert meaningful cells and styles at normal, narrow, and tiny sizes. Check
underlines as well as color; screenshot text alone does not prove error styling.
Do not use a full-screen snapshot for every internal transition.

### E2E tests: launch the real binary in a pseudo-terminal

A PTY harness creates an isolated terminal, starts the built binary, sends bytes
for characters and keys, and interprets ANSI output into a screen buffer. It
can resize the PTY and inspect terminal attributes on the slave descriptor.
The dev-only runner uses `portable-pty` 0.9 and `vt100` 0.16.2; JSON examples use
`serde_json`. It first launches a worker with explicitly cleared environment
inheritance because the PTY command builder otherwise enumerates ambient values.
The worker receives only fixed nonsecret terminal/locale/path/shell values.
Fixture and failure-injection launches use a separate feature-gated executable;
the production CLI has no test clock or fixture arguments.

| Scenario | User-level check |
| --- | --- |
| Startup and exit | Ready screen appears; Ctrl+C exits successfully and restores terminal state. |
| Words/code selection | Settings select the requested mode and duration. |
| Code whitespace | Typing trimmed lines with internal spaces completes without indentation/Enter input. |
| Mistakes and correction | Visible mark, changed metrics, and successful backspace correction. |
| Snippet completion | Results appear before the selected limit, with a completion reason. |
| Real timeout | At least one real 30-second process test expires and rejects subsequent typing. |
| Resize | Normal → small → normal preserves progress and the agreed timer policy. |
| Paste event | Bracketed pasted text does not produce normal typing progress. |
| Error/panic cleanup | A test-only failure harness restores raw/echo modes and alternate screen. |
| SIGTERM | Graceful signal path exits and restores the terminal on supported platforms. |

Keep fast clock-boundary coverage in unit/in-process integration tests. A PTY
cannot make the real production binary's `Instant` advance magically; do not
add a hidden production timer-bypass flag just to shorten tests. One real-time
timeout check can run in the slower E2E suite with bounded overall timeouts.
Wait on observable screen states rather than arbitrary short sleeps.

At first-input time, the engine's zero-elapsed contract should be asserted by
unit tests; a PTY screen can already be milliseconds past that point. Parse
terminal output rather than comparing escape-byte sequences, which may differ
between backends or terminal modes. Inspect terminal attributes and emitted
restore sequences; neither alone proves every emulator's visual behavior.

Run unit/rendering checks on Linux and macOS. Run PTY tests where supported and
report skipped platform checks. Manually verify the real terminal's font, light
and dark themes, cursor, and cleanup; a simulated terminal cannot prove those.

Required Rust checks once code exists:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Implemented commands:

```sh
cargo test --locked --features test-support --test pty
cargo test --locked --features test-support --test pty -- --slow
```

The fast runner covers 13 scenarios; the slow runner covers the real 30-second
clock. All 14 pass on Linux. Fault variants are lifecycle integration tests,
including failure after enabling raw mode but before alternate-screen entry.
The catalog is broader than individually named Rust functions: 25 Rust test
functions cover multiple case IDs and 56 JSON fixture examples. Manual emulator
checks remain pending; do not report the entire catalog as independently passed.

## 11. Alternatives and why we are not choosing them initially

These are tradeoffs for this workload, not claims that the alternatives are bad.

| Alternative | Benefit | Why not initially | Revisit when |
| --- | --- | --- | --- |
| One large `main.rs` | Fastest tiny prototype | Couples timing, input, scoring, and drawing; difficult to test boundaries. | Suitable only for an exploratory throwaway. |
| Traditional MVC with mutable widgets/controllers | Familiar GUI organization | Distributed state ownership makes deadline and scoring invariants harder to enforce. | Many independent editing widgets need rich local behavior. |
| Full component/action framework | Reusable components and standardized composition | Extra abstractions for a few screens; the engine boundary is more valuable now. | Screen count and independent widget state grow substantially. |
| Tokio plus async event stream | Natural network and concurrent I/O integration | No network work exists; task cancellation and shutdown add complexity. | Sync, downloads, or other genuinely concurrent I/O is approved. |
| Input thread + render thread + shared state | Can isolate slow work | Locks, queues, and event ordering complicate deadline handling without demonstrated latency benefit. | Measurements show bounded single-thread work is insufficient. |
| Separate domain/UI Cargo crates | Strong compile-time dependency isolation | More manifests, APIs, and versioning before the boundary has stabilized. | Engine is reused by another frontend or package. |
| Direct ANSI/raw terminal implementation | Full control and fewer UI abstractions | We would own escape handling, rendering differences, resize behavior, and test infrastructure. | Required behavior cannot be represented by the chosen backend. |
| Go with Bubble Tea | Event-loop UI model and straightforward distribution | Rust is the agreed direction and lets this codebase forbid application unsafe code; changing languages adds no current product benefit. | Team expertise or ecosystem requirements change. |
| Python with Textual | Fast UI iteration and a rich framework | Adds an interpreter/framework deployment choice; compiled Rust fits the current stack preference. | Prototyping speed or an existing Python product dominates. |
| Browser/Electron application | CSS typography and familiar web tooling | Changes the requested terminal product and runtime footprint. | A separate desktop/web frontend is requested. |
| Persistence before a history requirement | Durable history and reporting | Originally deferred to avoid speculative storage. The requirement now exists; section 15 recommends SQLite and compares file storage. | Current history work. |
| Language AST/parser for snippets | Syntax-aware selection and highlighting | Typing correctness uses exact characters, not program semantics. | Language-aware exercises or highlighting justify it. |
| Edit distance for live alignment | Can infer missed/extra characters | May hide failures or jump the caret; harder to explain and score consistently. | Adaptive error alignment is explicitly requested. |
| Scoring the raw formatted file | Simplest representation | Violates the requirement to skip indentation and trailing whitespace. | A separate exact-formatting practice mode is requested. |
| Hardcoded RGB themes/custom fonts | Predictable screenshots | Overrides the user's terminal appearance and adds setup. | User explicitly opts into an optional theme; never required. |

## 12. Evolution, observability, and interview follow-ups

### What happens when history is added?

History is implemented through a results repository boundary after completion,
aggregate-only records, and bundled SQLite transactions. Persistence failures
leave results visible and preserve future practice and terminal cleanup.
See sections 15–16 and the history test catalog for details.

### How would we diagnose slow input?

Measure event handling and rendering separately using opt-in aggregate timings.
Do not log input text, environments, or credentials. Check redraw frequency,
prompt scanning, and output backpressure before introducing threads. Benchmarks
should report terminal dimensions, platform, workload, and limitations.

### What if content is imported later?

Introduce size limits, control-character validation, explicit encoding and
Unicode policy, and license/user-provenance handling before accepting arbitrary
files. Selection should use explicit paths supplied by the user; never scan
environment files, secret directories, or the home directory for content.

### How would this scale to cloud features?

The local engine remains authoritative for its session behavior. A separate,
optional sync boundary could upload aggregate results after user authorization.
Authentication, offline conflict handling, retries, and anti-cheat then become
new design problems. They do not justify a backend for the current app.

### What can we promise about matching the terminal theme?

We can preserve default foreground/background and use terminal-selected palette
entries. We cannot assume those colors have universal RGB values or contrast.
Font selection belongs to the emulator. Text cues and real-terminal checks
remain necessary. No environment or terminal configuration file inspection is
needed to inherit default colors and font.

### What remains for product review and release validation?

1. Review the provisional initially selected duration of 60 seconds.
2. Confirm the proposed formulas/correction rules and the initial character set.
3. Validate shortcut and small-window policies with the actual TUI.
4. Run macOS and manual appearance checks, and measure performance targets.
   Automated coverage is implemented; do not infer unmeasured guarantees from it.

## 13. Implementation milestones and remaining review

1. Implemented: stable Rust package, lifecycle wrapper, and terminal-native view.
2. Implemented: source validation, prompt compilation, and whitespace mapping
   with fixture-backed tests.
3. Implemented: deterministic sessions, timing boundaries, correction, and metrics.
4. Implemented: normalized input and screen transitions in the synchronous runtime.
5. Implemented: Instrument practice, settings, and results views with resize tests.
6. Implemented: PTY coverage and Linux checks. Pending: macOS/manual appearance
   validation, product review of provisional behavior, and a release dependency audit.
7. Implemented: local aggregate history, progress dashboard, explicit purge, and
   varied bundled content. Pending release/platform work remains as above.

## 14. Application distribution

The source repository is the development input. End users receive compiled
GitHub Release archives and install `monkeytype-tui` into a directory on PATH;
they do not need a Rust toolchain. Linux x86_64/ARM64 packages use static musl;
macOS Intel/Apple silicon packages use their native Rust targets.

### Why users do not need Rust installed

Rust is a compiled language. On the developer machine or GitHub Actions runner,
`cargo build --locked --release --no-default-features --bin monkeytype-tui
--target TARGET` uses the pinned Rust compiler and platform linker to translate
the application and its dependencies into native machine code. The resulting
`target/TARGET/release/monkeytype-tui` is an executable the operating system can
load directly. Rust library code, including the needed standard-library code,
is linked into the executable in this build configuration. We do not enable
dynamic Rust-library linking or require a separately installed Rust runtime.
See the [Rust linkage reference](https://doc.rust-lang.org/reference/linkage.html).

```mermaid
flowchart LR
    A[Rust source and locked dependencies] --> B[Rust compiler and linker on developer or CI machine]
    B --> C[Native executable for one OS and CPU]
    C --> D[Release archive]
    D --> E[Installer copies executable into prefix/bin]
    E --> F[Shell finds monkeytype-tui through PATH]
    F --> G[Operating system runs machine code directly]
```

| Stage | Where it happens | Required tools |
| --- | --- | --- |
| Compile and package | Developer machine / CI | Pinned Rust/Cargo, target linker/SDK, Python packaging tools. |
| Download and install | User machine | Matching archive, normal shell, extraction/checksum utilities; no compilation. |
| Run | User machine | Compatible OS/CPU, required OS libraries, terminal; no Rust, Cargo, Python, or Git. |

`packaging/install.sh` copies the prebuilt executable into `~/.local/bin` by
default, makes it executable, and installs its documentation/notices. It does
not call Cargo or build from source. When the user types `monkeytype-tui`, the
shell searches PATH and starts that file. PATH controls where the shell finds
the executable; it does not provide a language runtime or alter the executable.

This is platform-specific distribution, not a universal executable. Linux musl
targets are intended to link the C runtime statically to avoid a host glibc
dependency; inspect the built artifacts to confirm linkage before release.
The local GNU Linux preview instead uses host-compatible system libraries.
Inspection with `file` and `readelf -d` confirms that this preview is an x86_64
ELF executable whose declared shared-library dependencies are libc.so.6,
libgcc_s.so.1, and libm.so.6, with no dynamic Rust or SQLite-library dependency. This is evidence for
the local preview, not for the other planned release targets.
macOS executables can depend on libraries supplied by macOS. All variants still
need a compatible OS/kernel version and terminal. Compiling from source remains
an optional developer path that does require Rust.

### Build, release, and installation flow

```mermaid
flowchart LR
    V[Matching version tag] --> T[Native build and tests on four runners]
    T --> P[Production binary + notices + offline installer]
    P --> S[Checksum and temporary installation smoke check]
    S --> D[Draft GitHub Release]
    D --> R[Maintainer review and publish]
    R --> U[User downloads matching archive]
    U --> I[Install in prefix/bin]
    I --> C[Run monkeytype-tui through PATH]
```

The implementation is `.github/workflows/release.yml`,
`scripts/package-release.py`, and `packaging/install.sh`. The packager traverses
target-filtered normal/build dependencies for public license notices, excludes
dev-only dependencies, and archives an explicit asset list. Raw metadata and
source files are not shipped. Only `--no-default-features --bin monkeytype-tui`
release builds are packaged; fixture features stay in the verification build.

The offline installer checks OS/CPU, supports an absolute installation prefix,
installs docs/notices, and replaces a regular executable only with `--force`.
The executable is staged beside its destination before rename. Shell profiles
and environment files are never inspected or modified. An installer subprocess
cannot change its parent shell's PATH; instructions explain the one-time setup.

Alternatives: crates.io/Cargo installation requires the user to compile and is
not the requested primary distribution. Homebrew and OS-native packages improve
package-manager integration but require additional manifests and maintenance.
Cargo-dist can generate several formats; a small explicit workflow and offline
installer suffice for the initial four targets without profile modification.
A remote installer could automate downloads later; it is not required to launch
a downloaded executable through PATH. No updater service or runtime networking
is introduced. See [release process](packaging/RELEASING.md) for the remaining
license, advisory, signing, and platform checks; this setup is not a published
release or evidence that unrun platforms pass.

The installed-artifact check (`tests/packaging.py --archive PATH`) verifies the
checksum and explicit asset list, installs into a temporary prefix, runs
`--version`/`--help`, completes/saves a code test in a PTY, removes/reinstalls
the binary, and reopens retained History/Progress, checking terminal restoration. It supplies an isolated environment
whose PATH excludes this workspace's Rust toolchain. This verifies the installed
executable's behavior without invoking build tools; it does not claim that Rust
was uninstalled from the test host or that unrun platforms passed.

## 15. Local history: implementation design

Status: implemented after test-first acceptance cases. History is enabled by
default and stores completed aggregate records only. All mapped Linux tests
pass; manual terminal appearance and other platforms remain unverified here.

### Product scope and history screen

- Save one aggregate record for a completed test: timeout or snippet exhausted.
  Do not save abandoned, never-started, or interrupted tests as completed runs.
- Enable local saving by default, with visible saved/unsaved status on results.
  Provide `--no-history` for a run with no history reads, writes, directory
  creation, migrations, or deletion. No network or accounts are introduced.
- F4 opens history from ready/results only. Ignore it while typing. Esc returns
  to the exact prior screen; opening history never creates or restarts a test.
- Show recent sessions newest-first, 50 per page, with UTC completion date/time,
  mode, selected duration, actual elapsed time, finish reason, WPM, raw WPM,
  accuracy, and error count. Use the existing terminal palette and text cues.
- Filter by words/code and selected duration. Show session count, best WPM, and
  weighted accuracy for the chosen comparison group. A short completed code
  snippet and a full timed word test must not become the same leaderboard.
- Compare only the same metric-definition version. Code bests additionally
  require the same exercise ID/version, duration, and completion reason; recent
  code runs may still be listed together. Do not label unlike exercises as a PB.
- Offer clear-all with explicit confirmation, defaulting to cancel. Keep records
  until the user clears them; no automatic expiry in the first iteration.
  Stats and charts are now requested for this iteration; section 16 extends the
  dashboard design. Export/import, per-key analysis, streaks, and cloud sync can
  follow later.

### Technology and alternatives

Keep the current stable Rust/Ratatui/Crossterm stack and synchronous event loop.
Use rusqlite 0.40.2 with bundled/fallible_uint features and Cargo.lock pinning.
The fallible integer conversions reject out-of-range unsigned counters. SQLite's embedded
model fits local application storage without a server. Its bundled library is
compiled and linked during our build, so users need neither SQLite nor Rust
installed. See [SQLite use cases](https://sqlite.org/whentouse.html) and
[rusqlite build documentation](https://github.com/rusqlite/rusqlite#notes-on-building-rusqlite-and-libsqlite3-sys).

Use `std::time::SystemTime` for stored UTC wall-clock timestamps and retain
`Instant`/injected durations for measurements. uuid 1.28.0 constructs a stable
random UUID-shaped record ID from the session selector, reused across retries;
these IDs are not security tokens. time 0.3.55 provides UTC calendar components
for readable dates. Do not enable local-time discovery,
serialization integrations, SQLite extensions, an ORM, or an async runtime
without a requirement. Recheck versions and compatibility when upgrading.
SQLite is native C behind Rust bindings; forbidding unsafe in application code
does not make every dependency memory-safe. Include it in dependency review.

| Alternative | Advantage | Why SQLite was selected |
| --- | --- | --- |
| One JSON document | Easy to inspect and export | Every update needs a protected read/modify/atomic-replace cycle; two app instances need locking to avoid lost updates. |
| Append-only JSONL | Small initial implementation | Crash-tail repair, deduplication, grouping, and clear/rewrite locking become custom storage work. |
| Embedded Rust key/value store | Avoids the SQLite C dependency | Requires application indexes/aggregation for these relational queries; reconsider if avoiding C becomes a firm requirement. |
| SQLx / ORM | Async and richer database abstraction | Current synchronous, single-table local workload does not require them. |
| Hosted database | Multi-device access | Adds accounts, networking, credentials, and service operations outside this request. |

### High-level flow and ownership

```mermaid
flowchart LR
    E[Engine finishes and freezes report] --> A[App transitions to Results once]
    A --> F[Completion effect with immutable summary and session ID]
    F --> R[Runtime handles effect after active timing ends]
    R --> D[History repository and SQLite transaction]
    D --> S[Saved or unsaved status on Results]
    H[F4 from ready or results] --> Q[Paginated history query]
    Q --> V[History screen and grouped statistics]
```

The engine continues to know nothing about the filesystem, SQLite, wall clocks,
or history UI. In the current code, completion can be discovered by runtime
`tick`, dispatch's initial deadline check, or its final tick after a character.
The centralized Results transition emits one Completion summary for all of them. Drain effects after both tick and dispatch, before interpreting a later
restart/quit. Store a frozen summary, not references to mutable settings.

Initially perform repository work synchronously only at startup, after completed
timing, or while the history/results screens are inactive. Do not query, write,
or migrate in the active typing path or renderer. Use short lock waits and
measure actual storage latency; introduce a single storage worker only if UI
stalls justify it. No background retry may start disk work during a later test.

Modules: history/model.rs owns validated summaries/filter/page types;
history/store.rs and schema.sql own transactions and the v0-to-v1 migration;
history/path.rs owns private app locations and no-follow files; service.rs
coordinates completion/retry/clear/query effects at the runtime boundary.
app.rs owns completion effects and navigation; ui.rs renders state only.
There is no separate migrations.rs while only one schema migration exists.
Keep an injectable repository boundary for tests,
without introducing separate Cargo crates.

### Low-level record and schema

One `sessions` table stores completed results; section 16 adds a separate table
for content-selection state. Session fields:

| Field | Meaning |
| --- | --- |
| `id` | Unique stable UUID assigned outside the engine; reused for every save retry. |
| `completed_ms` | UTC completion timestamp; not used to calculate elapsed time. |
| `mode`, `seconds` | Words/code and selected 30/60/120 limit. |
| `elapsed_ns`, `reason` | Actual monotonic elapsed duration and timeout/snippet-complete outcome. Nanoseconds preserve the report's measurement precision. |
| `exercise_id`, `exercise_version` | Stable bundled corpus/snippet identity; no prompt/source text. Add identity to content preparation rather than inferring from display text. |
| `pool`, `pool_version`, `language` | Identify the word pool/code collection and optional code language for filtering and fair comparisons as content expands. |
| `attempts`, `correct_attempts`, `current_correct`, `backspaces`, `uncorrected` | Integer aggregates from the frozen report; no character values. |
| `wpm`, `raw_wpm`, `accuracy` | Snapshot of the displayed metrics; nullable when unavailable. |
| `metric_version`, `app_version` | Scoring-definition version and producing app version. Schema version is separate. |

Use SQLite STRICT tables with NOT NULL/CHECK constraints where appropriate,
bounded integers, enumerated modes/reasons, and finite/range-checked metrics at
the Rust boundary. Counters must satisfy the engine's invariants; reject malformed
records without corrupting the current report. An IMMEDIATE transaction checks
the session ID then inserts with parameterized SQL. On an existing ID, compare
the immutable summary before acknowledging a retry; conflicting data is rejected.
This implements deduplication without broadly ignoring unrelated SQL errors. Show Saved only after a successful
commit or verified matching existing record.

Index newest-first pagination on `(completed_ms DESC, id DESC)`
and comparison groups on `(metric_version, mode, seconds, pool_version,
exercise_id, exercise_version, reason)`. Read page and statistics in one
read transaction for a consistent snapshot. Use keyset pagination with the
timestamp/ID tie-breaker, deterministic ordering even for equal timestamps,
and bounded page results. Never read all history into a render frame.

Compute errors as `attempts - correct_attempts`; weighted group accuracy is
`100 * sum(correct_attempts) / sum(attempts)`, not the average of percentages.
Exclude unavailable WPM values from bests. Group statistics must explicitly
describe their filters; code completion remains actual-duration scoring.

Apply schema migrations transactionally and track PRAGMA user_version. Read/recheck
the version inside the IMMEDIATE write transaction to prevent stale initialization
when two instances launch against a new database.
An unknown newer schema disables history with a visible notice; never downgrade,
overwrite, or delete it automatically. Start with rollback journaling and full
commit synchronization, rather than adding WAL maintenance for one small
post-test write. Configure a 100 ms SQLite busy timeout. That bounds
lock waiting, not all disk latency; concurrent instances may receive an unsaved
notice and retry explicitly on Results with the same session ID.

### Storage location, privacy, and failures

On Linux use `$XDG_STATE_HOME/monkeytype-tui/history.sqlite3`, falling back to
`~/.local/state/monkeytype-tui/history.sqlite3`; accept only absolute XDG paths.
The [XDG specification](https://specifications.freedesktop.org/basedir/0.8/)
identifies history as persistent user state. On macOS use
`~/Library/Application Support/monkeytype-tui/history.sqlite3`. An explicit
`--history-dir /absolute/path` supports controlled test/install locations.
`--no-history` takes precedence and bypasses location resolution and storage.

Read only the named nonsecret path variables needed by the app; never enumerate
the process environment, inspect shell profiles/environment files, or scan user
directories. Agent tests must use temporary injected paths, never the user's
existing history. Create app-owned directories/files with owner-only Unix
permissions (0700/0600); preserve unrelated files and do not follow unexpected
database symlinks. The database is local and not encrypted; do not describe
ordinary clear-all as secure erasure. Backups/import are later explicit actions.

If creation, migration, locking, disk writes, or queries fail, show a concise
history-unavailable/unsaved status. Preserve the current results in memory and
allow more practice and normal cleanup. Never silently reset a corrupt database,
claim an unsuccessful save succeeded, or auto-delete history on upgrade/uninstall.

### Implementation and acceptance checks

1. Implemented summary types, defaults/comparison groups,
   stable exercise identities, metric version 1, and one-shot completion effects.
2. Implemented bundled SQLite, schema/migrations, explicit path handling, disabled mode,
   validation, and temporary-directory repository tests.
3. Connected completion effects at the runtime boundary; add visible Saved/Unsaved
   state and an explicit retry path. Test no duplicate saves on redraw/restart,
   snippet completion, timeout-in-overlay, or deadline-coincident quit/navigation.
4. Implemented history navigation, pagination, filters, and grouped stats using the
   existing Ratatui layout; add rendering and state-transition tests.
5. Implemented PTY tests: finish, quit/relaunch, inspect saved result; incomplete/abandoned
   runs excluded; no-history creates/reads nothing; clear cancellation preserves
   data; confirmation clears; failed storage still permits typing and safe exit.
6. Verified rollback on migration failure, unknown schema, malformed/corrupt DB,
   locked/read-only storage, duplicate IDs, equal timestamps, metric versions,
   and comparisons across mode/duration/exercise. Verify schema contains no raw
   text fields and persisted fixture data contains no typed/source strings.
7. Updated README, test catalog, release notices, and the four-target workflow.
   Only the local GNU package has run here; native workflow targets remain unrun.
   Verify the installed binary saves/loads history without system SQLite or Rust.
   Run the existing scoring/timing/cleanup suites and measure storage/UI latency.

The mapped cases are executable and pass on Linux; see history-test-cases.md.
Performance numbers remain unmeasured targets. The release workflow will verify
other native targets; no macOS/ARM/musl pass is inferred from local GNU checks.

## 16. History lifecycle, progress dashboard, and varied exercises

Status: implemented. Stats, charts, varied practice, uninstall/reinstall
retention, and explicit purge have Linux unit/renderer/PTY/package coverage.
The following describes current behavior and remaining platform limits.

### Uninstall, reinstall, and purge

The installed executable and docs belong under the installation prefix; the
history database belongs under the user's state/application-data location from
section 15. Removing one must not implicitly remove the other.

| Action | Executable/docs | History and content-selection state |
| --- | --- | --- |
| Normal uninstall | Removed by the user's uninstall process | Preserved. |
| Reinstall or upgrade | Installed/replaced | Reopened; supported migrations preserve records. |
| Clear history in app | Kept | Delete result records/derived stats and reset selection state after explicit confirmation. |
| Explicit data purge | Kept unless followed by uninstall | Remove app-owned database and its journals/selection state after explicit confirmation. |
| Purge then uninstall | Removed | Removed by the preceding purge. |

The `monkeytype-tui history purge` command runs before uninstall and requires
typing PURGE; Enter/other input/EOF cancels. If the binary was already removed,
reinstall and purge, or explicitly remove only the documented app-owned state
files. Do not imply that a future OS package manager's purge necessarily erases
per-user data. A custom --history-dir must use the same documented lifecycle.

Purge must close its database connection and acquire exclusive lifecycle access;
if another instance is using history, refuse rather than unlinking a live database.
All normal history connections participate in that lifecycle lock. Remove only
known app-owned files, handle missing files idempotently, reject unexpected
symlinks, and preserve unrelated files even inside a custom directory. Partial
deletion failures must be reported honestly. Purge is ordinary deletion, not
secure erasure of filesystem snapshots/backups. Normal app startup and the
installer must never purge data. --no-history never reads/deletes stored history.
Purge retains an empty history.lock: deleting the lock inode would allow new
instances to lock a different inode while a previous purger still holds its lock.
The empty coordination file contains no results, identifiers, or selection state.

### Progress dashboard and meaningful patterns

Add Recent and Progress views under the history screen. Keep filters visible:
mode, selected duration, metric version, content pool/version, optional code
language/exercise, and period (7/30 days/all-time). Keep raw WPM and corrected
WPM distinctly labeled. Current dashboard:

- Cards: completed tests, total actual practice time, best and median WPM for
  comparable groups, and weighted accuracy from summed attempts/correct attempts.
- Speed chart: at most the latest 50 comparable sessions, ordered oldest-first,
  with a trailing 10-session mean when ten samples exist.
- Accuracy chart: a separate selectable chart with a labeled percentage axis;
  never overlay percentages on the WPM axis without clear units.
- Activity bars: summed actual practice minutes per UTC day for the last 30 days.
  Fill empty days with zero; label the UTC boundary consistently.
- Pattern summary: when at least 20 comparable sessions exist, compare the mean
  WPM of the latest ten with the previous ten and report the numerical change.
  Describe an observed change, not a guaranteed improvement or causal diagnosis.

Use Ratatui's existing Chart, Sparkline, and BarChart widgets. No browser, image
assets, charting service, or new plotting dependency is required. Preserve terminal
colors/font and text labels; narrow layouts show compact text/sparklines, and
the existing too-small fallback still applies. Sources:
[Chart](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Chart.html),
[Sparkline](https://docs.rs/ratatui/latest/ratatui/widgets/struct.Sparkline.html).

Group comparisons by mode/duration/metric version and word-pool version. For code,
exercise-specific PBs require matching exercise/version and finish reason as in
section 15. A mixed-snippet overview is allowed for activity and exploratory
speed trends, but must say "mixed snippets" and must not claim an equivalent
difficulty PB or improvement. Varied content and strict comparison identity
serve different purposes; neither should silently change the meaning of stats.

These charts use per-test summaries, not raw input or per-character samples.
They cannot identify particular weak keys or reconstruct speed within a test.
If those features are requested later, discuss additional aggregate counters
explicitly before collecting them. Missing metrics are unavailable, not zero.
Show no-data/insufficient-data states and flat/one-point charts without inventing
values or dividing by zero. Queries return bounded chart points/aggregates and
run on history navigation/filter changes, never on each frame or during typing.

### Typing data and selection

The original starter had 78 words/four snippets and reset a deterministic
sequence on every launch. That limitation is fixed: src/content.rs now embeds
1,598 unique curated ASCII English words and 60 original short exercises, 20
each of Rust/Python/JavaScript, using content/words.txt and content/catalog.rs.
The list is alphabetized, not frequency ranked. No user files or network service
supply text. content/PROVENANCE.md records AI-assisted curation/authorship.
Exercise IDs/versions and pool versions identify historical comparisons.
Language metadata is included; category/difficulty metadata is not yet needed. Review licenses/provenance
for every imported collection before bundling; prefer original authored code
and maintain a small provenance manifest. Never copy another app's dataset
without reviewing its license. All normal practice stays offline.

Use the rand crate for randomized selection; its current documentation reports
0.10.3. Obtain selector entropy fallibly from OS randomness before startup,
then use the in-memory generator for selection and deduplication IDs; inject
seeded generators in tests. Do not sample OS randomness per keystroke. Randomness
does not require reading secrets or environment files. See
[rand documentation](https://docs.rs/rand/latest/rand/).

- Words: draw shuffled blocks from the chosen pool, reshuffle after exhaustion,
  avoid immediate repeated words across block boundaries, and maintain the stream
  as word mode replenishes. Generate a fresh order per session; randomness cannot
  promise that a finite word bank never repeats words or combinations forever.
- Code: shuffle stable snippet IDs into a deck, present each once before
  reshuffling, and avoid repeating the last snippet at the next deck boundary.
  Keep authored indentation/newlines intact; the scoring compiler still skips
  line edges and requires internal spaces/punctuation.
- With history enabled, content_selection stores ordered remaining IDs and
  content_meta stores catalog version and last presented ID for the code pool.
  Reserve an exercise transactionally while preparing ready state, outside active
  timing, so two instances cannot accidentally reserve the same deck entry.
  Persist selection separately from completed sessions: abandoning a shown
  exercise must not cause that exercise to be forced again on the next launch.
- On a catalog update, discard obsolete deck IDs and rebuild an eligible deck
  without changing old result identities. If selection persistence fails, show
  a concise notice and continue with in-memory selection; cross-launch no-repeat
  behavior cannot be promised in that case.
- With --no-history, keep all selection state in memory and use fresh randomness.
  Avoid repeats within that run, but make no cross-launch guarantee because
  reading/writing prior state is explicitly disabled. Purge/clear starts a new
  selection history; a later match to previously removed records is possible.

Do not scan the user's repositories, documents, environment, or secret stores
for typing material. Future user-imported exercises must use explicit paths
and the existing size/control-character/encoding validation boundary.

### Additional acceptance checks

- Uninstall/reinstall retains a temporary test database; explicit purge removes
  owned data only; cancellation and concurrent-open refusal preserve it.
- Dashboard SQL and pure aggregation tests cover weighted accuracy, periods,
  empty days, UTC boundaries, comparison groups, missing metrics, and rolling
  windows. Renderer tests cover no/one/flat series, narrow layouts, and labels.
- Seeded selection tests cover word chunk boundaries, complete code deck coverage,
  deck reset, language eligibility, catalog upgrades, concurrent reservations,
  abandonment, and state across process launches. Test invariants rather than
  claiming random outputs can never coincide across fresh independent runs.
- PTY tests cover History/Progress navigation, restart retention, disabled-state
  privacy, and nonrepeating code presentation with an injected temporary state
  directory. Installed-package tests continue to require no external data files,
  database server, Rust compiler, or network access on the user's machine.
