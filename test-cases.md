# monkeytype-tui: test cases

Status: catalog with implemented automated coverage. Linux validation passes
for 25 Rust test functions, 56 fixture examples, and all 14 PTY scenario IDs.
Functions may cover multiple catalog IDs; this is not a claim that all 98 catalog
rows have independent executable assertions. Manual font/theme and macOS checks
remain pending.
Source of truth: [system-design.md](system-design.md), especially sections 6–10.

## 1. How to use this catalog

Implement these cases alongside the corresponding feature, preserving case IDs
in test names or annotations. Test observable results, not a particular struct
layout or screen coordinate. Do not create passing placeholder tests.

- **C**: checks confirmed requirements or established project instructions.
- **P**: checks a proposed contract in the system design; keep this status until
  that contract is accepted. A prepared fixture does not settle a product choice.
- **P0**: correctness, core behavior, or cleanup required before release.
- **P1**: robustness, presentation, or additional platform validation.

The implementation provisionally starts with words / 60 seconds. Behavioral tests
still select a duration explicitly instead of treating this as a permanent default. Proposed ASCII scope,
tab expansion, correction rules, formulas, automatic line transitions, shortcut
bindings, and small-window behavior remain labeled accordingly.

Prepared data:

- [typing-cases.json](tests/fixtures/typing-cases.json): prompt, session, timing,
  and metric examples. Contains expected values, not an engine implementation.
- [scenarios.json](tests/e2e/scenarios.json): ordered real-terminal test steps and
  assertions. Contains scenario specifications, not an executable PTY runner.

Unless stated otherwise, unit tests use injected monotonic durations from a
fixed origin and a 30-second limit. No real sleeps. Expected numeric metrics
use the proposed formulas from the design; compare unrounded values with a
small tolerance and test user-visible rounding separately.

## 2. Reusable fixtures and terminology

`<BS>` means Backspace, `SP` means a required U+0020 space, `LF` a source newline,
and `TAB` a source formatting tab. JSON strings use their normal `\n`, `\r\n`,
and `\t` escapes. These are source characters, not shell commands to execute.

| Fixture | Source | Required stream |
| --- | --- | --- |
| F-SPACE | `    let n = 1;   ` | `let n = 1;` |
| F-LINES | `a;` + LF + four spaces + `b c;` | `a;b c;` under automatic line transitions (P). |
| F-CORRECT | `abc` | `abc` |
| F-SYMBOLS | `fn f(x: i32) { x + 1; }` | Identical characters, including internal spaces. |
| F-WORDS | `red blue` | `red blue`; word separator is required. |

Counters used in assertions:

- `A`: all accepted character attempts, including retyping and internal spaces.
- `K`: historically correct attempts; correction does not erase this count.
- `C`: correctly matched positions remaining in the current typed prefix.
- `U`: incorrect positions remaining in that prefix.
- `errors = A - K`; `cursor = current_prefix_length = C + U`.

## 3. Prompt compilation unit cases

All are P0 except tab/encoding extensions explicitly marked P1.

| ID | Contract | Input / setup | Expected result |
| --- | --- | --- | --- |
| P01 | C | F-SPACE in code mode | Required stream `let n = 1;`; leading four and trailing three spaces remain display-only. |
| P02 | C | `let  n = 1;` | Both spaces between `let` and `n` are required; do not collapse whitespace. |
| P03 | C | `println!("a b");` | Space inside the string is required; punctuation is preserved. |
| P04 | P | F-LINES | Required stream `a;b c;`; cursor jumps to the next line without Enter or an inserted separator. |
| P05 | P | Four spaces + `a;` + CRLF + four spaces + `b;` + CRLF | Display normalizes CRLF to LF; required stream `a;b;`; no `\r` scored positions. |
| P06 | P | Empty/blank lines surrounding `a;` and `b;` | Blank lines remain visually present; required stream `a;b;`. |
| P07 | C | F-SYMBOLS | Every colon, parenthesis, brace, operator, semicolon, and internal space has a required position. |
| P08 | C | F-WORDS in word mode | Required stream includes the separator; word mode does not inherit code edge trimming. |
| P09 | C | `""`, spaces only, or blank lines only | Preparation fails with an empty-scored-prompt error; timer never starts. |
| P10 | C | Source containing ESC, NUL, or DEL | Preparation fails; control sequences cannot reach terminal output. |
| P11 | P, P1 | Leading TAB and an internal TAB with four-column tab stops | Tabs expand deterministically; leading expansion is skipped, internal expansion becomes required spaces. |
| P12 | P, P1 | Non-ASCII corpus input under the proposed ASCII policy | Explicit unsupported-source error; no partial prompt or byte-splitting. Revise if Unicode is included. |
| P13 | C | Leading/trailing spaces with a line of punctuation only | Symbols remain scored; edge spaces do not become attempts. |
| P14 | P | Source ends with LF versus no final LF | Same required stream; display preserves the chosen normalized line structure. |

