use super::{
    app::{App, Focus, Job},
    demo::Demo,
    view,
};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::{json, Value};
use tg_ipc::protocol::{Response, RpcError, ServerMessage};

fn response(id: String, result: Value) -> ServerMessage {
    ServerMessage::Response(Response {
        id,
        result: Some(result),
        error: None,
    })
}
fn message(id: i64, chat: i64, text: &str) -> Value {
    json!({"id":id,"chat_id":chat,"content":{"@type":"messageText","text":{"text":text}}})
}

#[test]
fn switching_chat_discards_old_history() {
    let mut app = App::new(50, false);
    app.auth = json!({"@type":"authorizationStateReady"});
    app.chats = vec![json!({"id":1,"title":"A"}), json!({"id":2,"title":"B"})];
    app.select_chat(0);
    let old = app
        .outbox
        .iter()
        .find(|r| r.method == "get_messages")
        .unwrap()
        .id
        .clone();
    app.select_chat(1);
    let new = app
        .outbox
        .iter()
        .rev()
        .find(|r| r.method == "get_messages")
        .unwrap()
        .id
        .clone();
    app.handle(response(old, json!({"messages":[message(10,1,"旧聊天")]})));
    assert!(app.messages.is_empty());
    app.handle(response(new, json!({"messages":[message(20,2,"新聊天")]})));
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0]["chat_id"], 2);
}

#[test]
fn send_failure_and_new_typing_preserve_draft() {
    let mut app = Demo::new().app();
    app.editor.set("第一条".into());
    app.send();
    let id = app.outbox.back().unwrap().id.clone();
    app.handle(ServerMessage::Response(Response {
        id,
        result: None,
        error: Some(RpcError {
            code: 403,
            message: "无法发送".into(),
        }),
    }));
    assert_eq!(app.editor.text, "第一条");
    app.send();
    let id = app.outbox.back().unwrap().id.clone();
    app.editor.set("下一条草稿".into());
    app.save_draft();
    app.handle(response(id, message(2001, -100100, "第一条")));
    assert_eq!(app.editor.text, "下一条草稿");
    assert_eq!(app.drafts[&-100100].text, "下一条草稿");
}

#[test]
fn empty_chat_result_clears_stale_display() {
    let mut app = Demo::new().app();
    app.refresh_chats();
    let id = app.outbox.back().unwrap().id.clone();
    app.handle(response(id, json!({"chats":[]})));
    assert!(app.chats.is_empty());
    assert!(app.messages.is_empty());
    assert!(app.active.is_none());
}

#[test]
fn receive_send_confirmation_replaces_temporary_id() {
    let mut app = Demo::new().app();
    app.messages = vec![message(-7, -100100, "发送中")];
    app.handle(ServerMessage::Event(tg_ipc::protocol::Event {
        name: "updateMessageSendSucceeded".into(),
        data: json!({"old_message_id":-7,"message":message(8001,-100100,"已发送")}),
    }));
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0]["id"], 8001);
}

#[test]
fn a_late_history_page_cannot_erase_new_messages_or_resurrect_deleted_ones() {
    let mut app = Demo::new().app();
    app.messages.clear();
    app.load_history(0);
    let id = app.outbox.back().unwrap().id.clone();
    app.handle(ServerMessage::Event(tg_ipc::protocol::Event {
        name: "updateNewMessage".into(),
        data: json!({"message":message(9002,-100100,"新消息")}),
    }));
    app.handle(ServerMessage::Event(tg_ipc::protocol::Event {
        name: "updateDeleteMessages".into(),
        data: json!({"chat_id":-100100,"message_ids":[9001]}),
    }));
    app.handle(response(
        id,
        json!({"messages":[message(9001,-100100,"已删除")]}),
    ));
    assert_eq!(app.messages.len(), 1);
    assert_eq!(app.messages[0]["id"], 9002);
}

#[test]
fn topic_view_filters_other_topic_updates() {
    let mut app = Demo::new().app();
    app.messages.clear();
    app.topic = Some(42);
    let mut m = message(9001, -100100, "另一个话题");
    m["topic_id"] = json!({"forum_topic_id":43});
    app.handle(ServerMessage::Event(tg_ipc::protocol::Event {
        name: "updateNewMessage".into(),
        data: json!({"message":m}),
    }));
    assert!(app.messages.is_empty());
}

#[test]
fn layouts_render_narrow_wide_and_login_without_exposing_password() {
    for (w, h) in [(30, 10), (42, 12), (60, 24), (80, 24), (120, 36), (160, 48)] {
        let mut app = Demo::new().app();
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| view::draw(f, &mut app)).unwrap();
        app.focus = Focus::Palette;
        terminal.draw(|f| view::draw(f, &mut app)).unwrap();
        app.set_auth(json!({"@type":"authorizationStateWaitPassword","password_hint":"提示"}));
        app.auth_input.set("secret-password-不应出现在画面".into());
        terminal.draw(|f| view::draw(f, &mut app)).unwrap();
        let rendered = format!("{:?}", terminal.backend().buffer());
        assert!(!rendered.contains("secret-password"));
    }
}

#[test]
fn background_and_search_do_not_mark_messages_read() {
    let mut app = Demo::new().app();
    app.window_focused = false;
    app.mark_visible_read();
    assert!(app.outbox.is_empty());
    app.window_focused = true;
    app.message_query = "search".into();
    app.mark_visible_read();
    assert!(app.outbox.is_empty());
    app.message_query.clear();
    app.mark_visible_read();
    assert_eq!(app.outbox.back().unwrap().method, "mark_read");
    assert!(app.pending.values().any(|p| matches!(p.job, Job::Draft)));
}
