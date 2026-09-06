use super::editor::Editor;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::{layout::Rect, widgets::ListState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};
use tg_ipc::protocol::{Request, ServerMessage};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Chats,
    Messages,
    Composer,
    ChatSearch,
    MessageSearch,
    Palette,
    Login,
}
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Draft {
    pub text: String,
    pub reply: Option<i64>,
    pub edit: Option<i64>,
}
#[derive(Clone)]
pub enum Job {
    Status,
    Auth,
    Folders,
    Chats {
        generation: u64,
    },
    MoreChats {
        generation: u64,
    },
    History {
        chat: i64,
        generation: u64,
        before: i64,
    },
    Send {
        chat: i64,
        draft: Draft,
    },
    Info(String),
    Action(String),
    Draft,
}
pub struct Pending {
    pub job: Job,
    pub started: Instant,
}
pub struct Confirm {
    pub title: String,
    pub method: String,
    pub params: Value,
}
#[derive(Default)]
pub struct HitAreas {
    pub chats: Rect,
    pub messages: Rect,
    pub composer: Rect,
    pub search: Rect,
    pub message_rows: Vec<(Rect, usize)>,
    pub chat_rows: Vec<(Rect, usize)>,
}

pub struct App {
    pub chats: Vec<Value>,
    pub chat_state: ListState,
    pub active: Option<i64>,
    pub messages: Vec<Value>,
    pub message_state: ListState,
    pub editor: Editor,
    pub search: Editor,
    pub palette: Editor,
    pub auth_input: Editor,
    pub drafts: HashMap<i64, Draft>,
    pub reply: Option<i64>,
    pub edit: Option<i64>,
    pub focus: Focus,
    pub status: String,
    pub connected: bool,
    pub auth: Value,
    pub list: String,
    pub folders: Vec<Value>,
    pub chat_query: String,
    pub message_query: String,
    pub pending: HashMap<String, Pending>,
    pub outbox: VecDeque<Request>,
    pub generation: u64,
    pub chat_generation: u64,
    pub page_size: usize,
    pub next_chat_offset: Option<i64>,
    pub info: Option<(String, String)>,
    pub confirm: Option<Confirm>,
    pub help: bool,
    pub quit: bool,
    pub demo: bool,
    pub reconnect: bool,
    pub hit: HitAreas,
    pub draft_dirty: bool,
    pub last_draft_save: Instant,
    pub needs_chat_refresh: bool,
    pub last_refresh: Instant,
    pub connection_label: String,
    pub narrow_sidebar: bool,
    pub info_scroll: u16,
    pub topic: Option<i64>,
    pub window_focused: bool,
    pub live: HashMap<i64, Value>,
    pub deleted: std::collections::HashSet<i64>,
}
impl App {
    pub fn new(page_size: usize, demo: bool) -> Self {
        Self {
            chats: vec![],
            chat_state: ListState::default(),
            active: None,
            messages: vec![],
            message_state: ListState::default(),
            editor: Editor::default(),
            search: Editor::default(),
            palette: Editor::default(),
            auth_input: Editor::default(),
            drafts: HashMap::new(),
            reply: None,
            edit: None,
            focus: Focus::Login,
            status: "正在连接…".into(),
            connected: true,
            auth: json!({}),
            list: "main".into(),
            folders: vec![],
            chat_query: String::new(),
            message_query: String::new(),
            pending: HashMap::new(),
            outbox: VecDeque::new(),
            generation: 0,
            chat_generation: 0,
            page_size: page_size.clamp(1, 100),
            next_chat_offset: Some(0),
            info: None,
            confirm: None,
            help: false,
            quit: false,
            demo,
            reconnect: false,
            hit: HitAreas::default(),
            draft_dirty: false,
            last_draft_save: Instant::now(),
            needs_chat_refresh: false,
            last_refresh: Instant::now(),
            connection_label: "连接中".into(),
            narrow_sidebar: false,
            info_scroll: 0,
            topic: None,
            window_focused: true,
            live: HashMap::new(),
            deleted: std::collections::HashSet::new(),
        }
    }
    pub fn ready(&self) -> bool {
        self.auth["@type"] == "authorizationStateReady"
    }
    pub fn chat(&self) -> Option<&Value> {
        self.active
            .and_then(|id| self.chats.iter().find(|c| c["id"].as_i64() == Some(id)))
    }
    pub fn selected_message(&self) -> Option<&Value> {
        self.message_state
            .selected()
            .and_then(|i| self.messages.get(i))
    }
    pub fn request(&mut self, method: &str, params: Value, job: Job) {
        if !self.connected {
            self.status = "连接已断开，草稿已保留；F5 重新连接".into();
            return;
        }
        if self.pending.len() >= 128 {
            self.status = "请求较多，请稍候".into();
            return;
        }
        let id = uuid::Uuid::new_v4().to_string();
        self.pending.insert(
            id.clone(),
            Pending {
                job,
                started: Instant::now(),
            },
        );
        self.outbox.push_back(Request {
            id,
            method: method.into(),
            params,
        });
    }
    pub fn bootstrap(&mut self) {
        self.request("status", json!({}), Job::Status);
    }
    pub fn refresh_chats(&mut self) {
        self.chat_generation += 1;
        let (method, p) = if self.chat_query.is_empty() {
            ("list_dialogs", json!({"list":self.list,"limit":100}))
        } else {
            ("find_chats", json!({"query":self.chat_query,"limit":100}))
        };
        self.request(
            method,
            p,
            Job::Chats {
                generation: self.chat_generation,
            },
        );
        self.last_refresh = Instant::now();
        self.needs_chat_refresh = false;
    }
    pub fn load_history(&mut self, before: i64) {
        if let Some(chat) = self.active {
            if self.pending.values().any(|p|matches!(p.job,Job::History{chat:c,generation:g,..} if c==chat && g==self.generation)) {return;}
            let method = if self.message_query.is_empty() {
                "get_messages"
            } else {
                "search"
            };
            self.request(method,json!({"chat_id":chat,"limit":self.page_size,"from_message_id":before,"query":self.message_query,"topic":self.topic}),
                Job::History{chat,generation:self.generation,before});
        }
    }
    pub fn more_chats(&mut self) {
        if !self.chat_query.is_empty()
            || self
                .pending
                .values()
                .any(|p| matches!(p.job, Job::MoreChats { .. }))
        {
            return;
        }
        if let Some(offset) = self.next_chat_offset {
            self.request(
                "list_dialogs",
                json!({"list":self.list,"limit":100,"offset":offset}),
                Job::MoreChats {
                    generation: self.chat_generation,
                },
            );
        }
    }
    pub fn save_draft(&mut self) {
        if let Some(id) = self.active {
            self.drafts.insert(
                id,
                Draft {
                    text: self.editor.text.clone(),
                    reply: self.reply,
                    edit: self.edit,
                },
            );
            self.draft_dirty = true;
        }
    }
    pub fn select_chat(&mut self, index: usize) {
        let Some(id) = self.chats.get(index).and_then(|c| c["id"].as_i64()) else {
            return;
        };
        if self.active == Some(id) {
            self.chat_state.select(Some(index));
            return;
        }
        self.save_draft();
        if let Some(old) = self.active {
            self.request("close_chat", json!({"chat_id":old}), Job::Draft);
        }
        self.active = Some(id);
        self.chat_state.select(Some(index));
        self.generation += 1;
        self.messages.clear();
        self.live.clear();
        self.deleted.clear();
        self.message_state.select(None);
        self.message_query.clear();
        self.topic = None;
        let draft = self.drafts.get(&id).cloned().unwrap_or_else(|| Draft {
            text: self.chats[index]["draft_message"]["content"]["text"]["text"]
                .as_str()
                .unwrap_or("")
                .into(),
            ..Default::default()
        });
        self.editor.set(draft.text);
        self.reply = draft.reply;
        self.edit = draft.edit;
        self.request("open_chat", json!({"chat_id":id}), Job::Draft);
        self.load_history(0);
        self.status = "正在加载聊天…".into();
        self.narrow_sidebar = false;
    }
    pub fn choose_list(&mut self, list: String) {
        self.save_draft();
        if let Some(chat) = self.active {
            self.request("close_chat", json!({"chat_id":chat}), Job::Draft);
        }
        self.list = list;
        self.chat_query.clear();
        self.search.clear();
        self.chats.clear();
        self.active = None;
        self.messages.clear();
        self.generation += 1;
        self.editor.clear();
        self.reply = None;
        self.edit = None;
        self.chat_state.select(None);
        self.refresh_chats();
        self.focus = Focus::Chats;
    }
    pub fn send(&mut self) {
        let Some(chat) = self.active else {
            return;
        };
        if self.editor.text.trim().is_empty() {
            return;
        }
        if self.editor.text.encode_utf16().count() > 4096 {
            self.status = "消息过长（最多 4096 个 UTF-16 单元）".into();
            return;
        }
        if self
            .pending
            .values()
            .any(|p| matches!(p.job,Job::Send{chat:c,..} if c==chat))
        {
            self.status = "上一条正在提交，请稍候".into();
            return;
        }
        self.save_draft();
        let draft = Draft {
            text: self.editor.text.clone(),
            reply: self.reply,
            edit: self.edit,
        };
        let method = if self.edit.is_some() {
            "edit_message"
        } else {
            "send_message"
        };
        self.request(method,json!({"chat_id":chat,"text":draft.text,"reply_to":draft.reply,"message_id":draft.edit,"topic":self.topic}),
            Job::Send{chat,draft});
        self.status = "正在提交…".into();
    }
    fn upsert_message(&mut self, message: Value) {
        if message["chat_id"].as_i64() != self.active {
            return;
        }
        if self.topic.is_some() && message["topic_id"]["forum_topic_id"].as_i64() != self.topic {
            return;
        }
        let id = message["id"].as_i64().unwrap_or(0);
        if self.deleted.contains(&id) {
            return;
        }
        let follow = self
            .message_state
            .selected()
            .is_none_or(|i| i + 1 >= self.messages.len());
        if let Some(old) = self
            .messages
            .iter_mut()
            .find(|m| m["id"].as_i64() == Some(id))
        {
            *old = message;
        } else {
            self.messages.push(message);
        }
        // Pending outgoing messages sort after confirmed history.
        self.messages.sort_by_key(|m| {
            (
                m["sending_state"].is_object() || m["id"].as_i64().unwrap_or(0) < 0,
                m["id"].as_i64().unwrap_or(0),
            )
        });
        if follow {
            self.message_state
                .select(self.messages.len().checked_sub(1));
        }
    }
    pub fn handle(&mut self, message: ServerMessage) {
        match message {
            ServerMessage::Response(response) => {
                let Some(pending) = self.pending.remove(&response.id) else {
                    return;
                };
                if let Some(error) = response.error {
                    self.status = format!("操作失败：{}", crate::output::safe(&error.message));
                    if matches!(pending.job, Job::Auth) {
                        self.auth_input.clear();
                        self.request("auth_trigger", json!({}), Job::Status);
                    }
                    return;
                }
                let result = response.result.unwrap_or(Value::Null);
                match pending.job {
                    Job::Status => {
                        let auth = if result["authorization"].is_object() {
                            result["authorization"].clone()
                        } else {
                            result.clone()
                        };
                        self.set_auth(auth);
                        if let Some(kind) = result["connection"]["@type"].as_str() {
                            self.connection_label = connection_label(kind).into();
                        }
                    }
                    Job::Auth => {
                        self.auth_input.clear();
                        self.request("auth_trigger", json!({}), Job::Status);
                    }
                    Job::Folders => {
                        self.folders = result["folders"].as_array().cloned().unwrap_or_default();
                    }
                    Job::Chats { generation } if generation == self.chat_generation => {
                        let selected = self.active;
                        let mut chats = result["chats"].as_array().cloned().unwrap_or_default();
                        // A first-page refresh must not discard pages already loaded by the user.
                        if self.chat_query.is_empty() && chats.len() == 100 {
                            for chat in &self.chats {
                                if !chats.iter().any(|c| c["id"] == chat["id"]) {
                                    chats.push(chat.clone());
                                }
                            }
                            self.next_chat_offset = Some(chats.len() as i64);
                        } else {
                            self.next_chat_offset = result["next_offset"].as_i64();
                        }
                        self.chats = chats;
                        if let Some(index) =
                            self.chats.iter().position(|c| c["id"].as_i64() == selected)
                        {
                            self.chat_state.select(Some(index));
                        } else if !self.chats.is_empty() {
                            self.select_chat(0);
                        } else {
                            self.save_draft();
                            if let Some(chat) = self.active {
                                self.request("close_chat", json!({"chat_id":chat}), Job::Draft);
                            }
                            self.active = None;
                            self.messages.clear();
                            self.editor.clear();
                            self.reply = None;
                            self.edit = None;
                            self.chat_state.select(None);
                            self.generation += 1;
                        }
                        self.status = format!("{} 个聊天", self.chats.len());
                    }
                    Job::MoreChats { generation } if generation == self.chat_generation => {
                        self.next_chat_offset = result["next_offset"].as_i64();
                        for chat in result["chats"].as_array().into_iter().flatten() {
                            if !self.chats.iter().any(|c| c["id"] == chat["id"]) {
                                self.chats.push(chat.clone());
                            }
                        }
                        self.status = format!("已加载 {} 个聊天", self.chats.len());
                    }
                    Job::History {
                        chat,
                        generation,
                        before,
                    } if self.active == Some(chat) && generation == self.generation => {
                        let msgs = result["messages"].as_array().cloned().unwrap_or_default();
                        let old_selected = self.selected_message().and_then(|m| m["id"].as_i64());
                        if before == 0 {
                            self.messages.clear();
                        }
                        let empty = msgs.is_empty();
                        for m in msgs {
                            self.upsert_message(m);
                        }
                        if self.message_query.is_empty() {
                            for message in self.live.values().cloned().collect::<Vec<_>>() {
                                self.upsert_message(message);
                            }
                        }
                        if before > 0 {
                            self.message_state.select(old_selected.and_then(|id| {
                                self.messages
                                    .iter()
                                    .position(|m| m["id"].as_i64() == Some(id))
                            }));
                        } else {
                            self.message_state
                                .select(self.messages.len().checked_sub(1));
                        }
                        self.status = if empty && before > 0 {
                            "没有更早的消息".into()
                        } else {
                            format!("{} 条消息 · PgUp 查看更早", self.messages.len())
                        };
                        if before == 0
                            && self.message_query.is_empty()
                            && self.focus != Focus::Chats
                        {
                            self.mark_visible_read();
                        }
                    }
                    Job::Send { chat, draft } => {
                        // Clear only the exact draft accepted by TDLib; never erase newer typing.
                        if self
                            .drafts
                            .get(&chat)
                            .is_some_and(|d| d.text == draft.text && d.edit == draft.edit)
                        {
                            self.drafts.remove(&chat);
                            self.draft_dirty = true;
                        }
                        if self.active == Some(chat)
                            && self.editor.text == draft.text
                            && self.edit == draft.edit
                        {
                            self.editor.clear();
                            self.reply = None;
                            self.edit = None;
                        }
                        self.upsert_message(result.clone());
                        if result["chat_id"].as_i64() == self.active {
                            if let Some(id) = result["id"].as_i64() {
                                self.live.insert(id, result.clone());
                            }
                        }
                        self.status = if result["sending_state"].is_null() {
                            "消息已提交".into()
                        } else {
                            "消息已排队，等待 Telegram 确认".into()
                        };
                    }
                    Job::Info(title) => {
                        self.info = Some((title.clone(), format_info(&title, &result)));
                        self.info_scroll = 0;
                    }
                    Job::Action(label) => {
                        self.status = label;
                        self.needs_chat_refresh = true;
                    }
                    _ => {}
                }
            }
            ServerMessage::Event(event) => {
                let p = event.data;
                match event.name.as_str() {
                    "updateAuthorizationState" => self.set_auth(p["authorization_state"].clone()),
                    "updateConnectionState" => {
                        self.connection_label =
                            connection_label(p["state"]["@type"].as_str().unwrap_or("")).into()
                    }
                    "updateChatFolders" => {
                        self.folders = p["chat_folders"].as_array().cloned().unwrap_or_default()
                    }
                    "updateNewMessage" => {
                        let m = p["message"].clone();
                        if m["chat_id"].as_i64() == self.active {
                            if let Some(id) = m["id"].as_i64() {
                                self.live.insert(id, m.clone());
                            }
                        }
                        if self.message_query.is_empty() {
                            self.upsert_message(m);
                        }
                        if self.focus == Focus::Composer
                            && self
                                .message_state
                                .selected()
                                .is_some_and(|i| i + 1 == self.messages.len())
                        {
                            self.mark_visible_read();
                        }
                        self.needs_chat_refresh = true;
                    }
                    "updateMessageSendSucceeded" | "updateMessageSendFailed" => {
                        if p["message"]["chat_id"].as_i64() == self.active {
                            let old = p["old_message_id"].as_i64();
                            if let Some(old) = old {
                                self.live.remove(&old);
                                self.deleted.insert(old);
                            }
                            if let Some(id) = p["message"]["id"].as_i64() {
                                self.live.insert(id, p["message"].clone());
                            }
                            self.messages.retain(|m| m["id"].as_i64() != old);
                            self.upsert_message(p["message"].clone());
                            self.status = if event.name == "updateMessageSendSucceeded" {
                                "消息已发送".into()
                            } else {
                                "消息发送失败，请检查消息状态后重试".into()
                            };
                        }
                    }
                    "updateMessageContent" => {
                        if p["chat_id"].as_i64() == self.active {
                            if let Some(m) = self
                                .messages
                                .iter_mut()
                                .find(|m| m["id"] == p["message_id"])
                            {
                                m["content"] = p["new_content"].clone();
                                if let Some(id) = m["id"].as_i64() {
                                    self.live.insert(id, m.clone());
                                }
                            }
                        }
                    }
                    "updateDeleteMessages" => {
                        if p["chat_id"].as_i64() == self.active {
                            if let Some(ids) = p["message_ids"].as_array() {
                                for id in ids.iter().filter_map(|id| id.as_i64()) {
                                    self.deleted.insert(id);
                                    self.live.remove(&id);
                                }
                                self.messages.retain(|m| !ids.contains(&m["id"]));
                            }
                            self.clamp_message_selection();
                        }
                    }
                    "updateChatReadInbox"
                    | "updateChatReadOutbox"
                    | "updateChatTitle"
                    | "updateChatNotificationSettings"
                    | "updateChatDraftMessage" => {
                        if let Some(chat) = self.chats.iter_mut().find(|c| c["id"] == p["chat_id"])
                        {
                            for field in [
                                "unread_count",
                                "last_read_inbox_message_id",
                                "last_read_outbox_message_id",
                                "title",
                                "notification_settings",
                                "draft_message",
                            ] {
                                if !p[field].is_null() {
                                    chat[field] = p[field].clone();
                                }
                            }
                        }
                    }
                    "updateChatLastMessage" | "updateChatPosition" => {
                        let list = match self.list.as_str() {
                            "main" => json!({"@type":"chatListMain"}),
                            "archive" => json!({"@type":"chatListArchive"}),
                            id => {
                                json!({"@type":"chatListFolder","chat_folder_id":id.parse::<i64>().unwrap_or(0)})
                            }
                        };
                        if let Some(chat) = self.chats.iter_mut().find(|c| c["id"] == p["chat_id"])
                        {
                            if event.name == "updateChatLastMessage" {
                                chat["last_message"] = p["last_message"].clone();
                            }
                        }
                        let removed = if event.name == "updateChatPosition" {
                            p["position"]["list"] == list
                                && p["position"]["order"].as_str() == Some("0")
                        } else {
                            p["positions"].as_array().is_some_and(|positions| {
                                !positions.iter().any(|pos| pos["list"] == list)
                            })
                        };
                        if self.chat_query.is_empty() && removed {
                            self.chats.retain(|c| c["id"] != p["chat_id"]);
                        }
                        self.needs_chat_refresh = true;
                    }
                    "updateNewChat" => self.needs_chat_refresh = true,
                    "updateFile" => {
                        let file = &p["file"];
                        if file["local"]["is_downloading_completed"] == true {
                            self.status = format!(
                                "下载完成：{}",
                                crate::output::safe(file["local"]["path"].as_str().unwrap_or(""))
                            );
                        } else if file["local"]["is_downloading_active"] == true {
                            self.status = format!(
                                "正在下载文件 {} · {} / {} 字节",
                                file["id"], file["local"]["downloaded_size"], file["size"]
                            );
                        }
                    }
                    "resync_required" => {
                        self.status = "更新接收落后，正在重新同步".into();
                        self.generation += 1;
                        self.load_history(0);
                        self.refresh_chats();
                    }
                    _ => {}
                }
            }
            ServerMessage::AuthState(_) => {}
        }
    }
    pub fn set_auth(&mut self, auth: Value) {
        let was_ready = self.ready();
        if self.auth["@type"] != auth["@type"] {
            self.auth_input.clear();
        }
        self.auth = auth;
        if self.ready() && !was_ready {
            self.focus = Focus::Chats;
            self.status = "已登录".into();
            self.connection_label = "已连接".into();
            self.refresh_chats();
            self.request("folders", json!({}), Job::Folders);
        } else if !self.ready() {
            self.focus = Focus::Login;
        }
    }
    pub fn disconnect(&mut self) {
        self.connected = false;
        self.save_draft();
        self.outbox.clear();
        self.pending.clear();
        self.status = "与本地服务断开，草稿已保留；发送结果未知，请核对历史".into();
        self.connection_label = "已断开".into();
    }
    pub fn tick(&mut self) {
        let expired: Vec<String> = self
            .pending
            .iter()
            .filter(|(_, p)| p.started.elapsed() > Duration::from_secs(35))
            .map(|(id, _)| id.clone())
            .collect();
        for id in expired {
            let pending = self.pending.remove(&id).unwrap();
            self.status = if matches!(pending.job, Job::Send { .. }) {
                "发送结果未知，草稿已保留；请先核对历史".into()
            } else {
                "请求超时，可按 F5 刷新".into()
            };
        }
        if self.connected
            && self.needs_chat_refresh
            && self.last_refresh.elapsed() > Duration::from_secs(1)
        {
            self.refresh_chats();
        }
        if !self.ready()
            && self.connected
            && self.last_refresh.elapsed() > Duration::from_secs(2)
            && !self
                .pending
                .values()
                .any(|p| matches!(p.job, Job::Status | Job::Auth))
        {
            self.request("auth_trigger", json!({}), Job::Status);
            self.last_refresh = Instant::now();
        }
    }
    fn clamp_message_selection(&mut self) {
        self.message_state.select(if self.messages.is_empty() {
            None
        } else {
            Some(
                self.message_state
                    .selected()
                    .unwrap_or(0)
                    .min(self.messages.len() - 1),
            )
        });
    }
    pub fn mark_visible_read(&mut self) {
        if !self.window_focused || !self.ready() || !self.message_query.is_empty() {
            return;
        }
        if let Some(chat) = self.active {
            let ids: Vec<i64> = self
                .selected_message()
                .and_then(|m| m["id"].as_i64())
                .filter(|id| *id > 0)
                .into_iter()
                .collect();
            if !ids.is_empty() {
                self.request(
                    "mark_read",
                    json!({"chat_id":chat,"message_ids":ids}),
                    Job::Draft,
                );
            }
        }
    }
    pub fn key(&mut self, key: KeyEvent) {
        if key.kind == crossterm::event::KeyEventKind::Repeat && key.code == KeyCode::Enter {
            return;
        }
        if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.save_draft();
            self.quit = true;
            return;
        }
        if self.confirm.is_some() {
            if key.code == KeyCode::Esc {
                self.confirm = None;
            } else if key.code == KeyCode::Enter {
                let c = self.confirm.take().unwrap();
                self.request(&c.method, c.params, Job::Action("操作完成".into()));
            }
            return;
        }
        if self.help || self.info.is_some() {
            if matches!(key.code, KeyCode::Esc | KeyCode::F(1)) {
                self.help = false;
                self.info = None;
            } else if self.info.is_some() {
                match key.code {
                    KeyCode::Up => self.info_scroll = self.info_scroll.saturating_sub(1),
                    KeyCode::Down => self.info_scroll = self.info_scroll.saturating_add(1),
                    KeyCode::PageUp => self.info_scroll = self.info_scroll.saturating_sub(10),
                    KeyCode::PageDown => self.info_scroll = self.info_scroll.saturating_add(10),
                    KeyCode::Home => self.info_scroll = 0,
                    _ => {}
                }
            }
            return;
        }
        if key.code == KeyCode::F(1) {
            self.help = true;
            return;
        }
        if key.code == KeyCode::F(5) {
            if !self.connected {
                self.reconnect = true;
            } else {
                self.generation += 1;
                self.load_history(0);
                self.refresh_chats();
            }
            return;
        }
        if !self.ready() {
            self.login_key(key);
            return;
        }
        if key.modifiers.contains(KeyModifiers::ALT) {
            match key.code {
                KeyCode::Char('1') => self.choose_list("main".into()),
                KeyCode::Char('2') => self.choose_list("archive".into()),
                KeyCode::Char(c) if ('3'..='9').contains(&c) => {
                    if let Some(id) = self
                        .folders
                        .get(c as usize - '3' as usize)
                        .and_then(|f| f["id"].as_i64())
                    {
                        self.choose_list(id.to_string());
                    }
                }
                _ => {}
            }
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('k') => {
                    self.focus = Focus::Palette;
                    self.palette.clear();
                    return;
                }
                KeyCode::Char('l') => {
                    self.focus = Focus::ChatSearch;
                    self.search.set(self.chat_query.clone());
                    return;
                }
                KeyCode::Char('f') => {
                    self.focus = Focus::MessageSearch;
                    self.search.set(self.message_query.clone());
                    return;
                }
                KeyCode::Char('b') => {
                    self.narrow_sidebar = !self.narrow_sidebar;
                    self.focus = Focus::Chats;
                    return;
                }
                _ => {}
            }
        }
        if key.code == KeyCode::Tab {
            self.focus = match self.focus {
                Focus::Chats => Focus::Messages,
                Focus::Messages => Focus::Composer,
                _ => Focus::Chats,
            };
            if self.focus == Focus::Composer {
                self.mark_visible_read();
            }
            return;
        }
        if key.code == KeyCode::Esc {
            match self.focus {
                Focus::Palette | Focus::ChatSearch | Focus::MessageSearch => {
                    self.focus = Focus::Composer
                }
                Focus::Composer if self.reply.is_some() || self.edit.is_some() => {
                    self.reply = None;
                    self.edit = None;
                    self.save_draft();
                }
                _ => {
                    self.focus = Focus::Chats;
                    self.narrow_sidebar = true;
                }
            }
            return;
        }
        match self.focus {
            Focus::Chats => match key.code {
                KeyCode::Down | KeyCode::Char('j') => {
                    let index = self.chat_state.selected().unwrap_or(0) + 1;
                    if index + 5 >= self.chats.len() {
                        self.more_chats();
                    }
                    self.select_chat(index.min(self.chats.len().saturating_sub(1)));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.select_chat(self.chat_state.selected().unwrap_or(0).saturating_sub(1))
                }
                KeyCode::Enter | KeyCode::Char('i') => {
                    self.focus = Focus::Composer;
                    self.mark_visible_read();
                }
                KeyCode::Char('/') => {
                    self.focus = Focus::ChatSearch;
                    self.search.clear();
                }
                KeyCode::PageDown => self.more_chats(),
                _ => {}
            },
            Focus::Messages => match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.message_state.select(Some(
                        self.message_state.selected().unwrap_or(0).saturating_sub(1),
                    ));
                    self.clamp_message_selection();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.message_state
                        .select(Some(self.message_state.selected().unwrap_or(0) + 1));
                    self.clamp_message_selection();
                }
                KeyCode::PageUp | KeyCode::Home => {
                    let before = self
                        .messages
                        .first()
                        .and_then(|m| m["id"].as_i64())
                        .unwrap_or(0);
                    self.load_history(before);
                    self.message_state.select(Some(
                        self.message_state.selected().unwrap_or(0).saturating_sub(5),
                    ));
                }
                KeyCode::PageDown => {
                    self.message_state
                        .select(Some(self.message_state.selected().unwrap_or(0) + 5));
                    self.clamp_message_selection();
                }
                KeyCode::End => self
                    .message_state
                    .select(self.messages.len().checked_sub(1)),
                KeyCode::Char('r') => self.reply_selected(false),
                KeyCode::Char('e') => self.reply_selected(true),
                KeyCode::Delete => self.delete_selected(),
                KeyCode::Char('d') => self.download_selected(),
                KeyCode::Char('v') => {
                    if let Some(message) = self.selected_message() {
                        self.info = Some(("完整消息".into(), crate::output::message_text(message)));
                        self.info_scroll = 0;
                    }
                }
                KeyCode::Char('f') => {
                    self.focus = Focus::Palette;
                    self.palette.set("forward ".into());
                }
                KeyCode::Enter => {
                    self.focus = Focus::Composer;
                    self.mark_visible_read();
                }
                _ => {}
            },
            Focus::Composer => {
                if key.code == KeyCode::Char('j') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    self.editor.insert("\n");
                    self.save_draft();
                    return;
                }
                if key.code == KeyCode::Enter {
                    if key
                        .modifiers
                        .intersects(KeyModifiers::SHIFT | KeyModifiers::CONTROL)
                    {
                        self.editor.insert("\n");
                        self.save_draft();
                    } else {
                        self.send();
                    }
                } else if key.code == KeyCode::PageUp {
                    self.focus = Focus::Messages;
                    let before = self
                        .messages
                        .first()
                        .and_then(|m| m["id"].as_i64())
                        .unwrap_or(0);
                    self.load_history(before);
                } else if self.editor.key(key) {
                    self.save_draft();
                }
            }
            Focus::ChatSearch | Focus::MessageSearch => {
                if key.code == KeyCode::Enter {
                    if self.focus == Focus::ChatSearch {
                        self.chat_query = self.search.text.clone();
                        self.refresh_chats();
                        self.focus = Focus::Chats;
                    } else {
                        self.message_query = self.search.text.clone();
                        self.messages.clear();
                        self.generation += 1;
                        self.load_history(0);
                        self.focus = Focus::Messages;
                    }
                } else {
                    self.search.key(key);
                }
            }
            Focus::Palette => {
                if key.code == KeyCode::Enter {
                    let command = self.palette.text.clone();
                    self.focus = Focus::Composer;
                    self.command(&command);
                } else {
                    self.palette.key(key);
                }
            }
            _ => {}
        }
    }
    fn login_key(&mut self, key: KeyEvent) {
        if key.code == KeyCode::Enter {
            if self.pending.values().any(|p| matches!(p.job, Job::Auth)) {
                return;
            }
            let value = self.auth_input.text.clone();
            let action = match self.auth["@type"].as_str().unwrap_or("") {
                "authorizationStateWaitPhoneNumber" => Some(("auth_phone", json!({"phone":value}))),
                "authorizationStateWaitCode" => Some(("auth_code", json!({"code":value}))),
                "authorizationStateWaitPassword" => {
                    Some(("auth_password", json!({"password":value})))
                }
                "authorizationStateWaitEmailAddress" => {
                    Some(("auth_email", json!({"email":value})))
                }
                "authorizationStateWaitEmailCode" => {
                    Some(("auth_email_code", json!({"code":value})))
                }
                _ => None,
            };
            if let Some((method, p)) = action {
                if !value.is_empty() {
                    self.request(method, p, Job::Auth);
                }
            }
        } else if key.code == KeyCode::Char('r') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.request("auth_resend", json!({}), Job::Auth);
        } else if key.code == KeyCode::Char('s') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.request("auth_qr", json!({}), Job::Auth);
        } else {
            self.auth_input.key(key);
        }
    }
    pub fn reply_selected(&mut self, edit: bool) {
        let Some(m) = self.selected_message().cloned() else {
            return;
        };
        let Some(id) = m["id"].as_i64().filter(|id| *id > 0) else {
            self.status = "请等待消息发送完成".into();
            return;
        };
        if edit {
            if m["is_outgoing"] != true || m["content"]["@type"] != "messageText" {
                self.status = "只能编辑自己发送的文本消息".into();
                return;
            }
            self.editor
                .set(m["content"]["text"]["text"].as_str().unwrap_or("").into());
            self.edit = Some(id);
            self.reply = None;
        } else {
            self.reply = Some(id);
            self.edit = None;
        }
        self.focus = Focus::Composer;
        self.save_draft();
    }
    pub fn delete_selected(&mut self) {
        if let (Some(chat), Some(id)) = (
            self.active,
            self.selected_message().and_then(|m| m["id"].as_i64()),
        ) {
            self.confirm = Some(Confirm {
                title: "删除选中消息（仅自己一侧）？".into(),
                method: "delete_message".into(),
                params: json!({"chat_id":chat,"message_id":id,"revoke":false}),
            });
        }
    }
    pub fn download_selected(&mut self) {
        if let Some(id) = self.selected_message().and_then(crate::output::file_id) {
            self.request(
                "download_file",
                json!({"file_id":id}),
                Job::Action(format!("已请求下载文件 {id}")),
            );
        } else {
            self.status = "选中的消息没有可下载附件".into();
        }
    }
    pub fn command(&mut self, command: &str) {
        let command = command.trim();
        let (name, arg) = command.split_once(' ').unwrap_or((command, ""));
        let arg = arg.trim();
        let chat = self.active.unwrap_or(0);
        match name {
            "help" | "帮助" => self.help = true,
            "chats" | "聊天" => self.choose_list("main".into()),
            "archive-list" | "归档列表" => self.choose_list("archive".into()),
            "folder" | "文件夹" => self.choose_list(arg.into()),
            "search" | "搜索" => {
                self.message_query = arg.into();
                self.messages.clear();
                self.generation += 1;
                self.load_history(0);
                self.focus = Focus::Messages;
            }
            "find" | "查找" => {
                self.chat_query = arg.into();
                self.refresh_chats();
                self.focus = Focus::Chats;
            }
            "info" | "资料" => self.request(
                "get_chat",
                json!({"chat_id":chat}),
                Job::Info("聊天资料".into()),
            ),
            "members" | "成员" => self.request(
                "members",
                json!({"chat_id":chat}),
                Job::Info("群成员".into()),
            ),
            "topics" | "话题" => {
                self.request("topics", json!({"chat_id":chat}), Job::Info("话题".into()))
            }
            "topic" => match arg.parse::<i64>() {
                Ok(id) if id >= 0 => {
                    self.topic = if id == 0 { None } else { Some(id) };
                    self.generation += 1;
                    self.messages.clear();
                    self.live.clear();
                    self.deleted.clear();
                    self.reply = None;
                    self.edit = None;
                    self.load_history(0);
                }
                _ => self.status = "topic 后输入话题 ID；0 返回全部消息".into(),
            },
            "pin" | "unpin" => self.request(
                "pin",
                json!({"chat_id":chat,"pinned":name=="pin","list":self.list}),
                Job::Action("置顶设置已更新".into()),
            ),
            "archive" | "unarchive" => self.request(
                "archive",
                json!({"chat_id":chat,"archived":name=="archive"}),
                Job::Action("归档设置已更新".into()),
            ),
            "mute" | "unmute" => self.request(
                "mute",
                json!({"chat_id":chat,"seconds":if name=="mute"{3600}else{0}}),
                Job::Action("通知设置已更新".into()),
            ),
            "read" | "已读" => self.mark_visible_read(),
            "reply" | "回复" => self.reply_selected(false),
            "edit" | "编辑" => self.reply_selected(true),
            "delete" | "删除" => self.delete_selected(),
            "download" | "下载" => self.download_selected(),
            "forward" | "转发" => {
                if let Some(id) = self.selected_message().and_then(|m| m["id"].as_i64()) {
                    if !arg.is_empty() {
                        self.confirm = Some(Confirm {
                            title: format!("将选中消息转发给 {arg}？"),
                            method: "forward_message".into(),
                            params: json!({"from_chat_id":chat,"to_chat_id":arg,"message_id":id}),
                        });
                    }
                }
            }
            "attach" | "附件" => {
                let path = std::path::Path::new(arg.trim_matches('"'));
                match std::fs::canonicalize(path) {
                    Ok(path) if path.is_file() => {
                        self.confirm = Some(Confirm {
                            title: format!(
                                "发送附件 {}？",
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            method: "send_file".into(),
                            params: json!({"chat_id":chat,"path":path,"reply_to":self.reply}),
                        });
                    }
                    _ => self.status = "附件不存在，请输入 attach 完整路径".into(),
                }
            }
            "logout" | "退出登录" => {
                self.confirm = Some(Confirm {
                    title: "退出 Telegram 账号？".into(),
                    method: "logout".into(),
                    params: json!({}),
                })
            }
            "quit" | "退出" => {
                self.save_draft();
                self.quit = true;
            }
            _ => self.status = "未知命令。Ctrl+K 打开命令面板，F1 查看帮助".into(),
        }
    }
}
fn format_info(title: &str, value: &Value) -> String {
    let text=match title {
        "聊天资料"=>format!("{}\n\n聊天 ID：{}\n类型：{}\n未读消息：{}\n通知：{}\n\n在命令面板使用 members 查看成员，topics 查看话题。",
            value["title"].as_str().unwrap_or(""),value["id"],
            match value["type"]["@type"].as_str().unwrap_or("") {"chatTypePrivate"=>"私聊","chatTypeBasicGroup"=>"群组","chatTypeSupergroup"=>"超级群组 / 频道",_=>"聊天"},
            value["unread_count"],if value["notification_settings"]["mute_for"].as_i64().unwrap_or(0)>0 {"静音"}else{"开启"}),
        "群成员"=>value["members"].as_array().into_iter().flatten().map(|m|format!("成员 {} · {}",
            m["member_id"]["user_id"],m["status"]["@type"].as_str().unwrap_or("").trim_start_matches("chatMemberStatus"))).collect::<Vec<_>>().join("\n"),
        "话题"=>value["topics"].as_array().into_iter().flatten().map(|t|format!("{}   {}\n  使用 topic {} 打开\n",
            t["info"]["forum_topic_id"],t["info"]["name"].as_str().unwrap_or(""),t["info"]["forum_topic_id"])).collect::<Vec<_>>().join("\n"),
        _=>"操作已完成".into(),
    };
    crate::output::safe(&text)
}
fn connection_label(state: &str) -> &str {
    match state {
        "connectionStateReady" => "已连接",
        "connectionStateConnecting" => "连接 Telegram 中",
        "connectionStateConnectingToProxy" => "连接代理中",
        "connectionStateUpdating" => "同步中",
        _ => "等待网络",
    }
}