Mapping assertions apply to P01–P08, P11, P13–P14: every required entry points
to the matching character in normalized display source; mappings are ordered,
valid, and exclude skipped formatting. Example F-LINES maps `a` to row 0,
column 0 and `b` to row 1, column 4. Cursor indices are not display columns.

## 4. Typing and correction unit cases

All are P0. Cases relying on the positional correction/completion proposal are P.

| ID | Contract | Actions | Expected result |
| --- | --- | --- | --- |
| ENG01 | C | F-SPACE: type `let n = 1;` | No indentation/trailing-space input is needed; all required positions consumed with no errors. |
| ENG02 | C | F-SYMBOLS: type exact input | Symbols and internal spaces count as attempts; all match. |
| ENG03 | P | Target `abc`; type `a`, `x` | `cursor=2`, `A=2`, `K=1`, `C=1`, `U=1`; wrong position is marked; `c` remains pending. |
| ENG04 | P | Continue ENG03 with `<BS>`, `b` | `cursor=2`, `A=3`, `K=2`, `C=2`, `U=0`; historical errors remain 1; session is still running. |
| ENG05 | P | Target `abc`; type `a`, `b`, `<BS>` | Current prefix is `a`; `A=2`, `K=2`, `C=1`; no new attempt/error for Backspace. |
| ENG06 | P | Backspace before first input | Remains ready, cursor zero, counters unchanged, timer not started. |
| ENG07 | P | Source `a` + LF + four spaces + `bc`; type `a`, `b`, `<BS>` | Prefix `a`; next position is displayed `b` on the second line; formatting is skipped in reverse too. |
| ENG08 | P | Target `a b`; type `a`, `b`, `x` | Missing space is not forgiven: final prefix `abx`, `A=3`, `K=1`, `C=1`, `U=2`; snippet exhausted. |
| ENG09 | P | Target `abc`; type `a`, `b`, `x` | Completion occurs with one uncorrected error; do not wait for perfect accuracy. |
| ENG10 | P | After ENG09, send `<BS>` or another character | Final report and completion time remain immutable. Results-screen commands are separate app actions. |
| ENG11 | P | Target `abc`; type `x`, `<BS>`, `<BS>`, `a` | Prefix `a`, `A=2`, `K=1`, `C=1`, `U=0`; extra Backspace at zero is safe. |
| ENG12 | C | Seeded word mode reaches initial buffer end before timeout | Append words with exactly one scored separator; preserve prefix and mapping; do not end early. |

The machine-readable cases use small equivalent sources where useful. Completion
fixtures must not schedule a correction after the final position was consumed.

## 5. Clock and finish-boundary unit cases

TIME03–TIME06, TIME08, TIME11 are P because they specify exact boundary/finish
semantics. Others check established timing requirements. All are P0.

Run TIME01–TIME07 for each `limit_ms ∈ {30000, 60000, 120000}`. Start the session
at origin offset 10,000 ms with target `abcd`, unless stated otherwise.

| ID | Setup / actions | Expected result |
| --- | --- | --- |
| TIME01 | Wait on ready screen for 20 seconds without accepted text | No start time or elapsed test time; settings/formatting events do not start it. |
| TIME02 | First character is wrong | Starts at that action's timestamp; exactly one attempt/error. |
| TIME03 | Process another character at `start + limit - 1 ms` | Character accepted and counted; not timed out yet. |
| TIME04 | Process another character at `start + limit` | Timeout wins; character is not accepted. |
| TIME05 | Process another character at `start + limit + 1 ms` | Timeout; unchanged prefix/counters. |
| TIME06 | Next clock sample arrives 250 ms late | Freeze elapsed at selected limit, not limit + 250 ms. |
| TIME07 | No further input after the first character | Clock update alone finalizes timeout; timer never relies on another key. |
| TIME08 | Final code position consumed 1 ms before deadline | Finish `SnippetExhausted` at actual elapsed time. |
| TIME09 | First character at offset 10,000 ms; inspect at 10,750 ms | Elapsed 750 ms; pre-start idle time is excluded. |
| TIME10 | Resize, redraw, release, or rejected paste | Does not reset/extend the active deadline. |
| TIME11 | Code final character processed exactly at deadline | `Timeout`, final character uncounted; no double finish/report. |
| TIME12 | Limit expires while abandon dialog is open (P) | Results replace dialog; cancel cannot resume the session. |
| TIME13 | Nonuniform clock updates (e.g. 7, 18, 203, 981 ms) | Elapsed matches supplied monotonic timestamps, not number of ticks. |

