//! Lightweight view state. Durable message storage belongs exclusively to TDLib.
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};
#[derive(Default)]
pub struct Snapshot {
    pub folders: Vec<Value>,
    pub users: HashMap<i64, Value>,
    pub connection: Value,
}
pub async fn run(mut rx: tokio::sync::broadcast::Receiver<Value>, snapshot: Arc<RwLock<Snapshot>>) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                let mut s = snapshot.write().unwrap();
                match event["@type"].as_str().unwrap_or("") {
                    "updateChatFolders" => {
                        s.folders = event["chat_folders"]
                            .as_array()
                            .cloned()
                            .unwrap_or_default()
                    }
                    "updateUser" => {
                        if let Some(id) = event["user"]["id"].as_i64() {
                            s.users.insert(id, event["user"].clone());
                        }
                    }
                    "updateConnectionState" => s.connection = event["state"].clone(),
                    _ => {}
                }
            }
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                tracing::warn!("view state lagged by {n} updates")
            }
            Err(_) => break,
        }
    }
}
