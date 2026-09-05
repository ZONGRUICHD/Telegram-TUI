//! SQLite cache for chats, messages, and downloads.

#![allow(dead_code)]

use anyhow::Result;
use serde_json::Value as JsonValue;
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions};
use tracing::info;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS chats (
    id INTEGER PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    kind TEXT NOT NULL DEFAULT 'private',
    last_message_id INTEGER,
    unread_count INTEGER DEFAULT 0,
    updated_at INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    chat_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    sender_id INTEGER,
    text TEXT,
    date INTEGER NOT NULL,
    is_outgoing BOOLEAN NOT NULL DEFAULT 0,
    content_type TEXT NOT NULL DEFAULT 'text',
    raw_json TEXT,
    PRIMARY KEY (chat_id, message_id)
);
CREATE INDEX IF NOT EXISTS idx_messages_chat_date ON messages(chat_id, date DESC);

CREATE TABLE IF NOT EXISTS downloads (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    chat_id INTEGER NOT NULL,
    message_id INTEGER NOT NULL,
    file_id INTEGER NOT NULL,
    local_path TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
"#;

#[derive(Clone)]
pub struct Cache {
    pool: SqlitePool,
}

impl Cache {
    pub async fn new(path: &std::path::Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await?;
        sqlx::query(SCHEMA).execute(&pool).await?;
        info!("SQLite cache opened at {}", path.display());
        Ok(Self { pool })
    }

    pub async fn upsert_chat(
        &self,
        id: i64,
        title: &str,
        kind: &str,
        last_msg_id: Option<i64>,
        unread: i32,
    ) -> Result<()> {
        let now = now_unix();
        sqlx::query(
            "INSERT INTO chats (id, title, kind, last_message_id, unread_count, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                kind = excluded.kind,
                last_message_id = COALESCE(excluded.last_message_id, chats.last_message_id),
                unread_count = excluded.unread_count,
                updated_at = excluded.updated_at",
        )
        .bind(id)
        .bind(title)
        .bind(kind)
        .bind(last_msg_id)
        .bind(unread)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn upsert_message(
        &self,
        chat_id: i64,
        msg_id: i64,
        sender_id: Option<i64>,
        text: Option<&str>,
        date: i64,
        is_outgoing: bool,
        content_type: &str,
        raw: &JsonValue,
    ) -> Result<()> {
        let raw_str = serde_json::to_string(raw)?;
        sqlx::query(
            "INSERT INTO messages (chat_id, message_id, sender_id, text, date, is_outgoing, content_type, raw_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(chat_id, message_id) DO UPDATE SET
                sender_id = excluded.sender_id,
                text = excluded.text,
                is_outgoing = excluded.is_outgoing,
                content_type = excluded.content_type,
                raw_json = excluded.raw_json"
        )
        .bind(chat_id).bind(msg_id).bind(sender_id).bind(text)
        .bind(date).bind(is_outgoing).bind(content_type).bind(raw_str)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get_chats(&self, limit: i64) -> Result<Vec<JsonChat>> {
        Ok(sqlx::query_as::<_, JsonChat>(
            "SELECT id, title, kind, last_message_id, unread_count, updated_at FROM chats ORDER BY updated_at DESC LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool).await?)
    }

    pub async fn get_messages(&self, chat_id: i64, limit: i64) -> Result<Vec<JsonMessage>> {
        Ok(sqlx::query_as::<_, JsonMessage>(
            "SELECT chat_id, message_id, sender_id, text, date, is_outgoing, content_type FROM messages WHERE chat_id = ? ORDER BY date DESC LIMIT ?"
        )
        .bind(chat_id).bind(limit)
        .fetch_all(&self.pool).await?)
    }

    /// Update only last_message_id without touching title or other fields.
    pub async fn update_chat_last_msg(&self, chat_id: i64, msg_id: i64) -> Result<()> {
        let now = now_unix();
        sqlx::query("UPDATE chats SET last_message_id = ?, updated_at = ? WHERE id = ?")
            .bind(msg_id)
            .bind(now)
            .bind(chat_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn search_messages(
        &self,
        chat_id: i64,
        query: &str,
        limit: i64,
    ) -> Result<Vec<JsonMessage>> {
        // Escape LIKE wildcards to prevent injection
        let escaped = query
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let pattern = format!("%{escaped}%");
        Ok(sqlx::query_as::<_, JsonMessage>(
            "SELECT chat_id, message_id, sender_id, text, date, is_outgoing, content_type FROM messages WHERE chat_id = ? AND text LIKE ? ESCAPE '\\' ORDER BY date DESC LIMIT ?"
        )
        .bind(chat_id).bind(pattern).bind(limit)
        .fetch_all(&self.pool).await?)
    }

    pub async fn add_download(&self, chat_id: i64, msg_id: i64, file_id: i64) -> Result<i64> {
        let now = now_unix();
        let result = sqlx::query(
            "INSERT INTO downloads (chat_id, message_id, file_id, status, created_at) VALUES (?, ?, ?, 'pending', ?)"
        )
        .bind(chat_id).bind(msg_id).bind(file_id).bind(now)
        .execute(&self.pool).await?;
        Ok(result.last_insert_rowid())
    }

    pub async fn update_download_status(
        &self,
        id: i64,
        status: &str,
        local_path: Option<&str>,
    ) -> Result<()> {
        sqlx::query("UPDATE downloads SET status = ?, local_path = ? WHERE id = ?")
            .bind(status)
            .bind(local_path)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct JsonChat {
    pub id: i64,
    pub title: String,
    pub kind: String,
    pub last_message_id: Option<i64>,
    pub unread_count: i32,
    pub updated_at: i64,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
pub struct JsonMessage {
    pub chat_id: i64,
    pub message_id: i64,
    pub sender_id: Option<i64>,
    pub text: Option<String>,
    pub date: i64,
    pub is_outgoing: bool,
    pub content_type: String,
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}
