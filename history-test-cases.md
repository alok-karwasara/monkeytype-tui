# History, progress, and varied content acceptance cases

Test-first specification and execution map. The first executable tests were
written before application code; the initial run failed because APIs were absent.
Implementation now passes the mapped Linux suites below. Later edge cases were
added as the real storage/UI surfaces became available.
All paths/processes in automation must be owned temporary fixtures. Never inspect
user history, environment files, secret stores, or ambient process environments.

## Repository and pure statistics: tests/history.rs

| ID | Required checks |
| --- | --- |
| HIS01 | Create, commit, close, reopen, exact summary/metadata roundtrip. |
| HIS02 | Duplicate ID is idempotent; different data with same ID is rejected. |
| HIS03 | Reject invalid counters, UUID, duration, elapsed, NaN/infinite/range metrics; database remains empty. |
| HIS04 | Zero elapsed preserves unavailable/null metrics; not a zero-speed PB. |
| HIS05 | 123 equal-time records paginate 50/50/23 with no duplicates/gaps. |
| HIS06 | Separate mode, duration, metric version, word-pool version, and exercise comparisons; mixed code has no PB. |
| HIS07 | Weighted accuracy and median/best calculations differ correctly from mean percentages; actual practice duration sums. |
| HIS08 | Bounded chronological chart points; ten-vs-ten pattern requires twenty samples. |
| HIS09 | Inclusive UTC period boundaries, empty days, daily totals, and zero-filled 30-day series. |
| HIS10 | Unknown newer schema and corrupt database are preserved; no silent reset. |
| HIS11 | Clear removes results and selection state in one transaction. |
| HIS12 | Locked writer fails within a short wait; rollback and retry retain previous rows. |
| HIS13 | Purge refuses a live instance, succeeds after close, preserves unrelated files, and is idempotent. |
| HIS14 | New directories/database/lock use private permissions; reject DB, lock, and directory symlinks. |
| HIS15 | SQL-looking metadata cannot execute SQL; schema excludes raw typed/source/key text. |
| HIS16 | Explicit path, XDG absolute/fallback, macOS path, absent home, disabled precedence; no environment discovery needed. |
| HIS17 | Failed migration rolls back; malformed schema, read-only storage, malicious record loads fail nonfatally. |
| HIS18 | Purge/clear cancellation preserves records; readonly/deletion errors are honest; no automatic unlink during upgrade. |

## Content selection: tests/history.rs

| ID | Required checks |
| --- | --- |
| CNT01 | At least 1,000 unique ASCII words and 60 unique original snippets, at least 20 per language, all valid prompts. |
| CNT02 | Injected seed is reproducible; 32-word chunks replenish across deck boundaries without adjacent duplicates. |
| CNT03 | Every code exercise appears once per in-memory deck; no immediate repeat across cycles. |
| CNT04 | Persisted code deck survives close/reopen and interleaved independent connections without duplicate reservation. |
| CNT05 | Unknown IDs/obsolete catalog state rebuild safely without changing historical identities. |
| CNT06 | No-history selection stays in memory; entropy failure, single-entry pools, abandon/settings navigation cannot corrupt selection. |

## Application and renderer: tests/history_app.rs

| ID | Required checks |
| --- | --- |
| HAP01 | One immutable completion effect despite redraw, restart, and quit. |
| HAP02 | Timeout in overlay or coincident with navigation/quit emits once and keeps deadline precedence. |
| HAP03 | Never-started, abandoned, or interrupted sessions emit no completed summary. |
| HAP04 | History roundtrips preserve exact ready prompt/results. |
| HAP05 | Disabled history cannot open; F4 during active typing does not change input/deadline. |
| HAP06 | Recent/progress, mode/duration filters; clear confirmation defaults to cancel; explicit y requests deletion. |
| HAP07 | Empty, one-point, flat graphs, narrow/tiny/zero layouts; terminal background remains inherited. |
| HAP08 | F4–F8 press actions; repeats/releases cannot navigate or delete. |
| HAP09 | Period/scope/metric filters reset pagination; left/right and row scrolling stay within bounds. |
| HAP10 | Unsaved retry preserves ID; clear does not resave old results; global quit/resize still work in overlays. |

## Real processes and PTYs: tests/pty.rs

| ID | Required checks |
| --- | --- |
| H2E01 | Complete/save, Recent/Progress, return to results, quit/relaunch and see retained summary. |
| H2E02 | --no-history overrides an explicit directory; opens no history and creates no state. |
| H2E03 | Abandon/quit incomplete test leaves completed count zero. |
| H2E04 | Blocked storage path still permits typing/results/clean exit, preserves unrelated file, shows Unsaved. |
| H2E05 | Clear cancellation preserves count; confirmed clear shows zero. |
| H2E06 | CLI purge refuses a live app; cancellation preserves DB; confirmed purge removes owned data only. |
| H2E07 | Production binary presents different code identities over four launches using persistent deck. |
| H2E08 | Lock database after ready; completion shows Unsaved; unlock/retry saves once. |
| H2E09 | Normal uninstall/reinstall simulation preserves DB; installed archive saves/reloads without toolchain/system SQLite. |
| H2E10 | Large/malformed/future DB and narrow dashboard do not break cleanup; confirmed purge cancellation/EOF is safe. |
| H2E11 | Seeded 123-result history paginates; speed/accuracy/activity views and resize/filter keys work in a production PTY. |

All PTY cases must check child exit and restored terminal modes/cursor/screen.
Linux execution: `tests/history.rs` has 25 functions, `tests/history_app.rs` has
12, and two internal store migration tests add two. The original 25 engine/UI
functions remain, for 64 named Rust tests total. `tests/pty.rs` passes all 11
H2E IDs plus the original 13 fast IDs; E2E06 separately passes its real 30-second
timeout. H2E09 also runs against a packaged/installed optimized binary through
`tests/packaging.py --archive PATH` (PKG07), including removal/reinstall retention.

HIS17 migration rollback is an internal store test; malformed/read-only data are
integration cases. HIS18 uses a held write lock and a read-only directory to
exercise failed clear/purge without deleting live data. CNT06's entropy failure
is an injected selector error, not a claim that OS randomness was disabled.
HAP07 covers all three graph types at empty/one/flat and narrow/tiny sizes.
HIS15 additionally checks the runtime database bytes for a unique typed marker.
macOS, musl/ARM builds, and real-emulator appearance require their own platform
run; Linux results do not establish those checks. These IDs describe variants,
not 45 independently named unit-test functions or universal coverage.
