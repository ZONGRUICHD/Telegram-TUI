//! `tg tui` — ratatui terminal interface.

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame, Terminal,
};
use tokio::sync::mpsc;

use tg_core::config::TgConfig;
use tg_ipc::client::{IpcClient, IpcWriter};
use tg_ipc::protocol::{methods, ServerMessage};

#[derive(PartialEq)]
enum Focus {
    Dialogs,
    Input,
}

struct App {
    dialogs: Vec<(i64, String, i32)>,
    selected: usize,
    messages: Vec<(i64, String, String, String)>, // (id, sender, text, time)
    input: String,
    focus: Focus,
    status: String,
    running: bool,
}

impl App {
    fn new() -> Self {
        Self {
            dialogs: Vec::new(),
            selected: 0,
            messages: Vec::new(),
            input: String::new(),
            focus: Focus::Dialogs,
            status: "Starting…".into(),
            running: true,
        }
    }
    fn current_chat(&self) -> Option<i64> {
        self.dialogs.get(self.selected).map(|(id, _, _)| *id)
    }
}

pub async fn run(config: TgConfig) -> Result<()> {
    let socket = &config.ipc.socket_path;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let client = IpcClient::connect(socket).await?;
    let (mut writer, mut reader) = client.split();

    // Background task: read all server messages and forward to UI
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(128);
    tokio::spawn(async move {
        while let Ok(msg) = reader.read_message().await {
            if msg_tx.send(msg).await.is_err() {
                break;
            }
        }
    });

    // Request initial dialogs
    writer
        .send_request(&tg_ipc::protocol::Request {
            id: uuid::Uuid::new_v4().to_string(),
            method: methods::LIST_DIALOGS.to_string(),
            params: serde_json::json!({"limit": 50}),
        })
        .await?;

    let mut app = App::new();
    app.status = "Loading…".into();

    let tick = Duration::from_millis(100);
    let mut last_tick = Instant::now();

    loop {
        terminal.draw(|f| ui(f, &app))?;
        let timeout = tick
            .checked_sub(last_tick.elapsed())
            .unwrap_or(Duration::ZERO);

        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                handle_key(key, &mut app, &mut writer).await?;
            }
        }
        if last_tick.elapsed() >= tick {
            last_tick = Instant::now();
        }

        while let Ok(msg) = msg_rx.try_recv() {
            handle_event(&msg, &mut app);
        }
        if !app.running {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

async fn handle_key(key: KeyEvent, app: &mut App, writer: &mut IpcWriter) -> Result<()> {
    match app.focus {
        Focus::Dialogs => match key.code {
            KeyCode::Char('q') | KeyCode::Esc => app.running = false,
            KeyCode::Char('i') | KeyCode::Enter => app.focus = Focus::Input,
            KeyCode::Char('j') | KeyCode::Down => {
                if app.selected < app.dialogs.len().saturating_sub(1) {
                    app.selected += 1;
                    load_msgs(app, writer).await?;
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if app.selected > 0 {
                    app.selected -= 1;
                    load_msgs(app, writer).await?;
                }
            }
            KeyCode::Char('/') => {
                app.focus = Focus::Input;
                app.input = "/".into();
            }
            _ => {}
        },
        Focus::Input => match key.code {
            KeyCode::Esc => {
                app.input.clear();
                app.focus = Focus::Dialogs;
            }
            KeyCode::Enter => {
                let text: String = std::mem::take(&mut app.input);
                if !text.is_empty() {
                    if text.starts_with("/q") {
                        app.running = false;
                    } else if let Some(chat) = app.current_chat() {
                        writer
                            .send_request(&tg_ipc::protocol::Request {
                                id: uuid::Uuid::new_v4().to_string(),
                                method: methods::SEND_MESSAGE.to_string(),
                                params: serde_json::json!({"chat_id": chat, "text": text}),
                            })
                            .await?;
                    }
                }
            }
            KeyCode::Char(c)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                app.input.push(c);
            }
            KeyCode::Backspace => {
                app.input.pop();
            }
            _ => {}
        },
    }
    Ok(())
}

async fn load_msgs(app: &mut App, writer: &mut IpcWriter) -> Result<()> {
    if let Some(chat) = app.current_chat() {
        writer
            .send_request(&tg_ipc::protocol::Request {
                id: uuid::Uuid::new_v4().to_string(),
                method: methods::GET_MESSAGES.to_string(),
                params: serde_json::json!({"chat_id": chat, "limit": 200}),
            })
            .await?;
    }
    Ok(())
}

fn handle_event(msg: &ServerMessage, app: &mut App) {
    match msg {
        ServerMessage::Response(resp) => {
            if let Some(result) = &resp.result {
                // Chat list: handler returns array of Chat objects
                if let Some(arr) = result.as_array() {
                    if arr
                        .first()
                        .map(|v| v.get("title").is_some())
                        .unwrap_or(false)
                    {
                        app.dialogs.clear();
                        for item in arr {
                            let id = item["id"].as_i64().unwrap_or(0);
                            let title = item["title"].as_str().unwrap_or("?").to_string();
                            let unread = item["unread_count"].as_i64().unwrap_or(0) as i32;
                            app.dialogs.push((id, title, unread));
                        }
                        app.status = format!("{} chats", app.dialogs.len());
                    }
                }

                // Messages: TDLib getChatHistory returns { "messages": [...] }
                if let Some(msgs) = result.get("messages").and_then(|v| v.as_array()) {
                    app.messages.clear();
                    app.status = format!("{} messages", msgs.len());
                    for m in msgs {
                        let id = m["id"].as_i64().unwrap_or(0);
                        let is_out = m["is_outgoing"].as_bool().unwrap_or(false);
                        let sender = if is_out {
                            "Me".to_string()
                        } else {
                            m["sender_id"]["user_id"]
                                .as_i64()
                                .map(|u| format!("user#{u}"))
                                .unwrap_or_else(|| "system".into())
                        };
                        let text = m["content"]["text"]["text"]
                            .as_str()
                            .map(|s| s.to_string())
                            .or_else(|| {
                                m["content"]["caption"]["text"]
                                    .as_str()
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| detect_content_label(m));
                        let ts = m["date"].as_i64().unwrap_or(0);
                        app.messages.push((id, sender, text, fmt_time(ts)));
                    }
                } else {
                    let keys: Vec<String> = result
                        .as_object()
                        .map(|o| o.keys().cloned().collect())
                        .unwrap_or_default();
                    app.status = format!("resp keys: {:?}", keys);
                }
            }
            if let Some(err) = &resp.error {
                app.status = format!("❌ {}", err.message);
            }
        }
        ServerMessage::Event(ev) => {
            if ev.name == "new_message" {
                app.status = "📨 New message".into();
            }
        }
        ServerMessage::AuthState(a) => {
            app.status = format!("🔐 {}", a.state);
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Min(1),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    f.render_widget(
        Paragraph::new(" 📱 tg tui │ j/k navigate │ i input │ /q quit ").style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        chunks[0],
    );

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(chunks[1]);

    // Chat list
    let ds = if app.focus == Focus::Dialogs {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    let items: Vec<ListItem> = app
        .dialogs
        .iter()
        .enumerate()
        .map(|(i, (_id, title, unread))| {
            let s = if i == app.selected {
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::White)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let p = if *unread > 0 {
                format!("({unread}) ")
            } else {
                String::new()
            };
            ListItem::new(Line::from(Span::styled(format!("{p}{title}"), s)))
        })
        .collect();
    f.render_widget(
        List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Chats ")
                .border_style(ds),
        ),
        body[0],
    );

    // Messages
    let msgs: Vec<Line> = app
        .messages
        .iter()
        .map(|(_, sender, text, time)| {
            let sender_color = if sender == "Me" {
                Color::Green
            } else {
                Color::Cyan
            };
            Line::from(vec![
                Span::styled(format!("[{time}] "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    format!("{sender}: "),
                    Style::default()
                        .fg(sender_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(text.as_str()),
            ])
        })
        .collect();
    f.render_widget(
        Paragraph::new(msgs)
            .block(Block::default().borders(Borders::ALL).title(" Messages "))
            .wrap(Wrap { trim: false }),
        body[1],
    );

    // Input
    let is = if app.focus == Focus::Input {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    };
    f.render_widget(
        Paragraph::new(format!("{}█", app.input)).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Input ")
                .border_style(is),
        ),
        chunks[2],
    );

    f.render_widget(
        Paragraph::new(app.status.clone()).style(Style::default().fg(Color::DarkGray)),
        chunks[3],
    );
}

fn fmt_time(ts: i64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH + Duration::from_secs(ts as u64);
    let dt: chrono::DateTime<chrono::Utc> = dt.into();
    dt.format("%H:%M").to_string()
}

fn detect_content_label(m: &serde_json::Value) -> String {
    let msg_type = m["content"]["@type"].as_str().unwrap_or("");
    match msg_type {
        "messagePhoto" => "📷 photo".into(),
        "messageVideo" => "🎬 video".into(),
        "messageVideoNote" => "🎥 video note".into(),
        "messageAnimation" => "🎞️ gif".into(),
        "messageSticker" => {
            let emoji = m["content"]["sticker"]["emoji"].as_str().unwrap_or("🏷️");
            format!("{emoji} sticker")
        }
        "messageDocument" => {
            let name = m["content"]["document"]["file_name"]
                .as_str()
                .unwrap_or("file");
            format!("📄 {name}")
        }
        "messageVoiceNote" => "🎤 voice".into(),
        "messageAudio" => {
            let title = m["content"]["audio"]["title"].as_str().unwrap_or("audio");
            format!("🎵 {title}")
        }
        "messageLocation" => "📍 location".into(),
        "messageContact" => "👤 contact".into(),
        "messagePoll" => {
            let question = m["content"]["poll"]["question"]["text"]
                .as_str()
                .unwrap_or("poll");
            format!("📊 {question}")
        }
        "messageCall" => "📞 call".into(),
        "messageGame" => "🎮 game".into(),
        "messageInvoice" => "💰 invoice".into(),
        "" => "[empty]".into(),
        _ => format!("[{}]", msg_type.strip_prefix("message").unwrap_or(msg_type)),
    }
}
