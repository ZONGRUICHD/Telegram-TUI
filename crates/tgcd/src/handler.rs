//! Validated RPC operations. Read operations never mark chats as read.
use crate::tdlib;
use anyhow::{anyhow, bail, ensure, Result};
use serde_json::{json, Value};
use tg_core::{config::TgConfig, error::TgError};
use tg_ipc::protocol::{Request, Response, RpcError};
use tokio::sync::{broadcast, watch};

pub struct AppState {
    pub config: TgConfig,
    pub td: tg_tdjson::TdClient,
    pub updates_tx: broadcast::Sender<Value>,
    pub shutdown_tx: watch::Sender<bool>,
    pub snapshot: std::sync::Arc<std::sync::RwLock<crate::dispatcher::Snapshot>>,
}

pub async fn handle_request(req: Request, state: &AppState) -> Response {
    match dispatch(&req.method, req.params, state).await {
        Ok(result) => Response {
            id: req.id,
            result: Some(result),
            error: None,
        },
        Err(error) => {
            let (code, message) = match error.downcast_ref::<TgError>() {
                Some(TgError::Tdlib { code, message }) => (*code, message.clone()),
                _ if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::TimedOut) =>
                {
                    (-32001, error.to_string())
                }
                _ => (-32602, error.to_string()),
            };
            Response {
                id: req.id,
                result: None,
                error: Some(RpcError { code, message }),
            }
        }
    }
}

async fn query(state: &AppState, value: Value) -> Result<Value> {
    tdlib::query(&state.td, value).await
}

async fn resolve(state: &AppState, value: &Value) -> Result<i64> {
    if let Some(id) = value.as_i64() {
        ensure!(id != 0, "聊天 ID 不能为 0");
        return Ok(id);
    }
    let text = value
        .as_str()
        .ok_or_else(|| anyhow!("需要聊天 ID、@username 或 me"))?;
    if let Ok(id) = text.parse::<i64>() {
        ensure!(id != 0, "聊天 ID 不能为 0");
        return Ok(id);
    }
    let chat = if text == "me" || text == "self" {
        let me = query(state, json!({"@type":"getMe"})).await?;
        query(
            state,
            json!({"@type":"createPrivateChat","user_id":me["id"],"force":true}),
        )
        .await?
    } else {
        ensure!(
            text.starts_with('@') && text.len() > 1,
            "聊天名称不唯一，请使用数字 ID 或 @username"
        );
        query(
            state,
            json!({"@type":"searchPublicChat","username":&text[1..]}),
        )
        .await?
    };
    chat["id"].as_i64().ok_or_else(|| anyhow!("无法解析聊天"))
}

