#![cfg(unix)]

use std::{os::fd::OwnedFd, os::unix::net::UnixStream, process::Stdio, time::Duration};

use serde_json::{Value, json};
use tokio::{
    io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};

const DEADLINE: Duration = Duration::from_secs(5);

async fn shutdown_with_open_input(signal: &str, socket_stdout: bool) {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_squad"));
    command
        .env_clear()
        .env("HOME", root.path())
        .current_dir(root.path())
        .args(["--data-dir", "room", "mcp"])
        .stdin(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true);
    let socket = if socket_stdout {
        // A socket instead of a pipe forces rmcp's blocking fallback on Linux.
        let (reader, writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        command.stdout(Stdio::from(OwnedFd::from(writer)));
        Some(tokio::net::UnixStream::from_std(reader).unwrap())
    } else {
        command.stdout(Stdio::piped());
        None
    };
    let mut child = command.spawn().unwrap();
    let mut input = child.stdin.take().unwrap();
    let output: Box<dyn AsyncRead + Unpin> = match socket {
        Some(reader) => Box::new(reader),
        None => Box::new(child.stdout.take().unwrap()),
    };
    let mut lines = BufReader::new(output).lines();
    timeout(DEADLINE, async {
        for message in [
            json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
                "protocolVersion": "2025-11-25", "capabilities": {},
                "clientInfo": {"name": "Squad shutdown test", "version": "1"}
            }}),
            json!({"jsonrpc": "2.0", "method": "notifications/initialized"}),
            json!({"jsonrpc": "2.0", "id": 2, "method": "ping"}),
        ] {
            input
                .write_all(format!("{message}\n").as_bytes())
                .await
                .unwrap();
        }
        for id in [1, 2] {
            let line = lines
                .next_line()
                .await
                .unwrap()
                .expect("Squad closed stdout");
            let response: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(response["id"], id, "{response}");
            assert!(response.get("result").is_some(), "{response}");
        }
    })
    .await
    .expect("MCP handshake and ping timed out");

    let signalled = Command::new("kill")
        .args([signal, &child.id().unwrap().to_string()])
        .status()
        .await
        .unwrap();
    assert!(signalled.success());
    // Keeping ownership outside Child makes wait leave the writer open.
    let status = timeout(DEADLINE, child.wait())
        .await
        .expect("Squad did not exit while MCP client kept stdin open")
        .unwrap();
    assert!(status.success(), "{status}");
    drop(input);
}

#[tokio::test]
async fn stdio_sigterm_exits_with_open_input() {
    shutdown_with_open_input("-TERM", false).await;
}

#[tokio::test]
async fn stdio_sigint_exits_with_open_input_on_blocking_fallback() {
    shutdown_with_open_input("-INT", true).await;
}
