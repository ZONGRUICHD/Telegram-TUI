use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::{
    io::Write,
    process::{Command, Stdio},
};
use tg_core::config::TgConfig;
use tg_ipc::transport::Listener;
use tokio_util::codec::{Framed, LengthDelimitedCodec};

async fn mock_call(
    args: Vec<&str>,
    input: Option<&str>,
    expected_method: &str,
    result: Value,
    error: bool,
) -> (std::process::Output, Value) {
    let root = std::env::temp_dir().join(format!("tg-cli-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    let mut config = TgConfig::default();
    config.ipc.socket_path = root.join("rpc.sock");
    config.save_to(&path).unwrap();
    let mut listener = Listener::bind(&config.ipc.socket_path).unwrap();
    let owned: Vec<String> = args.into_iter().map(str::to_owned).collect();
    let input = input.map(str::to_owned);
    let proc = tokio::task::spawn_blocking(move || {
        let mut child = Command::new(env!("CARGO_BIN_EXE_tg"))
            .arg("--config")
            .arg(path)
            .args(owned)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        if let Some(input) = input {
            child
                .stdin
                .take()
                .unwrap()
                .write_all(input.as_bytes())
                .unwrap();
        } else {
            drop(child.stdin.take());
        }
        child.wait_with_output().unwrap()
    });
    let stream = tokio::time::timeout(std::time::Duration::from_secs(5), listener.accept())
        .await
        .unwrap()
        .unwrap();
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    let frame = framed.next().await.unwrap().unwrap();
    let request: Value = serde_json::from_slice(&frame).unwrap();
    assert_eq!(request["method"], expected_method);
    let response = if error {
        json!({"type":"response","id":request["id"],"error":result})
    } else {
        json!({"type":"response","id":request["id"],"result":result})
    };
    framed
        .send(Bytes::from(response.to_string()))
        .await
        .unwrap();
    let out = proc.await.unwrap();
    drop(framed);
    drop(listener);
    std::fs::remove_file(root.join("config.toml")).unwrap();
    std::fs::remove_dir(root).unwrap();
    (out, request)
}

#[tokio::test]
async fn piped_output_and_config_and_pagination() {
    let (out, req) = mock_call(
        vec!["history", "-100123", "--before", "999"],
        None,
        "get_messages",
        json!({"messages":[],"next_from_message_id":null}),
        false,
    )
    .await;
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["ok"], true);
    assert_eq!(req["params"]["from_message_id"], 999);
    assert!(out.stderr.is_empty());
}

#[tokio::test]
async fn stdin_preserves_unicode_and_newlines() {
    let (out, req) = mock_call(
        vec!["--json", "send", "me", "--reply-to", "123", "--stdin"],
        Some("你好\n第二行\n"),
        "send_message",
        json!({"id":-7,"sending_state":{"@type":"messageSendingStatePending"}}),
        false,
    )
    .await;
    assert!(out.status.success());
    assert_eq!(req["params"]["text"], "你好\n第二行\n");
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        result["result"]["sending_state"]["@type"],
        "messageSendingStatePending"
    );
}

#[tokio::test]
async fn telegram_error_is_nonzero_and_structured() {
    let (out, _) = mock_call(
        vec!["me"],
        None,
        "get_me",
        json!({"code":401,"message":"Unauthorized"}),
        true,
    )
    .await;
    assert_eq!(out.status.code(), Some(3));
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], 401);
}

#[test]
fn dry_run_does_not_connect_or_send() {
    let out = Command::new(env!("CARGO_BIN_EXE_tg"))
        .args(["--json", "--dry-run", "send", "me", "hello"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let result: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(result["result"]["method"], "send_message");
    assert_eq!(result["result"]["dry_run"], true);
}
