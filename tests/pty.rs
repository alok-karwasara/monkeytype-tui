//! Real-process tests. Every worker starts with a fixed, nonsecret environment
//! before portable-pty constructs its command builder; ambient values are never
//! enumerated by this harness or copied into a PTY child.
#[cfg(unix)]
mod unix {
    use portable_pty::{Child, CommandBuilder, PtyPair, PtySize, native_pty_system};
    use std::{
        error::Error,
        fs,
        io::{self, Read, Write},
        path::PathBuf,
        process::{Command, ExitCode},
        sync::mpsc::{self, Receiver},
        thread,
        time::{Duration, Instant},
    };

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;
    const BINARY: &str = env!("CARGO_BIN_EXE_monkeytype-tui");
    const FIXTURE: &str = env!("CARGO_BIN_EXE_fixture-app");

    struct Harness {
        pair: PtyPair,
        child: Box<dyn Child + Send + Sync>,
        writer: Box<dyn Write + Send>,
        receiver: Receiver<Vec<u8>>,
        parser: vt100::Parser,
        output: Vec<u8>,
        baseline: String,
        directory: PathBuf,
        rows: u16,
        columns: u16,
    }

    impl Harness {
        fn new(binary: &str, args: &[&str]) -> TestResult<Self> {
            let pair = native_pty_system().openpty(PtySize {
                rows: 30,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            let baseline = pair
                .master
                .get_termios()
                .map(|t| format!("{t:?}"))
                .ok_or("PTY termios inspection unavailable")?;
            let directory =
                PathBuf::from(format!("/tmp/monkeytype-tui-e2e-{}", std::process::id()));
            fs::create_dir(&directory)?;
            // This worker was already launched with env_clear() by the parent.
            let mut command = CommandBuilder::new(binary);
            command.env_clear();
            command.env("TERM", "xterm-256color");
            command.env("LANG", "C.UTF-8");
            command.env("SHELL", "/bin/sh");
            command.env("PATH", "/usr/bin:/bin");
            command.args(args);
            command.cwd(&directory);
            let mut reader = pair.master.try_clone_reader()?;
            let writer = pair.master.take_writer()?;
            let child = match pair.slave.spawn_command(command) {
                Ok(child) => child,
                Err(error) => {
                    let _ = fs::remove_dir(&directory);
                    return Err(error.into());
                }
            };
            let (sender, receiver) = mpsc::channel();
            thread::spawn(move || {
                let mut bytes = [0; 4096];
                loop {
                    match reader.read(&mut bytes) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if sender.send(bytes[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            });
            Ok(Self {
                pair,
                child,
                writer,
                receiver,
                parser: vt100::Parser::new(30, 100, 0),
                output: Vec::new(),
                baseline,
                directory,
                rows: 30,
                columns: 100,
            })
        }

        fn fixture(name: &str, duration: &str) -> TestResult<Self> {
            Self::new(FIXTURE, &[name, duration])
        }
        fn screen(&self) -> String {
            self.parser.screen().contents()
        }
        fn send(&mut self, input: &[u8]) -> TestResult {
            self.writer.write_all(input)?;
            self.writer.flush()?;
            Ok(())
        }
        fn pump(&mut self, timeout: Duration) {
            if let Ok(bytes) = self.receiver.recv_timeout(timeout) {
                self.parser.process(&bytes);
                if self.output.len() < 2_000_000 {
                    self.output.extend_from_slice(&bytes);
                }
            }
        }
        fn wait(&mut self, description: &str, condition: impl Fn(&Self) -> bool) -> TestResult {
            self.wait_for(description, Duration::from_secs(5), condition)
        }
        fn wait_for(
            &mut self,
            description: &str,
            limit: Duration,
            condition: impl Fn(&Self) -> bool,
        ) -> TestResult {
            let deadline = Instant::now() + limit;
            loop {
                if condition(self) {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(
                        format!("waiting for {description}; screen:\n{}", self.screen()).into(),
                    );
                }
                self.pump(Duration::from_millis(20));
            }
        }
        fn ready(&mut self) -> TestResult {
            self.wait("ready", |h| h.screen().contains("READY"))
        }
        fn complete(&mut self) -> TestResult {
            self.wait("results", |h| h.screen().contains("TEST COMPLETE"))
        }
        fn underlined(&self, text: &str) -> bool {
            (0..self.rows).any(|row| {
                (0..self.columns).any(|column| {
                    self.parser
                        .screen()
                        .cell(row, column)
                        .is_some_and(|c| c.contents() == text && c.underline())
                })
            })
        }
        fn caret_on(&self, text: &str) -> bool {
            (0..self.rows).any(|row| {
                (0..self.columns).any(|column| {
                    self.parser
                        .screen()
                        .cell(row, column)
                        .is_some_and(|c| c.contents() == text && c.inverse())
                })
            })
        }
        fn resize(&mut self, columns: u16, rows: u16) -> TestResult {
            self.pair.master.resize(PtySize {
                rows,
                cols: columns,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            self.parser.screen_mut().set_size(rows, columns);
            self.rows = rows;
            self.columns = columns;
            Ok(())
        }
        fn exit(&mut self, successful: bool) -> TestResult {
            let deadline = Instant::now() + Duration::from_secs(5);
            let status = loop {
                self.pump(Duration::from_millis(10));
                if let Some(status) = self.child.try_wait()? {
                    break status;
                }
                if Instant::now() >= deadline {
                    return Err("child failed to exit".into());
                }
            };
            assert_eq!(
                status.success(),
                successful,
                "{}",
                String::from_utf8_lossy(&self.output)
            );
            let drain = Instant::now() + Duration::from_millis(60);
            while Instant::now() < drain {
                self.pump(Duration::from_millis(5));
            }
            let termios = self
                .pair
                .master
                .get_termios()
                .map(|t| format!("{t:?}"))
                .ok_or("PTY termios no longer inspectable")?;
            assert_eq!(termios, self.baseline, "raw/echo modes were not restored");
            assert!(
                self.output
                    .windows(b"\x1b[?1049l".len())
                    .any(|w| w == b"\x1b[?1049l"),
                "alternate screen not restored"
            );
            assert!(
                self.output
                    .windows(b"\x1b[?25h".len())
                    .any(|w| w == b"\x1b[?25h"),
                "cursor not restored"
            );
            Ok(())
        }
        fn quit(&mut self) -> TestResult {
            self.send(b"\x03")?;
            self.exit(true)
        }
    }

    impl Drop for Harness {
        fn drop(&mut self) {
            if matches!(self.child.try_wait(), Ok(None)) && self.child.kill().is_ok() {
                let _ = self.child.wait();
            }
            let _ = fs::remove_dir(&self.directory);
        }
    }

    fn run_case(case: &str) -> TestResult {
        match case {
            "E2E01" => {
                let mut h = Harness::new(BINARY, &[])?;
                h.ready()?;
                h.quit()?;
            }
            "E2E02" => {
                for mode in ["words", "code"] {
                    for duration in ["30", "60", "120"] {
                        let mut h =
                            Harness::new(BINARY, &["--mode", mode, "--duration", duration])?;
                        h.ready()?;
                        assert!(h.screen().contains(&format!("{mode} / {duration}s")));
                        h.send(b"\x1b")?;
                        h.wait("settings", |h| h.screen().contains("SETTINGS"))?;
                        h.send(b"\x1b[C")?;
                        h.send(b"\r")?;
                        let other = if mode == "words" { "code" } else { "words" };
                        h.wait("applied mode", |h| {
                            h.screen().contains("READY")
                                && h.screen().contains(&format!("{other} / {duration}s"))
                        })?;
                        h.send(b"\x1bOR")?;
                        let next = match duration {
                            "30" => "60",
                            "60" => "120",
                            _ => "30",
                        };
                        h.wait("cycled duration", |h| {
                            h.screen().contains(&format!("{other} / {next}s"))
                        })?;
                        h.quit()?;
                        drop(h);
                    }
                }
            }
            "E2E03" => {
                let mut h = Harness::fixture("lines", "30")?;
                h.ready()?;
                h.send(b"a;b c;")?;
                h.complete()?;
                assert!(h.screen().contains("100.0%") && h.screen().contains("Attempts: 6"));
                assert!(h.screen().contains("snippet complete"));
                h.quit()?;
            }
            "E2E04" => {
                let mut h = Harness::fixture("short", "30")?;
                h.ready()?;
                h.send(b"ax")?;
                h.wait("error underline", |h| h.underlined("x"))?;
                h.send(b"\x7fbc")?;
                h.complete()?;
                assert!(
                    h.screen().contains("75.0%")
                        && h.screen().contains("Uncorrected: 0")
                        && h.screen().contains("Attempts: 4")
                );
                h.quit()?;
            }
            "E2E05" => {
                let mut h = Harness::fixture("space", "30")?;
                h.ready()?;
                h.send(b"ab")?;
                h.wait("internal-space mismatch", |h| h.underlined("b"))?;
                assert!(!h.screen().contains("TEST COMPLETE"));
                h.quit()?;
            }
            "E2E06" => {
                let mut h = Harness::fixture("long", "30")?;
                h.ready()?;
                let start = Instant::now();
                h.send(b"a")?;
                h.wait_for("real 30-second timeout", Duration::from_secs(45), |h| {
                    h.screen().contains("TEST COMPLETE")
                })?;
                assert!(
                    start.elapsed() >= Duration::from_millis(29_800),
                    "timeout too early"
                );
                assert!(
                    h.screen().contains("time limit")
                        && h.screen().contains("30.0s")
                        && h.screen().contains("Attempts: 1")
                );
                h.send(b"bc")?;
                h.pump(Duration::from_millis(60));
                assert!(h.screen().contains("Attempts: 1"));
                h.quit()?;
            }
            "E2E07" => {
                let mut h = Harness::fixture("long", "30")?;
                h.ready()?;
                h.send(b"a")?;
                h.wait("typing", |h| h.screen().contains("TYPING"))?;
                h.resize(30, 10)?;
                h.wait("resize fallback", |h| h.screen().contains("Resize"))?;
                h.send(b"x")?;
                // No textual acknowledgement is possible for ignored tiny-window input.
                // Drain terminal output through a bounded settling interval; exact ordering
                // and timer policies are also covered with injected events in unit tests.
                let settle = Instant::now() + Duration::from_millis(120);
                while Instant::now() < settle {
                    h.pump(Duration::from_millis(10));
                }
                h.resize(100, 30)?;
                h.wait("restored prompt", |h| {
                    h.screen().contains("TYPING") && h.caret_on("b")
                })?;
                h.send(b"bcd")?;
                h.complete()?;
                assert!(h.screen().contains("100.0%") && h.screen().contains("Attempts: 4"));
                h.quit()?;
            }
            "E2E08" => {
                let mut h = Harness::fixture("long", "30")?;
                h.ready()?;
                h.send(b"\x1b[200~abcd\x1b[201~")?;
                h.wait("ready paste rejection", |h| {
                    h.screen().contains("Paste ignored") && h.screen().contains("READY")
                })?;
                h.send(b"a")?;
                h.wait("typing", |h| h.screen().contains("TYPING"))?;
                h.send(b"\x1b[200~bcd\x1b[201~")?;
                h.wait("active paste rejection", |h| {
                    h.screen().contains("Paste ignored") && h.screen().contains("TYPING")
                })?;
                h.send(b"bcd")?;
                h.complete()?;
                assert!(h.screen().contains("Attempts: 4"));
                h.quit()?;
            }
            "E2E09" => {
                let mut h = Harness::fixture("long", "30")?;
                h.ready()?;
                h.send(b"a")?;
                h.wait("typing", |h| h.screen().contains("TYPING"))?;
                h.send(b"\x1b")?;
                h.wait("abandon prompt", |h| h.screen().contains("CONFIRM ABANDON"))?;
                h.send(b"n")?;
                h.wait("resume", |h| {
                    !h.screen().contains("CONFIRM ABANDON") && h.caret_on("b")
                })?;
                h.send(b"\x1b")?;
                h.wait("abandon prompt", |h| h.screen().contains("CONFIRM ABANDON"))?;
                h.send(b"y")?;
                h.ready()?;
                assert!(h.caret_on("a"));
                h.quit()?;
            }
            "E2E10" => {
                let mut h = Harness::fixture("short", "120")?;
                h.ready()?;
                h.send(b"abc")?;
                h.complete()?;
                h.send(b"\r")?;
                h.ready()?;
                assert!(h.screen().contains("code / 120s") && h.caret_on("a"));
                h.quit()?;
            }
            "E2E11" => {
                for failure in ["init", "partial-init", "read", "draw", "panic", "sigterm"] {
                    let mut h = if failure == "sigterm" {
                        Harness::fixture("long", "30")?
                    } else {
                        Harness::new(FIXTURE, &["long", "30", failure])?
                    };
                    if failure == "sigterm" {
                        h.ready()?;
                        let pid = h.child.process_id().ok_or("missing child pid")?;
                        let result = Command::new("/bin/kill")
                            .env_clear()
                            .args(["-TERM", &pid.to_string()])
                            .status()?;
                        assert!(result.success());
                        h.exit(true)?;
                    } else {
                        h.exit(false)?;
                    }
                    drop(h);
                }
            }
            "E2E12" => {
                let output = Command::new(BINARY).env_clear().output()?;
                assert!(!output.status.success());
                assert!(String::from_utf8_lossy(&output.stderr).contains("interactive terminal"));
                assert!(!output.stdout.contains(&0x1b) && !output.stderr.contains(&0x1b));
            }
            "E2E13" => {
                let mut h = Harness::fixture("short", "30")?;
                h.ready()?;
                h.send(b"abx")?;
                h.complete()?;
                assert!(h.screen().contains("66.7%") && h.screen().contains("Uncorrected: 1"));
                h.send(b"\x7f")?;
                h.pump(Duration::from_millis(60));
                assert!(h.screen().contains("TEST COMPLETE"));
                h.quit()?;
            }
            "E2E14" => {
                let mut h = Harness::fixture("words", "30")?;
                h.ready()?;
                h.send(b"red blue")?;
                h.wait("more word content", |h| {
                    h.screen().contains("TYPING") && h.caret_on(" ")
                })?;
                assert!(!h.screen().contains("TEST COMPLETE"));
                h.quit()?;
            }
            _ => return Err(format!("unknown E2E case {case}").into()),
        }
        Ok(())
    }

    pub fn main() -> ExitCode {
        let args: Vec<_> = std::env::args().skip(1).collect();
        if args.first().is_some_and(|a| a == "--worker") {
            let result = args
                .get(1)
                .ok_or_else(|| io::Error::other("worker case required"))
                .map_err(|e| -> Box<dyn Error> { Box::new(e) })
                .and_then(|case| run_case(case));
            return match result {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::FAILURE
                }
            };
        }
        let cases: Vec<&str> = if args.iter().any(|a| a == "--slow") {
            vec!["E2E06"]
        } else if args.first().is_some_and(|a| a == "--case") {
            args.get(1).map_or_else(Vec::new, |s| vec![s.as_str()])
        } else {
            vec![
                "E2E01", "E2E02", "E2E03", "E2E04", "E2E05", "E2E07", "E2E08", "E2E09", "E2E10",
                "E2E11", "E2E12", "E2E13", "E2E14",
            ]
        };
        for case in cases {
            print!("{case} ... ");
            let _ = io::stdout().flush();
            let result = (|| -> TestResult {
                // Standard Command can clear inheritance without enumerating ambient values.
                let mut worker = Command::new(std::env::current_exe()?)
                    .env_clear()
                    .env("PATH", "/usr/bin:/bin")
                    .env("TERM", "xterm-256color")
                    .env("LANG", "C.UTF-8")
                    .env("SHELL", "/bin/sh")
                    .args(["--worker", case])
                    .spawn()?;
                let deadline =
                    Instant::now() + Duration::from_secs(if case == "E2E06" { 55 } else { 35 });
                loop {
                    if let Some(status) = worker.try_wait()? {
                        if status.success() {
                            return Ok(());
                        }
                        return Err(format!("{case} failed").into());
                    }
                    if Instant::now() >= deadline {
                        let _ = worker.kill();
                        let _ = worker.wait();
                        return Err(format!("{case} exceeded worker deadline").into());
                    }
                    thread::sleep(Duration::from_millis(10));
                }
            })();
            if let Err(error) = result {
                eprintln!("FAILED: {error}");
                return ExitCode::FAILURE;
            }
            println!("ok");
        }
        ExitCode::SUCCESS
    }
}

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    unix::main()
}

#[cfg(not(unix))]
fn main() {
    println!("PTY suite skipped: first-release targets are Linux and macOS.");
}