async fn dispatch(method: &str, mut p: Value, state: &AppState) -> Result<Value> {
    if p.is_null() {
        p = json!({});
    }
    ensure!(p.is_object(), "params 必须为 JSON 对象");
    match method {
        "status" => {
            let auth = query(state, json!({"@type":"getAuthorizationState"})).await?;
            let connection = state.snapshot.read().unwrap().connection.clone();
            return Ok(
                json!({"socket":state.config.ipc.socket_path,"authorization":auth,
                "connection":connection,"version":env!("CARGO_PKG_VERSION"),"pid":std::process::id()}),
            );
        }
        "shutdown" => return Ok(json!({"status":"stopping"})),
        "folders" => return Ok(json!({"folders":state.snapshot.read().unwrap().folders})),
        "api" => {
            let mut q = p["query"].clone();
            ensure!(q.is_object(), "query 必须为 TDLib JSON 对象");
            let kind = required_str(&q, "@type")?;
            ensure!(
                ![
                    "setTdlibParameters",
                    "destroy",
                    "close",
                    "setDatabaseEncryptionKey",
                    "setLogStream",
                    "setLogVerbosityLevel"
                ]
                .contains(&kind),
                "此方法由 daemon 管理"
            );
            q.as_object_mut().unwrap().remove("@extra");
            q.as_object_mut().unwrap().remove("@client_id");
            return query(state, q).await;
        }
        "list_dialogs" => {
            let limit = bounded(&p, "limit", 20, 1, 100)?;
            let offset = bounded(&p, "offset", 0, 0, 9900)?;
            let list = chat_list(&p)?;
            // loadChats can return fewer than requested. getChats reflects the loaded list.
            for _ in 0..10 {
                let loaded = query(
                    state,
                    json!({"@type":"loadChats","chat_list":list,"limit":offset+limit}),
                )
                .await;
                match loaded {
                    Err(e)
                        if matches!(
                            e.downcast_ref::<TgError>(),
                            Some(TgError::Tdlib { code: 404, .. })
                        ) =>
                    {
                        break
                    }
                    Err(e) => return Err(e),
                    Ok(_) => {}
                }
                let count = query(
                    state,
                    json!({"@type":"getChats","chat_list":list,"limit":offset+limit}),
                )
                .await?;
                if count["chat_ids"].as_array().map_or(0, Vec::len) >= (offset + limit) as usize {
                    break;
                }
            }
            let chats = query(
                state,
                json!({"@type":"getChats","chat_list":list,"limit":offset+limit}),
            )
            .await?;
            let mut result = Vec::new();
            for id in chats["chat_ids"]
                .as_array()
                .into_iter()
                .flatten()
                .skip(offset as usize)
                .take(limit as usize)
            {
                result.push(query(state, json!({"@type":"getChat","chat_id":id})).await?);
            }
            let next = if result.is_empty() {
                None
            } else {
                Some(offset + result.len() as i64)
            };
            return Ok(json!({"chats":result,"next_offset":next,"chat_list":list}));
        }
        "find_chats" => {
            let result = query(
                state,
                json!({"@type":"searchChats","query":required_str(&p,"query")?,
                "limit":bounded(&p,"limit",20,1,100)?}),
            )
            .await?;
            let mut chats = Vec::new();
            for id in result["chat_ids"].as_array().into_iter().flatten() {
                chats.push(query(state, json!({"@type":"getChat","chat_id":id})).await?);
            }
            return Ok(json!({"chats":chats}));
        }
        "contacts" => {
            let result = query(state, json!({"@type":"getContacts"})).await?;
            let mut users = Vec::new();
            for id in result["user_ids"].as_array().into_iter().flatten() {
                users.push(query(state, json!({"@type":"getUser","user_id":id})).await?);
            }
            return Ok(json!({"users":users}));
        }
        _ => {}
    }
    for key in ["chat_id", "from_chat_id", "to_chat_id"] {
        if !p[key].is_null() {
            p[key] = json!(resolve(state, &p[key]).await?);
        }
    }
    if method == "mark_read" {
        let chat_id = required_id(&p, "chat_id")?;
        if p["message_ids"].as_array().is_none_or(|ids| ids.is_empty()) {
            let chat = query(state, json!({"@type":"getChat","chat_id":chat_id})).await?;
            match chat["last_message"]["id"].as_i64().filter(|id| *id != 0) {
                Some(id) => p["message_ids"] = json!([id]),
                None => return Ok(json!({"@type":"ok","read_message_ids":[]})),
            }
        }
    }
    if method == "mute" {
        let chat = query(
            state,
            json!({"@type":"getChat","chat_id":required_id(&p,"chat_id")?}),
        )
        .await?;
        let mut settings = chat["notification_settings"].clone();
        settings["use_default_mute_for"] = json!(false);
        settings["mute_for"] = json!(bounded(&p, "seconds", 3600, 0, i32::MAX as i64)?);
        return query(state,json!({"@type":"setChatNotificationSettings","chat_id":p["chat_id"],"notification_settings":settings})).await;
    }
    if method == "members" {
        let chat = query(
            state,
            json!({"@type":"getChat","chat_id":required_id(&p,"chat_id")?}),
        )
        .await?;
        return match chat["type"]["@type"].as_str().unwrap_or("") {
            "chatTypeSupergroup" => query(state,json!({"@type":"getSupergroupMembers",
                "supergroup_id":chat["type"]["supergroup_id"],
                "filter":{"@type":"supergroupMembersFilterRecent"},
                "offset":bounded(&p,"offset",0,0,i32::MAX as i64)?,
                "limit":bounded(&p,"limit",50,1,200)?})).await,
            "chatTypeBasicGroup" => query(state,json!({"@type":"getBasicGroupFullInfo","basic_group_id":chat["type"]["basic_group_id"]})).await,
            _ => bail!("此聊天不是群组"),
        };
    }
    let request = build_query(method, &p)?;
    let mut result = query(state, request).await?;
    if method == "get_messages" || method == "search" {
        let cursor = p["from_message_id"].as_i64().unwrap_or(0);
        if let Some(messages) = result["messages"].as_array_mut() {
            if cursor > 0 {
                messages.retain(|m| m["id"].as_i64().unwrap_or(0) < cursor);
            }
            messages.truncate(bounded(&p, "limit", 50, 1, 100)? as usize);
            messages.sort_by_key(|m| std::cmp::Reverse(m["id"].as_i64().unwrap_or(0)));
            let snapshot = state.snapshot.read().unwrap();
            for message in messages.iter_mut() {
                if let Some(user) = message["sender_id"]["user_id"]
                    .as_i64()
                    .and_then(|id| snapshot.users.get(&id))
                {
                    message["sender_name"] = json!(format!(
                        "{} {}",
                        user["first_name"].as_str().unwrap_or(""),
                        user["last_name"].as_str().unwrap_or("")
                    )
                    .trim());
                }
            }
            let next = messages
                .last()
                .map(|m| m["id"].clone())
                .unwrap_or(Value::Null);
            result["next_from_message_id"] = next;
        }
    }
    Ok(result)
}

