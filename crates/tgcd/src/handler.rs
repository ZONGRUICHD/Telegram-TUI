//! Request handler — maps IPC requests to TDLib queries.

use anyhow::Result;
use serde_json::Value as JsonValue;
use tokio::sync::broadcast;

use tg_core::config::TgConfig;
use tg_ipc::protocol::{methods, Request, Response, RpcError};

use crate::cache::Cache;
use crate::tdlib;

pub struct AppState {
    pub config: TgConfig,
    pub td: tg_tdjson::TdClient,
    #[allow(dead_code)]
    pub cache: Cache,
    pub updates_tx: broadcast::Sender<JsonValue>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
}

pub async fn handle_request(req: Request, state: &AppState) -> Response {
    let result = dispatch(&req.method, &req.params, state).await;
    match result {
        Ok(val) => Response {
            id: req.id,
            result: Some(val),
            error: None,
        },
        Err(e) => Response {
            id: req.id,
            result: None,
            error: Some(RpcError {
                code: match e.downcast_ref::<tg_core::error::TgError>() {
                    Some(tg_core::error::TgError::Tdlib { code, .. }) => *code,
                    _ => -1,
                },
                message: e.to_string(),
            }),
        },
    }
}

async fn dispatch(method: &str, params: &JsonValue, state: &AppState) -> Result<JsonValue> {
    match method {
        methods::GET_STATUS => {
            let auth = tdlib::query(&state.td, serde_json::json!({"@type":"getAuthorizationState"})).await?;
            Ok(serde_json::json!({
                "socket": state.config.ipc.socket_path,
                "authorization": auth,
                "version": env!("CARGO_PKG_VERSION"),
                "pid": std::process::id(),
            }))
        }

        methods::GET_ME => {
            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "getMe"
            })).await?;
            Ok(resp)
        }

        methods::LIST_DIALOGS => {
            let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(20);

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "getChats",
                "chat_list": {"@type": "chatListMain"},
                "limit": limit
            })).await?;

            // getChats returns { "chat_ids": [id1, id2, ...] }
            // Expand each id into full chat info
            if let Some(ids) = resp.get("chat_ids").and_then(|v| v.as_array()) {
                let mut chats = Vec::new();
                for id in ids {
                    if let Some(chat_id) = id.as_i64() {
                        if let Ok(chat) = tdlib::query(&state.td, serde_json::json!({
                            "@type": "getChat",
                            "chat_id": chat_id
                        })).await {
                            chats.push(chat);
                        }
                    }
                }
                Ok(serde_json::Value::Array(chats))
            } else {
                Ok(resp)
            }
        }

        methods::GET_MESSAGES => {
            let chat_id = params["chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing chat_id"))?;
            let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "getChatHistory",
                "chat_id": chat_id,
                "from_message_id": 0,
                "offset": 0,
                "limit": limit,
                "only_local": false
            })).await?;
            Ok(resp)
        }

        methods::SEND_MESSAGE => {
            let chat_id = params["chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing chat_id"))?;
            let text = params["text"].as_str()
                .ok_or_else(|| anyhow::anyhow!("missing text"))?;

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "sendMessage",
                "chat_id": chat_id,
                "input_message_content": {
                    "@type": "inputMessageText",
                    "text": { "@type": "formattedText", "text": text }
                }
            })).await?;
            Ok(resp)
        }

        methods::SEARCH => {
            let chat_id = params["chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing chat_id"))?;
            let query = params["query"].as_str()
                .ok_or_else(|| anyhow::anyhow!("missing query"))?;
            let limit = params.get("limit").and_then(|v| v.as_i64()).unwrap_or(50);

            // Search via TDLib for consistent response format
            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "searchChatMessages",
                "chat_id": chat_id,
                "query": query,
                "limit": limit
            })).await?;
            Ok(resp)
        }

        methods::FORWARD_MESSAGE => {
            let from = params["from_chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing from_chat_id"))?;
            let to = params["to_chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing to_chat_id"))?;
            let msg_id = params["message_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing message_id"))?;

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "forwardMessages",
                "chat_id": to,
                "from_chat_id": from,
                "message_ids": [msg_id]
            })).await?;
            Ok(resp)
        }

        methods::DELETE_MESSAGE => {
            let chat_id = params["chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing chat_id"))?;
            let msg_id = params["message_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing message_id"))?;

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "deleteMessages",
                "chat_id": chat_id,
                "message_ids": [msg_id],
                "revoke": true
            })).await?;
            Ok(resp)
        }

        methods::MARK_READ => {
            let chat_id = params["chat_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing chat_id"))?;

            tdlib::notify(&state.td, serde_json::json!({
                "@type": "viewMessages",
                "chat_id": chat_id,
                "message_ids": [],
                "force_read": true
            }));
            Ok(serde_json::json!({"status": "sent"}))
        }

        methods::DOWNLOAD_FILE => {
            let file_id = params["file_id"].as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing file_id"))?;

            let resp = tdlib::query(&state.td, serde_json::json!({
                "@type": "downloadFile",
                "file_id": file_id,
                "priority": 1,
                "synchronous": true
            })).await?;
            Ok(resp)
        }

        methods::LOGOUT => {
            tdlib::notify(&state.td, serde_json::json!({"@type": "logOut"}));
            Ok(serde_json::json!({"status": "sent"}))
        }

        methods::SHUTDOWN => {
            Ok(serde_json::json!({"status":"stopping"}))
        }

        // Auth methods
        methods::AUTH_PHONE => {
            let phone = params["phone"].as_str()
                .ok_or_else(|| anyhow::anyhow!("missing phone"))?;
            tdlib::query(&state.td, serde_json::json!({
                "@type": "setAuthenticationPhoneNumber",
                "phone_number": phone,
                "settings": {
                    "@type": "phoneNumberAuthenticationSettings",
                    "is_current_phone_number": true
                }
            })).await
        }

        methods::AUTH_CODE => {
            let code = params["code"].as_str()
                .ok_or_else(|| anyhow::anyhow!("missing code"))?;
            tdlib::query(&state.td, serde_json::json!({
                "@type": "checkAuthenticationCode",
                "code": code
            })).await
        }

        methods::AUTH_PASSWORD => {
            let pw = params["password"].as_str()
                .ok_or_else(|| anyhow::anyhow!("missing password"))?;
            tdlib::query(&state.td, serde_json::json!({
                "@type": "checkAuthenticationPassword",
                "password": pw
            })).await
        }

        // Trigger auth state machine (called by tg login)
        "auth_trigger" => {
            tdlib::query(&state.td, serde_json::json!({
                "@type": "getAuthorizationState"
            })).await
        }

        "auth_email" => tdlib::query(&state.td, serde_json::json!({"@type":"setAuthenticationEmailAddress","email_address":params["email"]})).await,
        "auth_email_code" => tdlib::query(&state.td, serde_json::json!({"@type":"checkAuthenticationEmailCode","code":{"@type":"emailAddressAuthenticationCode","code":params["code"]}})).await,
        "auth_resend" => tdlib::query(&state.td, serde_json::json!({"@type":"resendAuthenticationCode","reason":{"@type":"resendCodeReasonUserRequest"}})).await,
        "auth_qr" => tdlib::query(&state.td, serde_json::json!({"@type":"requestQrCodeAuthentication","other_user_ids":[]})).await,

        _ => Err(anyhow::anyhow!("unknown method: {}", method)),
    }
}
