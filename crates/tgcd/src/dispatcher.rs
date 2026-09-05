//! Event dispatcher — receives TDLib updates and updates the SQLite cache.

use serde_json::Value as JsonValue;
use tracing::{debug, warn};

use crate::cache::Cache;

#[derive(Default)]
pub struct Snapshot {
    pub folders: Vec<JsonValue>,
    pub users: std::collections::HashMap<i64, JsonValue>,
    pub connection: JsonValue,
}

pub async fn run_cache_updater(
    cache: Cache,
    mut rx: tokio::sync::broadcast::Receiver<JsonValue>,
    snapshot: std::sync::Arc<std::sync::RwLock<Snapshot>>,
) {
    loop {
        match rx.recv().await {
            Ok(ev) => {
                {
                    let mut s = snapshot.write().unwrap();
                    match ev["@type"].as_str().unwrap_or("") {
                        "updateChatFolders" => {
                            s.folders = ev["chat_folders"].as_array().cloned().unwrap_or_default()
                        }
                        "updateUser" => {
                            if let Some(id) = ev["user"]["id"].as_i64() {
                                s.users.insert(id, ev["user"].clone());
                            }
                        }
                        "updateConnectionState" => s.connection = ev["state"].clone(),
                        _ => {}
                    }
                }
                if let Err(e) = process_event(&ev, &cache).await {
                    debug!("cache update error: {e}");
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                warn!("cache updater lagged by {n} events");
            }
            Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
        }
    }
}

async fn process_event(ev: &JsonValue, cache: &Cache) -> anyhow::Result<()> {
    let ev_type = ev.get("@type").and_then(|v| v.as_str()).unwrap_or("");

    match ev_type {
        "updateNewMessage" => {
            let msg = &ev["message"];
            let msg_id = msg["id"].as_i64().unwrap_or(0);
            let chat_id = msg["chat_id"].as_i64().unwrap_or(0);
            let sender_id = msg["sender_id"]["user_id"].as_i64();
            let text = msg["content"]["text"]["text"]
                .as_str()
                .or_else(|| msg["content"]["caption"]["text"].as_str());
            let date = msg["date"].as_i64().unwrap_or(0);
            let is_outgoing = msg["is_outgoing"].as_bool().unwrap_or(false);
            let content_type = detect_content_type(msg);

            if chat_id != 0 && msg_id != 0 {
                cache
                    .upsert_message(
                        chat_id,
                        msg_id,
                        sender_id,
                        text,
                        date,
                        is_outgoing,
                        &content_type,
                        msg,
                    )
                    .await?;
            }
        }
        "updateChatLastMessage" => {
            let chat_id = ev["chat_id"].as_i64().unwrap_or(0);
            let msg = &ev["last_message"];
            let msg_id = msg["id"].as_i64().unwrap_or(0);
            if chat_id != 0 && msg_id != 0 {
                cache.update_chat_last_msg(chat_id, msg_id).await?;
            }
        }
        "updateChatTitle" => {
            let chat_id = ev["chat_id"].as_i64().unwrap_or(0);
            let title = ev["title"].as_str().unwrap_or("");
            if chat_id != 0 {
                cache
                    .upsert_chat(chat_id, title, "private", None, 0)
                    .await?;
            }
        }
        "updateUnreadMessageCount" => {
            let chat_id = ev["chat_id"].as_i64().unwrap_or(0);
            let unread = ev["unread_count"].as_i64().unwrap_or(0) as i32;
            if chat_id != 0 {
                cache
                    .upsert_chat(chat_id, "", "private", None, unread)
                    .await?;
            }
        }
        "updateFile" => {
            // Track download progress
            let file_id = ev["file"]["id"].as_i64().unwrap_or(0);
            let local = &ev["file"]["local"];
            let _is_downloading = local["is_downloading_active"].as_bool().unwrap_or(false);
            let is_downloaded = local["is_downloading_completed"].as_bool().unwrap_or(false);
            let path = local["path"].as_str();

            if is_downloaded {
                if let Some(path) = path {
                    debug!("file {file_id} downloaded to {path}");
                }
            }
        }
        _ => {}
    }

    Ok(())
}

fn detect_content_type(msg: &JsonValue) -> String {
    let content = &msg["content"];
    let msg_type = content
        .get("@type")
        .and_then(|v| v.as_str())
        .unwrap_or("messageText");
    match msg_type {
        "messageText" => "text",
        "messagePhoto" => "photo",
        "messageVideo" => "video",
        "messageDocument" => "document",
        "messageSticker" => "sticker",
        "messageVoiceNote" | "messageAudio" => "voice",
        _ => "other",
    }
    .to_string()
}