## 6. Metric unit cases

All are P0/P: exact formulas are proposals. Preserve enough calculation precision;
UI display rounding is a separate assertion. The `target=abc` correction example
is a live session with `c` pending, not a completed two-character snippet.

| ID | Counters / elapsed | Expected result |
| --- | --- | --- |
| MET01 | `A=0, K=0, C=0, U=0`, ready | Rate/accuracy placeholders; errors 0; never NaN/infinity. |
| MET02 | First accepted key at elapsed zero | WPM/raw WPM placeholders; zero elapsed is not replaced with an invented epsilon. Accuracy presentation follows the zero-time contract in the design. |
| MET03 | `A=3, K=2, C=2, U=0`, 10 sec | Accuracy 66.666…%, errors 1, raw WPM 3.6, WPM 2.4. |
| MET04 | `A=3, K=3, C=3, U=0`, 2 sec | Accuracy 100%, errors 0, raw WPM 18, WPM 18. |
| MET05 | `A=3, K=0, C=0, U=3`, 15 sec | Accuracy 0%, errors 3, raw WPM 2.4, WPM 0. |
| MET06 | `A=2, K=2, C=1, U=0`, 5 sec | Accuracy 100%, errors 0, raw WPM 4.8, WPM 2.4; deleted progress does not inflate WPM. |
| MET07 | Same required stream in differently indented source | Identical attempts and metrics for identical timed inputs. |
| MET08 | Score `a b` exactly | Three required characters, including internal space; formatting-only characters excluded. |
| MET09 | Correct an error, then backspace a correct character | Historical accuracy/errors remain historical; current correct progress changes separately. |

## 7. Input normalization unit cases

P0 unless P1 indicated. Inject event objects directly; test terminal-byte parsing
separately through the PTY scenarios. This avoids assuming every terminal emits
the same physical key sequences.

| ID | Contract | Event | Expected action / outcome |
| --- | --- | --- | --- |
| IN01 | C | Printable key press | One typing action, never two. |
| IN02 | C | Matching key release | No typing action. |
| IN03 | P | Text/backspace Repeat event | One corresponding action; no duplicate Press+Release count. |
| IN04 | P | Repeated menu command | No repeated screen transition. |
| IN05 | C | `q`, `:`, `[`, `]`, `{`, `}`, `(`, `)` during typing | Ordinary typed characters; never global quit/menu shortcuts. |
| IN06 | P | Enter or Tab during practice | Ignored formatting input; no progress, attempt, or start. Internal spaces still require Space. |
| IN07 | P | Reported bracketed Paste in ready/running | No typing progress or timer start from pasted text. |
| IN08 | C | Ctrl+C on each screen/overlay | Quit action; route through cleanup. |
| IN09 | P | Non-ASCII key under initial ASCII proposal | Visible unsupported-input notice; no scoring or cursor movement. |
| IN10 | P | Ctrl+Z | No raw-mode shell suspension; show unsupported-suspend notice. |
| IN11 | C | Resize event | New viewport dimensions only; expected source and counters unchanged. |
| IN12 | P1, C | Fast alternating printable presses/releases | Accepted characters retain order; deadline checks are not starved. |

Reported Paste can be rejected; unreported pasted text is not reliably
distinguishable from typing. No test should assert perfect anti-cheat detection.

## 8. Application transition tests

All are P0/P except confirmed option availability. Drive app actions and injected
time; no terminal is required.

| ID | Actions | Expected result |
| --- | --- | --- |
| APP01 | Open settings from ready; select Words/Code | Both modes available; applying prepares the selected mode (C). |
| APP02 | Select each 30/60/120-second option; apply | Exactly selected duration used by next test (C). |
| APP03 | Edit settings then cancel | Previously committed mode/duration preserved; no session starts. |
| APP04 | Esc while running; cancel abandon | Same prefix/counters/start time; elapsed time continues. |
| APP05 | Esc while running; confirm abandon | Fresh ready session; abandoned run is not recorded as completed. |
| APP06 | Finish; Enter for another test | Ready with selected mode/duration; counters/report reset; no automatic timer start. |
| APP07 | Finish; open settings | Immutable report remains unchanged; settings prepare a fresh test. |
| APP08 | Timeout and queued printable input in one loop iteration | Exactly one report; queued text cannot mutate finished engine state. |
| APP09 | Results/ready screen receives character `q` | Does not trigger an undocumented global exit; use defined shortcuts only. |