pub fn required_str<'a>(p: &'a Value, name: &str) -> Result<&'a str> {
    p[name]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| anyhow!("{name} 不能为空"))
}
fn required_id(p: &Value, name: &str) -> Result<i64> {
    p[name]
        .as_i64()
        .filter(|id| *id != 0)
        .ok_or_else(|| anyhow!("{name} 必须为非零整数"))
}
fn bounded(p: &Value, name: &str, default: i64, min: i64, max: i64) -> Result<i64> {
    let n = if p[name].is_null() {
        default
    } else {
        p[name]
            .as_i64()
            .ok_or_else(|| anyhow!("{name} 必须为整数"))?
    };
    ensure!((min..=max).contains(&n), "{name} 必须在 {min}..={max} 之间");
    Ok(n)
}
fn chat_list(p: &Value) -> Result<Value> {
    Ok(match p["list"].as_str().unwrap_or("main") {
        "main" => json!({"@type":"chatListMain"}),
        "archive" => json!({"@type":"chatListArchive"}),
        folder => {
            json!({"@type":"chatListFolder","chat_folder_id":folder.parse::<i32>().map_err(|_|anyhow!("list 必须为 main、archive 或文件夹 ID"))?})
        }
    })
}
fn formatted(text: &str) -> Value {
    json!({"@type":"formattedText","text":text,"entities":[]})
}
fn history_limit(p: &Value) -> Result<i64> {
    let requested = bounded(p, "limit", 50, 1, 100)?;
    Ok((requested
        + if p["from_message_id"].as_i64().unwrap_or(0) > 0 {
            1
        } else {
            0
        })
    .min(100))
}
fn text_content(p: &Value) -> Result<Value> {
    let text = required_str(p, "text")?;
    ensure!(
        text.encode_utf16().count() <= 4096,
        "文本超过 4096 个 UTF-16 单元，请拆分消息"
    );
    Ok(json!({"@type":"inputMessageText","text":formatted(text),"clear_draft":true}))
}

