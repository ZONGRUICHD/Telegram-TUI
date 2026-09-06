use super::{
    app::{App, Focus},
    editor::wrap,
};
use ratatui::{
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};

const BG: Color = Color::Rgb(18, 20, 25);
const PANEL: Color = Color::Rgb(24, 27, 33);
const FG: Color = Color::Rgb(225, 228, 234);
const MUTED: Color = Color::Rgb(130, 141, 158);
const BORDER: Color = Color::Rgb(49, 56, 68);
const ACCENT: Color = Color::Rgb(219, 166, 115);
const TEAL: Color = Color::Rgb(103, 206, 181);
const SELECT: Color = Color::Rgb(40, 48, 61);
fn style(color: Color) -> Style {
    Style::default().fg(color)
}
fn block(title: &str, active: bool) -> Block<'_> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .title(format!(" {title} "))
        .border_style(style(if active { ACCENT } else { BORDER }))
        .style(Style::default().bg(PANEL).fg(FG))
}
fn centered(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = f.area();
    app.hit = Default::default();
    f.render_widget(Block::default().style(Style::default().bg(BG).fg(FG)), area);
    if area.width < 42 || area.height < 12 {
        f.render_widget(
            Paragraph::new("Telegram-TUI\n\n请将终端放大到至少 42 × 12\nCtrl+Q 退出")
                .style(style(FG)),
            area,
        );
        return;
    }
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    let mode = if app.demo {
        "演示 · 离线数据"
    } else {
        &app.connection_label
    };
    let header = Layout::horizontal([Constraint::Min(20), Constraint::Length(28)]).split(rows[0]);
    f.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" ◈ ", style(ACCENT)),
            Span::styled("Telegram", style(FG).add_modifier(Modifier::BOLD)),
            Span::styled("  /  TUI", style(MUTED)),
        ])),
        header[0],
    );
    f.render_widget(
        Paragraph::new(format!("● {mode}  "))
            .alignment(Alignment::Right)
            .style(style(if app.connected { TEAL } else { ACCENT })),
        header[1],
    );
    if !app.ready() {
        login(f, app, rows[2]);
    } else {
        let mut tabs = vec![];
        for (value, name, key) in [("main", "全部聊天", "1"), ("archive", "归档", "2")] {
            tabs.push(Span::styled(
                format!("  {key} {name}  "),
                style(if app.list == value { ACCENT } else { MUTED }),
            ));
        }
        for (i, folder) in app.folders.iter().take(7).enumerate() {
            let name = folder["name"]["text"]["text"]
                .as_str()
                .or_else(|| folder["title"].as_str())
                .unwrap_or("文件夹");
            tabs.push(Span::styled(
                format!("  {} {}  ", i + 3, crate::output::safe(name)),
                style(if folder["id"].as_i64() == app.list.parse::<i64>().ok() {
                    ACCENT
                } else {
                    MUTED
                }),
            ));
        }
        f.render_widget(Paragraph::new(Line::from(tabs)), rows[1]);
        let narrow = area.width < 84;
        let show_chats = !narrow || app.narrow_sidebar || app.active.is_none();
        let body = if show_chats && !narrow {
            Layout::horizontal([Constraint::Length(30), Constraint::Min(20)]).split(rows[2])
        } else {
            Layout::horizontal([
                Constraint::Percentage(if show_chats { 100 } else { 0 }),
                Constraint::Min(0),
            ])
            .split(rows[2])
        };
        if show_chats {
            chats(f, app, body[0]);
        }
        if !narrow || !show_chats {
            conversation(f, app, body[1]);
        }
    }
    f.render_widget(
        Paragraph::new(format!(
            " {}",
            crate::output::safe(&app.status).replace('\n', " ")
        ))
        .style(style(MUTED)),
        rows[3],
    );
    f.render_widget(
        Paragraph::new(
            " Ctrl+K 命令   Ctrl+L 查找   Ctrl+F 搜索消息   Tab 切换   F1 帮助   Ctrl+Q 退出",
        )
        .style(style(MUTED)),
        rows[4],
    );
    if app.focus == Focus::Palette {
        palette(f, app, area);
    }
    if app.focus == Focus::MessageSearch {
        search_modal(f, app, area);
    }
    if app.help {
        help(f, area);
    }
    if let Some((title, text)) = &app.info {
        let rect = centered(area, 90, area.height.saturating_sub(4));
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(text.as_str())
                .wrap(Wrap { trim: false })
                .scroll((app.info_scroll, 0))
                .block(block(&format!("{title} · ↑↓ 滚动 · Esc 关闭"), true)),
            rect,
        );
    }
    if let Some(confirm) = &app.confirm {
        let rect = centered(area, 64, 8);
        f.render_widget(Clear, rect);
        f.render_widget(
            Paragraph::new(format!(
                "\n {}\n\n Enter 确认     Esc 取消",
                crate::output::safe(&confirm.title)
            ))
            .wrap(Wrap { trim: false })
            .block(block("确认操作", true)),
            rect,
        );
    }
}
fn chats(f: &mut Frame, app: &mut App, area: Rect) {
    let rows = Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).split(area);
    app.hit.search = rows[0];
    app.hit.chats = rows[1];
    let text = if app.focus == Focus::ChatSearch {
        app.search.text.as_str()
    } else if !app.chat_query.is_empty() {
        &app.chat_query
    } else {
        "查找聊天 · Ctrl+L"
    };
    f.render_widget(
        Paragraph::new(crate::output::safe(text))
            .block(block("聊天", app.focus == Focus::ChatSearch)),
        rows[0],
    );
    if app.focus == Focus::ChatSearch {
        cursor(f, &app.search, rows[0], false);
    }
    let width = rows[1].width.saturating_sub(4) as usize;
    let items: Vec<ListItem> = app
        .chats
        .iter()
        .map(|chat| {
            let unread = chat["unread_count"].as_i64().unwrap_or(0);
            let pinned = chat["positions"]
                .as_array()
                .is_some_and(|p| p.iter().any(|p| p["is_pinned"] == true));
            let title = crate::output::safe(chat["title"].as_str().unwrap_or("未知聊天"));
            let draft = chat["draft_message"].is_object()
                || chat["id"]
                    .as_i64()
                    .and_then(|id| app.drafts.get(&id))
                    .is_some_and(|d| !d.text.is_empty());
            let preview = if draft {
                "草稿".into()
            } else {
                crate::output::message_text(&chat["last_message"]).replace('\n', " ")
            };
            let prefix = if pinned { "↑ " } else { "" };
            let badge = if unread > 0 {
                format!("  {unread}")
            } else {
                String::new()
            };
            ListItem::new(vec![
                Line::from(vec![
                    Span::styled(
                        format!("{prefix}{title}"),
                        style(FG).add_modifier(if unread > 0 {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                    ),
                    Span::styled(badge, style(TEAL)),
                ]),
                Line::from(Span::styled(
                    wrap(&preview, width).first().cloned().unwrap_or_default(),
                    style(if draft { ACCENT } else { MUTED }),
                )),
                Line::from(""),
            ])
        })
        .collect();
    f.render_stateful_widget(
        List::new(items)
            .block(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(style(BORDER)),
            )
            .highlight_style(Style::default().bg(SELECT))
            .highlight_symbol("▎ "),
        rows[1],
        &mut app.chat_state,
    );
    let mut y = rows[1].y;
    for index in app.chat_state.offset()..app.chats.len() {
        if y >= rows[1].bottom() {
            break;
        }
        app.hit.chat_rows.push((
            Rect::new(rows[1].x, y, rows[1].width, 3.min(rows[1].bottom() - y)),
            index,
        ));
        y += 3;
    }
    if app.chats.is_empty() {
        f.render_widget(
            Paragraph::new("\n 没有匹配的聊天\n\n Ctrl+L 更换关键词\n F5 刷新").style(style(MUTED)),
            rows[1],
        );
    }
}
fn conversation(f: &mut Frame, app: &mut App, area: Rect) {
    if app.active.is_none() {
        f.render_widget(
            Paragraph::new(
                "\n\n\n选择一个聊天\n\n使用 ↑↓ 浏览，Enter 开始聊天\nCtrl+K 打开命令面板",
            )
            .alignment(Alignment::Center)
            .style(style(MUTED)),
            area,
        );
        return;
    }
    let input_lines = wrap(&app.editor.text, area.width.saturating_sub(4) as usize)
        .len()
        .clamp(1, 5) as u16;
    let rows = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(2),
        Constraint::Length(input_lines + 3),
    ])
    .split(area);
    let title = app
        .chat()
        .and_then(|c| c["title"].as_str())
        .unwrap_or("聊天");
    let subtitle = if !app.message_query.is_empty() {
        format!("搜索：{}", app.message_query)
    } else if let Some(topic) = app.topic {
        format!("话题 {topic}")
    } else {
        "消息已同步到此设备".into()
    };
    let title = crate::output::safe(title).to_string();
    f.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled(
                format!(" {title}"),
                style(FG).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(format!(" {subtitle}"), style(MUTED))),
        ])
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(style(BORDER)),
        ),
        rows[0],
    );
    app.hit.messages = rows[1];
    app.hit.composer = rows[2];
    let width = rows[1].width.saturating_sub(5) as usize;
    let mut heights = vec![];
    let last_read = app
        .chat()
        .and_then(|c| c["last_read_outbox_message_id"].as_i64())
        .unwrap_or(0);
    let items: Vec<ListItem> = app
        .messages
        .iter()
        .map(|message| {
            let outgoing = message["is_outgoing"] == true;
            let sending = match message["sending_state"]["@type"].as_str().unwrap_or("") {
                "messageSendingStatePending" => " ◷",
                "messageSendingStateFailed" => " ! 发送失败",
                _ => {
                    if outgoing {
                        if message["id"].as_i64().unwrap_or(0) <= last_read {
                            " ✓✓"
                        } else {
                            " ✓"
                        }
                    } else {
                        ""
                    }
                }
            };
            let mut lines = vec![Line::from(vec![
                Span::styled(
                    crate::output::sender(message),
                    style(if outgoing { TEAL } else { ACCENT }).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  {}{sending}",
                        crate::output::time(message["date"].as_i64().unwrap_or(0), "%m-%d %H:%M")
                    ),
                    style(MUTED),
                ),
            ])];
            if let Some(reply) = message["reply_to"]["message_id"].as_i64() {
                lines.push(Line::from(Span::styled(
                    format!("↪ 回复 #{reply}"),
                    style(MUTED),
                )));
            }
            let text = crate::output::message_text(message);
            let wrapped = wrap(&text, width);
            let max_lines = (rows[1].height as usize).saturating_sub(5).clamp(1, 14);
            for line in wrapped.iter().take(max_lines) {
                lines.push(Line::from(line.clone()));
            }
            if wrapped.len() > max_lines {
                lines.push(Line::from(Span::styled(
                    "… 按 v 查看完整消息",
                    style(MUTED),
                )));
            }
            lines.push(Line::from(""));
            heights.push(lines.len() as u16);
            ListItem::new(lines)
        })
        .collect();
    f.render_stateful_widget(
        List::new(items)
            .block(Block::default().padding(ratatui::widgets::Padding::new(2, 1, 1, 0)))
            .highlight_style(Style::default().bg(if app.focus == Focus::Messages {
                SELECT
            } else {
                BG
            })),
        rows[1],
        &mut app.message_state,
    );
    let mut y = rows[1].y + 1;
    for (index, height) in heights.iter().enumerate().skip(app.message_state.offset()) {
        if y >= rows[1].bottom() {
            break;
        }
        app.hit.message_rows.push((
            Rect::new(
                rows[1].x,
                y,
                rows[1].width,
                (*height).min(rows[1].bottom() - y),
            ),
            index,
        ));
        y += height;
    }
    if app.messages.is_empty() {
        f.render_widget(
            Paragraph::new("\n 暂无消息，或正在加载…").style(style(MUTED)),
            rows[1],
        );
    }
    let composing = if let Some(id) = app.edit {
        format!("编辑 #{id} · Esc 取消")
    } else if let Some(id) = app.reply {
        format!("回复 #{id} · Esc 取消")
    } else {
        "消息 · Enter 发送 · Shift+Enter 换行".into()
    };
    let input_block = block(&composing, app.focus == Focus::Composer);
    let inner = input_block.inner(rows[2]);
    let (_, cursor_row) = app.editor.cursor_position(inner.width as usize);
    let scroll = (cursor_row as u16).saturating_sub(inner.height.saturating_sub(1));
    let content = if app.editor.text.is_empty() {
        "输入消息…".to_owned()
    } else {
        app.editor.text.clone()
    };
    f.render_widget(
        Paragraph::new(content)
            .wrap(Wrap { trim: false })
            .scroll((scroll, 0))
            .style(style(if app.editor.text.is_empty() {
                MUTED
            } else {
                FG
            }))
            .block(input_block),
        rows[2],
    );
    if app.focus == Focus::Composer {
        cursor(f, &app.editor, rows[2], false);
    }
}
fn cursor(f: &mut Frame, editor: &super::editor::Editor, area: Rect, hidden: bool) {
    if area.width <= 2 || area.height <= 2 {
        return;
    }
    let (x, y) = if hidden {
        (
            editor
                .text
                .chars()
                .count()
                .min(area.width.saturating_sub(3) as usize),
            0,
        )
    } else {
        editor.cursor_position(area.width.saturating_sub(2) as usize)
    };
    f.set_cursor_position((
        area.x + 1 + x as u16,
        area.y + 1 + (y as u16).min(area.height - 3),
    ));
}
fn login(f: &mut Frame, app: &App, area: Rect) {
    let state = app.auth["@type"].as_str().unwrap_or("");
    let (heading, prompt, hidden) = match state {
        "authorizationStateWaitPhoneNumber" => ("登录 Telegram", "手机号（例如 +86…）", false),
        "authorizationStateWaitCode" => ("输入验证码", "验证码", true),
        "authorizationStateWaitPassword" => ("两步验证", "密码", true),
        "authorizationStateWaitEmailAddress" => ("验证登录邮箱", "邮箱地址", false),
        "authorizationStateWaitEmailCode" => ("验证登录邮箱", "邮箱验证码", true),
        "authorizationStateWaitOtherDeviceConfirmation" => {
            ("在另一台设备确认", "请打开已登录的 Telegram 确认", false)
        }
        "authorizationStateWaitRegistration" => {
            ("此号码尚未注册", "请先通过官方 Telegram 注册账号", false)
        }
        "authorizationStateWaitPremiumPurchase" => (
            "需要在官方客户端完成登录",
            "请在官方 Telegram 处理购买要求后重试",
            false,
        ),
        "authorizationStateClosed"
        | "authorizationStateClosing"
        | "authorizationStateLoggingOut" => ("会话已关闭", "退出后重新运行 tg login", false),
        _ => ("正在连接", "正在读取登录状态…", false),
    };
    let rect = centered(area, 66, 17);
    if rect.height < 16 {
        let value = if hidden {
            "•".repeat(app.auth_input.text.chars().count())
        } else {
            app.auth_input.text.clone()
        };
        f.render_widget(Paragraph::new(value).block(block(prompt, true)), rect);
        cursor(f, &app.auth_input, rect, hidden);
        return;
    }
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                "欢迎使用 Telegram-TUI",
                style(FG).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "手机号  →  验证码  →  两步验证",
                style(ACCENT),
            )),
            Line::from(""),
            Line::from(crate::output::safe(&crate::login::auth_hint(&app.auth))),
        ])
        .alignment(Alignment::Center)
        .block(block(heading, true)),
        rect,
    );
    let field = Rect::new(rect.x + 3, rect.y + 8, rect.width.saturating_sub(6), 3);
    let value = if hidden {
        "•".repeat(app.auth_input.text.chars().count())
    } else {
        app.auth_input.text.clone()
    };
    f.render_widget(Paragraph::new(value).block(block(prompt, true)), field);
    cursor(f, &app.auth_input, field, hidden);
    let hint = Rect::new(rect.x + 2, rect.y + 12, rect.width.saturating_sub(4), 3);
    let text = if state == "authorizationStateWaitOtherDeviceConfirmation" {
        format!(
            "在已登录设备中打开确认链接：\n{}",
            app.auth["link"].as_str().unwrap_or("")
        )
    } else {
        "Enter 继续   Ctrl+R 重发验证码   Ctrl+S 设备确认\n你的会话保存在此设备，下次自动登录"
            .into()
    };
    f.render_widget(
        Paragraph::new(text)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: false })
            .style(style(MUTED)),
        hint,
    );
}
fn palette(f: &mut Frame, app: &App, area: Rect) {
    let rect = centered(area, 76, 19);
    f.render_widget(Clear, rect);
    let lines = vec![
        "输入命令和参数后回车",
        "",
        "find 关键词          查找聊天",
        "search 关键词        搜索当前聊天（空关键词返回历史）",
        "reply / edit         回复 / 编辑选中消息",
        "forward @username    转发选中消息",
        "attach 完整路径      发送文件",
        "download             下载选中附件",
        "pin / unpin          置顶 / 取消置顶",
        "archive / unarchive  归档 / 移出归档",
        "mute / unmute        静音一小时 / 取消静音",
        "info / members       聊天资料 / 群成员",
        "topics / topic ID    查看话题 / 切换话题（0 返回全部）",
        "folder ID            切换聊天文件夹",
        "logout / quit        退出账号 / 退出程序",
    ];
    f.render_widget(
        Paragraph::new(lines.join("\n"))
            .block(block("命令面板 · Esc 返回", true))
            .style(style(MUTED)),
        rect,
    );
    let input = Rect::new(
        rect.x + 1,
        rect.bottom().saturating_sub(4),
        rect.width.saturating_sub(2),
        3,
    );
    f.render_widget(
        Paragraph::new(app.palette.text.clone()).block(block("›", true)),
        input,
    );
    cursor(f, &app.palette, input, false);
}
fn search_modal(f: &mut Frame, app: &App, area: Rect) {
    let rect = centered(area, 64, 5);
    f.render_widget(Clear, rect);
    f.render_widget(
        Paragraph::new(app.search.text.clone())
            .block(block("搜索此聊天 · Enter 搜索 · 空关键词恢复", true)),
        rect,
    );
    cursor(f, &app.search, rect, false);
}
fn help(f: &mut Frame, area: Rect) {
    let rect = centered(area, 80, 23);
    f.render_widget(Clear, rect);
    let text="聊天\n  ↑↓ / j k  切换聊天        Enter  输入消息\n  Alt+1 全部  Alt+2 归档     Alt+3…9 文件夹\n  Ctrl+L 查找聊天           Ctrl+F 搜索消息\n\n消息\n  Tab 切换焦点             ↑↓ 选择消息\n  PgUp / Home 更早历史      End 最新消息\n  r 回复  e 编辑  f 转发    Delete 删除（确认）\n  d 下载附件               v 查看完整消息\n\n输入\n  Enter 发送               Shift+Enter / Ctrl+Enter 换行\n  ←→ / Home / End 编辑      Ctrl+U 清除前文\n  支持中文、Emoji、多行粘贴；失败保留草稿\n\n  Ctrl+K 命令面板           F5 刷新 / 重连\n  Ctrl+B 聊天栏             Ctrl+Q 退出\n\nEsc 关闭帮助";
    f.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(block("快捷键", true)),
        rect,
    );
}