## 9. Rendering tests using TestBackend

P0 for UI01–UI06; P1 for UI07–UI10. Choose fixed clocks and fixtures. Assert
meaningful regions and styles rather than coupling all tests to one snapshot.

| ID | Contract | View | Expected result |
| --- | --- | --- | --- |
| UI01 | C | Instrument at 100×30 cells | Mode, selected duration, time, WPM, accuracy, errors, prompt, and shortcut hints visible. |
| UI02 | C | Terminal-native appearance | Foreground/background inherit terminal defaults; no forced RGB background/font configuration. |
| UI03 | C | One wrong position | Error carries underline/text cue in addition to any color; remains identifiable in monochrome. |
| UI04 | P | Caret crosses code line boundary | Caret appears at next mapped character, not indentation; source line structure remains visible. |
| UI05 | C | Resize 100×30 → 60×18 → 100×30 | No panic/out-of-bounds access; scoring/state unchanged; display reflows appropriately. |
| UI06 | P | 1×1, 0×0, and below chosen minimum | Safe fallback or empty frame; no panic; preserve session state. |
| UI07 | P | Small terminal during active test | Fallback shows when possible; text input is ignored without scoring; deadline continues. |
| UI08 | P | Long snippet and caret near bottom | Viewport follows caret; skipped formatting and previous error styles stay aligned. |
| UI09 | P | Finished report | Frozen elapsed, reason, accuracy/errors, and correctly labeled WPM/raw WPM. |
| UI10 | P | Abandon/settings overlay | Readable visible focus and hints; settings do not accidentally cover a running scored state. |

The implemented initial minimum is 40×12. Rendering tests cover 40×12, 1×1, 0×0,
and larger sizes; product review may revise the minimum.

## 10. PTY E2E scenarios

Each scenario below is specified in [scenarios.json](tests/e2e/scenarios.json)
and implemented in `tests/pty.rs`. All 14 IDs pass on Linux (E2E06 is the separate
slow suite). macOS execution remains pending.

| ID | Priority / contract | User workflow | Observable result |
| --- | --- | --- | --- |
| E2E01 | P0, C | Launch in PTY; wait ready; Ctrl+C | App appears and exits cleanly; raw/echo state restored. |
| E2E02 | P0, C | Choose each mode × each duration | Six settings combinations produce the requested test configuration. |
| E2E03 | P0, mixed | Type trimmed multiline code, including internal spaces and symbols | Formatting remains visible; no indentation or Enter needed; completion results before timeout. |
| E2E04 | P0, P | Wrong nonfinal character; Backspace; correct; finish | Error mark appears then clears; historical error remains in results. |
| E2E05 | P0, P | Omit an internal space | Mismatch is visible/counts as an error; app does not silently skip the space. |
| E2E06 | P0, C | Select 30 sec; start; stop input | Real timeout appears; extra typing cannot change the report. |
| E2E07 | P0, P | Resize active session normal → tiny → normal | Prefix survives; tested small-window input/time policy applied. |
| E2E08 | P0, P | Send bracketed paste in ready and active states | Paste does not start/advance the test; a normal subsequent key works. |
| E2E09 | P0, P | Open abandon dialog; cancel; then abandon | Cancel retains progress and elapsed; confirm yields a fresh unstarted test. |
| E2E10 | P0, P | Finish and restart | Counters reset; mode/duration preserved; timer awaits first input. |
| E2E11 | P0, mixed | Quit, graceful SIGTERM, induced I/O failure, and unwinding panic | Cleanup attempted/restored as applicable; errors printed after leaving raw mode. |
| E2E12 | P1, C | Launch binary with piped/nonterminal I/O | Plain supported-error path; no terminal-mode escape pollution. |
| E2E13 | P0, P | Type an incorrect final character | Results appear with an uncorrected error; Backspace cannot reopen test. |
| E2E14 | P1, P | Complete initial word chunk before timeout | More words appear; progress preserved; word mode keeps running. |

### Harness requirements

1. Launch the actual compiled binary via a new PTY; never send tests into the
   user's active shell. Use a temporary working directory and fixture corpus.
2. Select deterministic bundled fixtures through the separate `fixture-app`
   binary, enabled only with `test-support`. Production CLI timing is unchanged;
   there is no hidden clock bypass.
