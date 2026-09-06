//! Explicit offline fixture; never connects to Telegram.
use super::app::{App, Focus};
use serde_json::{json, Value};
use std::collections::HashMap;
use tg_ipc::protocol::{Request, Response, ServerMessage};

pub struct Demo {
    chats: Vec<Value>,
    messages: HashMap<i64, Vec<Value>>,
    next: i64,
}
impl Demo {
    pub fn new() -> Self {
        let chat = -100100;
        let date = 1788680400;
        let messages = vec![
            msg(
                1001,
                chat,
                date,
                false,
                "林夕",
                "早上好，今天把 Telegram-TUI 的聊天体验再打磨一下。",
            ),
            msg(
                1002,
                chat,
                date + 120,
                true,
                "你",
                "左侧切换聊天，右侧浏览消息。回复、编辑、搜索都可以直接用键盘完成。",
            ),
            msg(
                1003,
                chat,
                date + 240,
                false,
                "林夕",
                "输入框支持中文、Emoji 和多行粘贴了吗？👋",
            ),
            msg(
                1004,
                chat,
                date + 300,
                true,
                "你",
                "支持了。Shift+Enter 换行，Enter 发送。\n切换聊天时草稿也会留下来。",
            ),
            json!({"id":1005,"chat_id":chat,"date":date+360,"sender_name":"周舟","is_outgoing":false,
                "sender_id":{"user_id":12},"content":{"@type":"messageDocument","document":{"file_name":"交互验收清单.pdf","document":{"id":72}},
                    "caption":{"text":"这是本轮的验收清单，先看日常聊天流程。"}}}),
            msg(
                1006,
                chat,
                date + 480,
                false,
                "林夕",
                "很好。我来检查快速切换聊天时的消息顺序，你看一下窄窗口的布局。",
            ),
        ];
        let chats = vec![
            json!({"id":chat,"title":"Telegram-TUI 讨论组","unread_count":2,"last_read_outbox_message_id":1004,
                "type":{"@type":"chatTypeSupergroup"},"positions":[{"list":{"@type":"chatListMain"},"order":"900","is_pinned":true}],"last_message":messages.last()}),
            json!({"id":100,"title":"收藏夹","unread_count":0,"positions":[{"list":{"@type":"chatListMain"},"order":"800"}],
                "last_message":{"content":{"@type":"messageText","text":{"text":"记录灵感，稍后继续"}}}}),
            json!({"id":101,"title":"林夕","unread_count":1,"positions":[{"list":{"@type":"chatListMain"},"order":"700"}],
                "last_message":{"content":{"@type":"messageText","text":{"text":"一会儿见 👋"}}}}),
            json!({"id":-100200,"title":"设计与开发","unread_count":8,"positions":[{"list":{"@type":"chatListMain"},"order":"600"}],
                "last_message":{"content":{"@type":"messageText","text":{"text":"新的界面稿已经更新"}}}}),
            json!({"id":-100300,"title":"Telegram 资讯","unread_count":0,"positions":[{"list":{"@type":"chatListMain"},"order":"500"}],
                "last_message":{"content":{"@type":"messageText","text":{"text":"本周更新与项目动态"}}}}),
        ];
        Self {
            chats,
            messages: HashMap::from([(chat, messages)]),
            next: 2000,
        }
    }
    pub fn app(&self) -> App {
        let mut app = App::new(50, true);
        app.auth = json!({"@type":"authorizationStateReady"});
        app.focus = Focus::Composer;
        app.chats = self.chats.clone();
        app.active = Some(-100100);
        app.chat_state.select(Some(0));
        app.messages = self.messages[&-100100].clone();
        app.message_state.select(Some(app.messages.len() - 1));
        app.folders = vec![
            json!({"id":2,"name":{"text":{"text":"工作"}}}),
            json!({"id":3,"name":{"text":{"text":"个人"}}}),
        ];
        app.status = "演示模式 · 所有数据均为虚构，不会发送到 Telegram".into();
        app.connection_label = "离线演示".into();
        app
    }
    pub fn respond(&mut self, request: Request) -> ServerMessage {
        let p = &request.params;
        let chat = p["chat_id"].as_i64().unwrap_or(-100100);
        let result = match request.method.as_str() {
            "status" => json!({"authorization":{"@type":"authorizationStateReady"}}),
            "auth_trigger" => json!({"@type":"authorizationStateReady"}),
            "list_dialogs" | "find_chats" => {
                let query = p["query"].as_str().unwrap_or("");
                json!({"chats":self.chats.iter().filter(|c|c["title"].as_str().unwrap_or("").contains(query)).cloned().collect::<Vec<_>>()})
            }
            "get_messages" | "search" => {
                let before = p["from_message_id"].as_i64().unwrap_or(0);
                let query = p["query"].as_str().unwrap_or("");
                json!({"messages":self.messages.get(&chat).into_iter().flatten().filter(|m|
                    (before==0 || m["id"].as_i64().unwrap_or(0)<before) && crate::output::message_text(m).contains(query)
                ).cloned().collect::<Vec<_>>()})
            }
            "send_message" | "edit_message" => {
                self.next += 1;
                let id = if request.method == "edit_message" {
                    p["message_id"].as_i64().unwrap_or(self.next)
                } else {
                    self.next
                };
                let mut message = msg(
                    id,
                    chat,
                    chrono::Utc::now().timestamp(),
                    true,
                    "你",
                    p["text"].as_str().unwrap_or(""),
                );
                if !p["reply_to"].is_null() {
                    message["reply_to"] = json!({"message_id":p["reply_to"]});
                }
                let messages = self.messages.entry(chat).or_default();
                messages.retain(|m| m["id"].as_i64() != Some(id));
                messages.push(message.clone());
                message
            }
            "delete_message" => {
                self.messages
                    .entry(chat)
                    .or_default()
                    .retain(|m| m["id"] != p["message_id"]);
                json!({"@type":"ok"})
            }
            "get_chat" => self
                .chats
                .iter()
                .find(|c| c["id"].as_i64() == Some(chat))
                .cloned()
                .unwrap_or(json!({})),
            "folders" => {
                json!({"folders":[{"id":2,"name":{"text":{"text":"工作"}}},{"id":3,"name":{"text":{"text":"个人"}}}]})
            }
            _ => json!({"@type":"ok"}),
        };
        ServerMessage::Response(Response {
            id: request.id,
            result: Some(result),
            error: None,
        })
    }
}
fn msg(id: i64, chat: i64, date: i64, outgoing: bool, name: &str, text: &str) -> Value {
    json!({"id":id,"chat_id":chat,"date":date,"is_outgoing":outgoing,"sender_name":name,
        "sender_id":{"user_id":11},"content":{"@type":"messageText","text":{"text":text}}})
}
