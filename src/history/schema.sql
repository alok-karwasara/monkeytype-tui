CREATE TABLE sessions (
    id TEXT PRIMARY KEY NOT NULL CHECK(length(id)=36),
    completed_ms INTEGER NOT NULL CHECK(completed_ms>=0),
    mode TEXT NOT NULL CHECK(mode IN ('words','code')),
    seconds INTEGER NOT NULL CHECK(seconds IN (30,60,120)),
    elapsed_ns INTEGER NOT NULL CHECK(elapsed_ns>=0 AND elapsed_ns<=seconds*1000000000),
    reason TEXT NOT NULL CHECK(reason IN ('timeout','snippet')),
    exercise_id TEXT NOT NULL, exercise_version INTEGER NOT NULL CHECK(exercise_version>0),
    pool TEXT NOT NULL, pool_version INTEGER NOT NULL CHECK(pool_version>0), language TEXT,
    attempts INTEGER NOT NULL CHECK(attempts>0),
    correct_attempts INTEGER NOT NULL CHECK(correct_attempts>=0 AND correct_attempts<=attempts),
    current_correct INTEGER NOT NULL CHECK(current_correct>=0 AND current_correct<=correct_attempts),
    backspaces INTEGER NOT NULL CHECK(backspaces>=0 AND backspaces<=attempts),
    uncorrected INTEGER NOT NULL CHECK(uncorrected>=0 AND uncorrected<=attempts-correct_attempts AND current_correct+uncorrected<=attempts),
    wpm REAL CHECK(wpm>=0), raw_wpm REAL CHECK(raw_wpm>=0), accuracy REAL CHECK(accuracy>=0 AND accuracy<=100),
    metric_version INTEGER NOT NULL CHECK(metric_version>0), app_version TEXT NOT NULL
) STRICT;
CREATE INDEX sessions_recent ON sessions(completed_ms DESC,id DESC);
CREATE INDEX sessions_comparison ON sessions(metric_version,mode,seconds,pool_version,exercise_id,exercise_version,reason);
CREATE TABLE content_selection(position INTEGER PRIMARY KEY,exercise_id TEXT NOT NULL UNIQUE) STRICT;
CREATE TABLE content_meta(singleton INTEGER PRIMARY KEY CHECK(singleton=1),version INTEGER NOT NULL,last_id TEXT NOT NULL) STRICT;
