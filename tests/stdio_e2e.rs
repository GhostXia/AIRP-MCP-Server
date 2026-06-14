//! Cross-process stdio MCP contract test.
//!
//! Spawns the real `airp-mcp mcp` binary and speaks newline-delimited JSON-RPC
//! over its stdio exactly as any MCP client would, then asserts the full
//! `initialize` -> `notifications/initialized` -> `tools/call` lifecycle returns
//! real payloads, not stubs. This pins the generic stdio contract:
//!   A2 one JSON-RPC object per stdout line (newline-delimited),
//!   A3 logs go to stderr only (stdout stays parseable — proven by parsing it),
//!   A4 lifecycle with a non-empty protocolVersion + real serverInfo,
//!   A5 list_characters on an empty data dir succeeds with the empty-state text,
//!   A6 the process exits on its own once stdin closes.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStdin, Command};
use tokio::time::timeout;

/// Cargo hands integration tests the path to the built binary via this env var.
const BIN: &str = env!("CARGO_BIN_EXE_airp-mcp");

type Stdout = tokio::io::Lines<BufReader<tokio::process::ChildStdout>>;

/// Write one newline-delimited JSON-RPC frame to the child's stdin.
async fn send(stdin: &mut ChildStdin, msg: &serde_json::Value) {
    let mut line = serde_json::to_string(msg).unwrap();
    line.push('\n');
    stdin.write_all(line.as_bytes()).await.unwrap();
    stdin.flush().await.unwrap();
}

/// Read stdout frames until one carries the given JSON-RPC id, skipping any
/// notification/log frames. Times out rather than hanging CI on a silent server.
async fn read_response(stdout: &mut Stdout, id: i64) -> serde_json::Value {
    loop {
        let line = timeout(Duration::from_secs(10), stdout.next_line())
            .await
            .expect("timed out waiting for a stdout frame")
            .expect("stdout read error")
            .expect("stdout closed before the response arrived");
        if line.trim().is_empty() {
            continue;
        }
        let frame: serde_json::Value = serde_json::from_str(&line).unwrap_or_else(|e| {
            panic!("each stdout line must be one JSON-RPC object ({e}): {line}")
        });
        if frame.get("id").and_then(|i| i.as_i64()) == Some(id) {
            return frame;
        }
        // Otherwise an unrelated notification frame — keep reading.
    }
}

#[tokio::test]
async fn stdio_handshake_then_tool_call_returns_real_data() {
    let dir = tempfile::TempDir::new().unwrap();

    let mut child = Command::new(BIN)
        .arg("mcp")
        .arg("--data-dir")
        .arg(dir.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        // Inherit the server's stderr so a CI failure can surface its logs. We
        // still parse only the piped stdout, so the stdout-clean check (A3) holds.
        .stderr(Stdio::inherit())
        // Reap the child if an assertion panics before we reach child.wait().
        .kill_on_drop(true)
        .spawn()
        .expect("failed to spawn airp-mcp");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap()).lines();

    // A4: initialize -> real protocolVersion + serverInfo.
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": "airp-e2e", "version": "0" }
            }
        }),
    )
    .await;
    let init = read_response(&mut stdout, 1).await;
    assert!(init.get("error").is_none(), "initialize errored: {init}");
    assert!(
        init["result"]["protocolVersion"]
            .as_str()
            .is_some_and(|s| !s.is_empty()),
        "initialize must return a non-empty protocolVersion: {init}"
    );
    assert_eq!(
        init["result"]["serverInfo"]["name"], "airp-mcp-server",
        "initialize must return the real serverInfo: {init}"
    );

    // A4: the initialized notification carries no response.
    send(
        &mut stdin,
        &serde_json::json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
    )
    .await;

    // A5: the read-only smoke tool succeeds on an empty data dir with real text.
    send(
        &mut stdin,
        &serde_json::json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "list_characters", "arguments": {} }
        }),
    )
    .await;
    let call = read_response(&mut stdout, 2).await;
    assert!(call.get("error").is_none(), "tools/call errored: {call}");
    assert_ne!(
        call["result"]["isError"],
        serde_json::Value::Bool(true),
        "tools/call must not be an error result: {call}"
    );
    let text = call["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tools/call must return text content: {call}"));
    assert!(
        text.contains("No characters"),
        "list_characters on an empty dir must return the real empty-state message, got: {text:?}"
    );

    // A6: closing stdin makes the server exit on its own.
    drop(stdin);
    timeout(Duration::from_secs(10), child.wait())
        .await
        .expect("server did not exit within 10s of stdin closing")
        .expect("failed to await child exit");
}
