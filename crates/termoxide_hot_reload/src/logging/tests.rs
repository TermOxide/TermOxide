#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::HashSet,
    env,
    fs,
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU32, Ordering},
    thread,
    time::{Duration, Instant},
};

use log::{Level, Record};

use super::{
    location::{file_name, log_dir},
    logger::{format_line, now},
    *,
};

/// A fresh directory under the system's temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "termoxide-log-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let dir = std::env::temp_dir().join(name);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

#[test]
fn tags_name_their_side() {
    assert_eq!(Tag::Host.to_string(), "[framework:host]");
    assert_eq!(Tag::Child.to_string(), "[framework:child]");
    assert_eq!(Tag::App.to_string(), "[app]");
}

#[test]
fn a_file_name_has_no_colon() {
    let name = file_name(now());

    assert!(!name.contains(':'), "{name}");
    assert!(name.ends_with(&format!("_{}.log", std::process::id())), "{name}");
}

#[test]
fn the_log_directory_is_per_app() {
    let dir = log_dir().unwrap();
    let parts: Vec<_> = dir.components().rev().take(3).map(|part| part.as_os_str().to_owned()).collect();

    assert_eq!(parts[0], "logs");
    assert_eq!(parts[2], "termoxide");
}

#[test]
fn a_line_has_time_level_tag_and_message() {
    let line = format_line(
        &Record::builder()
            .args(format_args!("hello"))
            .level(Level::Warn)
            .target("test")
            .build(),
        Tag::Child,
    );

    assert!(line.ends_with(" WARN  [framework:child] hello"), "{line}");
}

#[test]
fn lines_written_at_once_from_two_handles_stay_whole() {
    let dir = TempDir::new();
    let host = LogFile::create_in(&dir.0).unwrap();
    let child = LogFile::open(host.path().to_owned()).unwrap();
    let path = host.path().to_owned();

    thread::scope(|scope| {
        for (file, name) in [(&host, "host"), (&child, "child")] {
            scope.spawn(move || {
                for i in 0..500 {
                    file.write_line(&format!("{name} {i} {}", "x".repeat(100))).unwrap();
                }
            });
        }
    });

    let contents = fs::read_to_string(Path::new(&path)).unwrap();
    let lines: Vec<_> = contents.lines().collect();
    assert_eq!(lines.len(), 1000);
    assert!(
        lines.iter().all(|line| line.ends_with(&"x".repeat(100))),
        "a line was interleaved"
    );
}

#[test]
fn relaying_ends_with_its_output() {
    relay_output(Cursor::new(b"first\nsecond\r\nno newline".to_vec()), Level::Info)
        .join()
        .unwrap();
}

/// Set on the writer processes: the log file to append to.
const WRITER_LOG_ENV: &str = "TERMOXIDE_LOG_TEST_FILE";
/// Set on the writer processes: the name each puts on its lines.
const WRITER_NAME_ENV: &str = "TERMOXIDE_LOG_TEST_NAME";
/// Set on the writer processes: the file whose appearance means "start".
const WRITER_GO_ENV: &str = "TERMOXIDE_LOG_TEST_GO";
const LINES_PER_WRITER: usize = 500;

fn numbered_line(name: &str, i: usize) -> String { format!("{name} {i} {}", "x".repeat(2000)) }

/// Waits for `go` to exist, so every writer starts at about the same time.
fn wait_for(go: &Path) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !go.exists() {
        assert!(Instant::now() < deadline, "never told to start");
        thread::sleep(Duration::from_millis(1));
    }
}

/// Run as its own process by `lines_from_several_processes_stay_whole`.
/// Does nothing in a normal test run.
#[test]
fn writer_process() {
    let Some(path) = env::var_os(WRITER_LOG_ENV) else {
        return;
    };
    let name = env::var(WRITER_NAME_ENV).unwrap();
    let file = LogFile::open(PathBuf::from(path)).unwrap();

    wait_for(Path::new(&env::var_os(WRITER_GO_ENV).unwrap()));
    for i in 0..LINES_PER_WRITER {
        file.write_line(&numbered_line(&name, i)).unwrap();
    }
}

#[test]
fn lines_from_several_processes_stay_whole() {
    let dir = TempDir::new();
    let go = dir.0.join("go");
    let host = LogFile::create_in(&dir.0).unwrap();

    let writers: Vec<_> = ["first", "second"]
        .into_iter()
        .map(|name| {
            Command::new(env::current_exe().unwrap())
                .args(["logging::tests::writer_process", "--exact", "--quiet"])
                .env(WRITER_LOG_ENV, host.path())
                .env(WRITER_NAME_ENV, name)
                .env(WRITER_GO_ENV, &go)
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect();

    fs::write(&go, "").unwrap();
    for i in 0..LINES_PER_WRITER {
        host.write_line(&numbered_line("host", i)).unwrap();
    }
    for mut writer in writers {
        assert!(writer.wait().unwrap().success(), "a writer process failed");
    }

    let contents = fs::read_to_string(host.path()).unwrap();
    let expected: HashSet<String> = ["host", "first", "second"]
        .into_iter()
        .flat_map(|name| (0..LINES_PER_WRITER).map(move |i| numbered_line(name, i)))
        .collect();
    let found: HashSet<String> = contents.lines().map(str::to_owned).collect();
    assert_eq!(contents.lines().count(), expected.len(), "lines were lost or split");
    assert!(found == expected, "a line was interleaved with another");
}
