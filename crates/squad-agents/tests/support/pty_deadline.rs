//! Each PTY test runs alone in a supervised subprocess. Its deadline includes
//! fixture setup, blocking reads/writes, terminal cleanup and child reaping.
use std::{ffi::OsStr, process::Output};

pub fn run_command(args: &[&OsStr], seconds: f64) -> Output {
    std::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("pty_deadline.py"))
        .arg(seconds.to_string())
        .args(args)
        .output()
        .expect("launch PTY deadline supervisor")
}

/// True in the parent after successful supervision; false inside the test.
pub fn run_test(name: &str) -> bool {
    if std::env::var("SQUAD_PTY_SUPERVISED_TEST").as_deref() == Ok(name) {
        return false;
    }
    let result = std::process::Command::new("python3")
        .arg("-c")
        .arg(include_str!("pty_deadline.py"))
        .arg("60")
        .arg(std::env::current_exe().expect("PTY test binary"))
        .args(["--exact", name, "--nocapture"])
        .env("SQUAD_PTY_SUPERVISED_TEST", name)
        .output()
        .expect("launch PTY deadline supervisor");
    assert!(
        result.status.success(),
        "PTY test {name} failed under its 60s overall deadline ({:?}):\n{}\n{}",
        result.status,
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    true
}