/// Pure request builder, tested against the pinned official TDLib schema.
pub fn build_query(method: &str, p: &Value) -> Result<Value> {
    let chat = || required_id(p, "chat_id");
    Ok(match method {
        "get_me" => json!({"@type":"getMe"}),
        "auth_trigger" => json!({"@type":"getAuthorizationState"}),
        "auth_phone" => {
            json!({"@type":"setAuthenticationPhoneNumber","phone_number":required_str(p,"phone")?,"settings":{"@type":"phoneNumberAuthenticationSettings"}})
        }
        "auth_code" => json!({"@type":"checkAuthenticationCode","code":required_str(p,"code")?}),
        "auth_password" => {
            json!({"@type":"checkAuthenticationPassword","password":required_str(p,"password")?})
        }
        "auth_email" => {
            json!({"@type":"setAuthenticationEmailAddress","email_address":required_str(p,"email")?})
        }
        "auth_email_code" => {
            json!({"@type":"checkAuthenticationEmailCode","code":{"@type":"emailAddressAuthenticationCode","code":required_str(p,"code")?}})
        }
        "auth_resend" => {
            json!({"@type":"resendAuthenticationCode","reason":{"@type":"resendCodeReasonUserRequest"}})
        }
        "auth_qr" => json!({"@type":"requestQrCodeAuthentication","other_user_ids":[]}),
        "get_chat" => json!({"@type":"getChat","chat_id":chat()?}),
        "get_message" => {
            json!({"@type":"getMessage","chat_id":chat()?,"message_id":required_id(p,"message_id")?})
        }
        "get_messages" => {
            if let Some(topic) = p["topic"].as_i64().filter(|id| *id > 0) {
                json!({"@type":"getForumTopicHistory","chat_id":chat()?,"forum_topic_id":topic,
                "from_message_id":bounded(p,"from_message_id",0,0,i64::MAX)?,"offset":0,"limit":history_limit(p)?})
            } else {
                json!({"@type":"getChatHistory","chat_id":chat()?,
            "from_message_id":bounded(p,"from_message_id",0,0,i64::MAX)?,"offset":0,
            "limit":history_limit(p)?,"only_local":false})
            }
        }
        "search" => {
            json!({"@type":"searchChatMessages","chat_id":chat()?,"query":required_str(p,"query")?,
            "from_message_id":bounded(p,"from_message_id",0,0,i64::MAX)?,"offset":0,
            "limit":history_limit(p)?,"topic_id":p["topic"].as_i64().filter(|id|*id>0).map(|id|json!({"@type":"messageTopicForum","forum_topic_id":id})),"filter":{"@type":"searchMessagesFilterEmpty"}})
        }
        "send_message" | "send_file" => {
            let content = if method == "send_message" {
                text_content(p)?
            } else {
                let path = std::path::Path::new(required_str(p, "path")?);
                ensure!(
                    path.is_absolute() && path.is_file(),
                    "附件必须是存在的绝对文件路径"
                );
                let caption = p["caption"].as_str().unwrap_or("");
                ensure!(
                    caption.encode_utf16().count() <= 1024,
                    "附件说明不能超过 1024 个 UTF-16 单元"
                );
                let input = json!({"@type":"inputFileLocal","path":path});
                if p["photo"].as_bool().unwrap_or(false) {
                    json!({"@type":"inputMessagePhoto","photo":{"@type":"inputPhoto","photo":input},"caption":formatted(caption)})
                } else {
                    json!({"@type":"inputMessageDocument","document":{"@type":"inputDocument","document":input,"disable_content_type_detection":false},"caption":formatted(caption)})
                }
            };
            let mut q = json!({"@type":"sendMessage","chat_id":chat()?,"input_message_content":content,
                "options":{"@type":"messageSendOptions","disable_notification":p["silent"].as_bool().unwrap_or(false)}});
            if let Some(id) = p["reply_to"].as_i64().filter(|id| *id > 0) {
                q["reply_to"] = json!({"@type":"inputMessageReplyToMessage","message_id":id});
            }
            if let Some(id) = p["topic"].as_i64().filter(|id| *id > 0) {
                q["topic_id"] = json!({"@type":"messageTopicForum","forum_topic_id":id});
            }
            q
        }
        "edit_message" => {
            json!({"@type":"editMessageText","chat_id":chat()?,"message_id":required_id(p,"message_id")?,"input_message_content":text_content(p)?})
        }
        "forward_message" => {
            json!({"@type":"forwardMessages","chat_id":required_id(p,"to_chat_id")?,"from_chat_id":required_id(p,"from_chat_id")?,"message_ids":[required_id(p,"message_id")?]})
        }
        "delete_message" => {
            json!({"@type":"deleteMessages","chat_id":chat()?,"message_ids":[required_id(p,"message_id")?],"revoke":p["revoke"].as_bool().unwrap_or(false)})
        }
        "mark_read" => {
            let ids = p["message_ids"]
                .as_array()
                .filter(|ids| !ids.is_empty())
                .ok_or_else(|| anyhow!("已读操作必须指定实际消息 ID"))?;
            ensure!(
                ids.iter().all(|id| id.as_i64().is_some_and(|id| id > 0)),
                "无效的消息 ID"
            );
            json!({"@type":"viewMessages","chat_id":chat()?,"message_ids":ids,"source":{"@type":"messageSourceChatHistory"},"force_read":true})
        }
        "download_file" => {
            json!({"@type":"downloadFile","file_id":required_id(p,"file_id")?,"priority":16,"synchronous":false})
        }
        "get_file" => json!({"@type":"getFile","file_id":required_id(p,"file_id")?}),
        "open_chat" => json!({"@type":"openChat","chat_id":chat()?}),
        "close_chat" => json!({"@type":"closeChat","chat_id":chat()?}),
        "pin" => {
            json!({"@type":"toggleChatIsPinned","chat_id":chat()?,"chat_list":chat_list(p)?,"is_pinned":p["pinned"].as_bool().unwrap_or(true)})
        }
        "archive" => {
            json!({"@type":"addChatToList","chat_id":chat()?,"chat_list":if p["archived"].as_bool().unwrap_or(true) {json!({"@type":"chatListArchive"})} else {json!({"@type":"chatListMain"})}})
        }
        "draft" => {
            json!({"@type":"setChatDraftMessage","chat_id":chat()?,"topic_id":p["topic"].as_i64().filter(|id|*id>0).map(|id|json!({"@type":"messageTopicForum","forum_topic_id":id})),"draft_message": if p["text"].as_str().unwrap_or("").is_empty() { Value::Null } else {
                json!({"@type":"draftMessage","content":{"@type":"draftMessageContentText","text":formatted(p["text"].as_str().unwrap_or(""))}})
            }})
        }
        "join" => {
            if p["invite_link"].is_string() {
                json!({"@type":"joinChatByInviteLink","invite_link":required_str(p,"invite_link")?})
            } else {
                json!({"@type":"joinChat","chat_id":chat()?})
            }
        }
        "leave" => json!({"@type":"leaveChat","chat_id":chat()?}),
        "topics" => {
            json!({"@type":"getForumTopics","chat_id":chat()?,"query":p["query"].as_str().unwrap_or(""),
            "offset_date":p["offset_date"].as_i64().unwrap_or(0),"offset_message_id":p["offset_message_id"].as_i64().unwrap_or(0),
            "offset_forum_topic_id":p["offset_topic_id"].as_i64().unwrap_or(0),"limit":bounded(p,"limit",50,1,100)?})
        }
        "logout" => json!({"@type":"logOut"}),
        _ => bail!("未知方法：{method}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn read_requires_real_message_ids() {
        assert!(build_query("mark_read", &json!({"chat_id":-100,"message_ids":[]})).is_err());
        assert_eq!(
            build_query("mark_read", &json!({"chat_id":-100,"message_ids":[123]})).unwrap()
                ["message_ids"],
            json!([123])
        );
    }
    #[test]
    fn validates_history_and_preserves_cursor() {
        assert_eq!(
            build_query(
                "get_messages",
                &json!({"chat_id":1,"limit":1,"from_message_id":123})
            )
            .unwrap()["limit"],
            2
        );
        assert!(build_query("get_messages", &json!({"chat_id":-1,"limit":200})).is_err());
        assert!(build_query("get_messages", &json!({"chat_id":-1,"limit":0})).is_err());
        assert_eq!(
            build_query(
                "get_messages",
                &json!({"chat_id":-1,"from_message_id":123,"limit":50})
            )
            .unwrap()["from_message_id"],
            123
        );
    }
    #[test]
    fn sends_unicode_reply_and_silent_without_losing_text() {
        let q = build_query(
            "send_message",
            &json!({"chat_id":-100,"text":"  中文\n下一行  ","reply_to":123,"silent":true}),
        )
        .unwrap();
        assert_eq!(
            q["input_message_content"]["text"]["text"],
            "  中文\n下一行  "
        );
        assert_eq!(q["reply_to"]["message_id"], 123);
        assert_eq!(q["options"]["disable_notification"], true);
        assert!(build_query("send_message", &json!({"chat_id":-1,"text":" "})).is_err());
    }
    #[test]
    fn deletion_requires_explicit_revoke() {
        assert_eq!(
            build_query("delete_message", &json!({"chat_id":1,"message_id":123})).unwrap()
                ["revoke"],
            false
        );
    }
}