3. Use a VT/ANSI screen parser. Await semantic screen conditions (Ready, Results,
   visible error mark) with bounded deadlines, not arbitrary sleep chains.
4. Send ordinary key bytes at a modest rate. Reported Paste is tested with its
   framing bytes; sequential characters sent through a PTY remain keystroke input.
5. Snapshot relevant termios raw/echo settings before and after. Retain the slave
   descriptor for inspection. Also inspect alternate-screen/cursor restore output.
6. Use a real 30-second clock for E2E06. Suggested test-runner ceiling: 45 seconds
   after the first accepted input, with separate bounded startup/exit waits.
   Verify timeout is not significantly early; use a documented scheduling
   tolerance, not millisecond-exact PTY screen timing.
7. A dev-only failure executable may exercise the same lifecycle wrapper for
   injected failures. Mark this as lifecycle integration coverage; it is not
   proof that the production binary's complete UI passed an induced-panic test.
8. Terminate only child processes created by the harness, including on assertion
   failure. Do not inspect process environments or secret configuration to debug.
   If child variables are needed, supply explicit nonsecret values; do not dump
   or copy the ambient environment into logs or artifacts.

Do not assert restoration after SIGKILL or power loss: application cleanup cannot
run there. A PTY also cannot prove a real emulator's chosen font or color palette.

## 11. Property and manual checks

### Generated action-sequence invariants (P0/P)

For bounded prompts and nondecreasing timestamps, generate characters,
Backspaces, and clock updates. Verify:

- `0 <= cursor <= required_length` and `C + U == cursor`.
- `0 <= K <= A`, `errors == A - K`, and `C <= K`.
- Every required position maps to a matching character in normalized display.
- Formatting-only changes to code edges do not change required input/metrics.
- Finished reports remain immutable under later engine input/time updates.
- No panic or arithmetic underflow from repeated Backspace, empty views, or
  timeout at an exact boundary.

Use seeded reproducible generation. This is optional tooling around required
invariants, not a requirement to add a framework before basic examples work.

### Manual emulator checks (P1)

| ID | Setup | Check |
| --- | --- | --- |
| MAN01 | A user's dark-theme terminal | Uses its background/foreground; error/caret/status remain readable. |
| MAN02 | A light-theme terminal | Same behavior without forced dark backgrounds. |
| MAN03 | Change terminal font and size | App follows emulator choices; no downloaded font/custom configuration needed. |
| MAN04 | Linux and macOS terminal sessions | Keyboard, resize, cursor, normal exit, and error recovery behave correctly. |
| MAN05 | Restricted/monochrome palette | Error meaning remains visible without color alone. |

Use controlled test terminal profiles. Never inspect the user's configuration,
environment files, or secrets to discover their theme/font.

## 12. Execution and release checklist

Execute the implemented checks:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --locked
```

Run the real process scenarios separately:

```sh
cargo test --locked --features test-support --test pty
cargo test --locked --features test-support --test pty -- --slow
```

`tests/pty.rs` uses a custom runner, so `-- --case E2E07` selects one scenario.
The driver creates a cleared-environment worker before constructing a PTY
command. Failure injection uses the feature-gated `fixture-app` binary with the
shared runtime and lifecycle wrapper, not hidden production flags.

Release evidence should record case IDs, implementation revision, platform,
pass/fail/skip status, and relevant nonsecret failure details. P0 failures block
release. Cases relying on proposals need a resolved contract before their
pass/fail status becomes an acceptance claim. The provisional default, ASCII scope, and 40×12 minimum are implemented initial
policies, still subject to product review.

Current evidence:

| Suite | Implementation | Linux result |
| --- | --- | --- |
| Prompt/session/timing/metrics | `tests/engine.rs`: 9 functions, 56 data examples, generated invariants | Passed |
| Input/application/CLI transitions | `tests/transitions.rs`: 10 functions | Passed |
| Rendering and viewport | `tests/rendering.rs`: 6 functions | Passed |
| Fast real-process PTY | `tests/pty.rs`: 13 scenario IDs, including variants | Passed |
| Real-time expiry | `tests/pty.rs --slow`: E2E06, real 30 seconds | Passed |
| Format / Clippy with denied warnings | Cargo checks | Passed |
| macOS and actual emulator appearance | Manual/platform validation | Not run |

The PTY resize scenario caught and verified a fix for stale input dimensions.
A unit regression also checks that a deadline-coincident Enter/Esc is not
reinterpreted as navigation away from an expiring session's results.
