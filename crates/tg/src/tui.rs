//! Responsive terminal client with isolated state, rendering and transport.
mod app;
mod demo;
mod editor;
#[cfg(test)]
mod tests;
mod view;
use anyhow::Result;
use app::{App, Focus, Job};
use crossterm::{
    event::{
        DisableBracketedPaste, DisableFocusChange, DisableMouseCapture, EnableBracketedPaste,
        EnableFocusChange, EnableMouseCapture, Event, EventStream, KeyEventKind, MouseButton,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_util::StreamExt;
use ratatui::{
    backend::{CrosstermBackend, TestBackend},
    Terminal,
};
use serde_json::json;
use std::{
    io::{self, IsTerminal},
    path::Path,
    time::{Duration, Instant},
};
use tg_core::config::TgConfig;
use tg_ipc::client::IpcClient;

struct TerminalGuard;
impl TerminalGuard {
    fn enter(mouse: bool) -> Result<Self> {
        enable_raw_mode()?;
        let guard = Self;
        execute!(
            io::stdout(),
            EnterAlternateScreen,
            EnableBracketedPaste,
            EnableFocusChange
        )?;
        if mouse {
            execute!(io::stdout(), EnableMouseCapture)?;
        }
        Ok(guard)
    }
}
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            DisableFocusChange,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
    }
}

pub fn snapshot(path: &Path, width: u16, height: u16) -> Result<()> {
    let mut app = demo::Demo::new().app();
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    terminal.draw(|f| view::draw(f, &mut app))?;
    let buffer = terminal.backend().buffer();
    if path.extension().is_some_and(|ext| ext == "svg") {
        let mut svg = format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}"><rect width="100%" height="100%" fill="#121419"/><g font-family="Cascadia Mono,Consolas,Microsoft YaHei,monospace" font-size="16">"##,
            width as usize * 10 + 32,
            height as usize * 22 + 32,
            width as usize * 10 + 32,
            height as usize * 22 + 32
        );
        for y in 0..height {
            let mut x = 0;
            while x < width {
                let cell = &buffer[(x, y)];
                let symbol = cell.symbol();
                let cells = unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
                let color = |color, fallback| match color {
                    ratatui::style::Color::Rgb(r, g, b) => format!("#{r:02x}{g:02x}{b:02x}"),
                    _ => fallback,
                };
                let bg = color(cell.bg, "#121419".to_owned());
                let fg = color(cell.fg, "#e1e4ea".to_owned());
                let symbol = symbol
                    .replace('&', "&amp;")
                    .replace('<', "&lt;")
                    .replace('>', "&gt;");
                let px = x as usize * 10 + 16;
                let py = y as usize * 22 + 16;
                let advance = cells as usize * 10;
                svg.push_str(&format!(r#"<rect x="{px}" y="{py}" width="{advance}" height="22" fill="{bg}"/><text x="{px}" y="{}" fill="{fg}" textLength="{advance}" lengthAdjust="spacingAndGlyphs">{symbol}</text>"#,py+17));
                x += cells;
            }
        }
        svg.push_str("</g></svg>");
        std::fs::write(path, svg)?;
        return Ok(());
    }
    let mut text = String::new();
    for y in 0..height {
        let mut x = 0;
        while x < width {
            let symbol = buffer[(x, y)].symbol();
            text.push_str(symbol);
            x += unicode_width::UnicodeWidthStr::width(symbol).max(1) as u16;
        }
        text.push('\n');
    }
    std::fs::write(path, text)?;
    Ok(())
}

pub async fn run(config: TgConfig, is_demo: bool) -> Result<()> {
    anyhow::ensure!(
        io::stdin().is_terminal() && io::stdout().is_terminal(),
        "TUI 需要交互终端"
    );
    let mut fixture = demo::Demo::new();
    let mut app = if is_demo {
        fixture.app()
    } else {
        App::new(config.tui.message_page_size, false)
    };
    let draft_path = config.tdlib.database_directory.join("tui-drafts.json");
    if !is_demo {
        if let Ok(content) = std::fs::read_to_string(&draft_path) {
            app.drafts = serde_json::from_str(&content).unwrap_or_default();
        }
    }
    let client = if is_demo {
        None
    } else {
        Some(IpcClient::connect(&config.ipc.socket_path).await?)
    };
    let (mut writer, mut reader) = match client {
        Some(client) => {
            let (w, r) = client.split();
            (Some(w), Some(r))
        }
        None => (None, None),
    };
    let _guard = TerminalGuard::enter(config.tui.enable_mouse)?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            DisableMouseCapture,
            DisableBracketedPaste,
            DisableFocusChange,
            LeaveAlternateScreen,
            crossterm::cursor::Show
        );
        previous(info);
    }));
    if !is_demo {
        app.bootstrap();
    }
    let mut events = EventStream::new();
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let mut reconnect_after = Instant::now();
    loop {
        // Requests remain ordered; rendering and update handling never await TDLib responses.
        while let Some(request) = app.outbox.pop_front() {
            if is_demo {
                app.handle(fixture.respond(request));
            } else if let Some(writer) = &mut writer {
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(2), writer.send_request(&request))
                        .await,
                    Ok(Ok(()))
                ) {
                    app.disconnect();
                    break;
                }
            }
        }
        terminal.draw(|f| view::draw(f, &mut app))?;
        if app.quit {
            break;
        }
        tokio::select! {
            event=events.next()=>match event {
                Some(Ok(Event::Key(key))) if key.kind==KeyEventKind::Press || key.kind==KeyEventKind::Repeat=>app.key(key),
                Some(Ok(Event::Paste(text)))=>{
                    match app.focus {
                        Focus::Composer=>{app.editor.insert(&text);app.save_draft();}
                        Focus::ChatSearch|Focus::MessageSearch=>app.search.insert(&text.replace(['\r','\n'],"")),
                        Focus::Palette=>app.palette.insert(&text.replace(['\r','\n'],"")),
                        Focus::Login=>app.auth_input.insert(text.trim_end_matches(['\r','\n'])),
                        _=>{}
                    }
                }
                Some(Ok(Event::Mouse(mouse))) if config.tui.enable_mouse && app.ready()=>{
                    let position=ratatui::layout::Position::new(mouse.column,mouse.row);
                    if app.help || app.info.is_some() || app.confirm.is_some() || app.focus==Focus::Palette {continue;}
                    match mouse.kind {
                        MouseEventKind::Down(MouseButton::Left)=>{
                            if app.hit.search.contains(position) {app.focus=Focus::ChatSearch;app.search.set(app.chat_query.clone());}
                            else if app.hit.composer.contains(position) {app.focus=Focus::Composer;app.mark_visible_read();}
                            else if let Some((_,index))=app.hit.chat_rows.iter().find(|(r,_)|r.contains(position)).copied() {app.select_chat(index);app.focus=Focus::Composer;}
                            else if let Some((_,index))=app.hit.message_rows.iter().find(|(r,_)|r.contains(position)).copied() {app.message_state.select(Some(index));app.focus=Focus::Messages;}
                        }
                        MouseEventKind::ScrollUp|MouseEventKind::ScrollDown=>{
                            let code=if mouse.kind==MouseEventKind::ScrollUp {crossterm::event::KeyCode::Up}else{crossterm::event::KeyCode::Down};
                            app.focus=if app.hit.chats.contains(position){Focus::Chats}else{Focus::Messages};
                            app.key(crossterm::event::KeyEvent::new(code,crossterm::event::KeyModifiers::NONE));
                            if app.focus==Focus::Messages && app.message_state.selected()==Some(0) && mouse.kind==MouseEventKind::ScrollUp {
                                let before=app.messages.first().and_then(|m|m["id"].as_i64()).unwrap_or(0);app.load_history(before);
                            }
                        }
                        _=>{}
                    }
                }
                Some(Ok(Event::FocusGained))=>{app.window_focused=true;app.mark_visible_read();}
                Some(Ok(Event::FocusLost))=>app.window_focused=false,
                Some(Err(error))=>return Err(error.into()),
                None=>{app.save_draft();break;}
                _=>{}
            },
            message=async {
                match &mut reader {Some(reader)=>reader.read_message().await,None=>std::future::pending().await}
            },if !is_demo && app.connected=>{
                match message {
                    Ok(message)=>app.handle(message),
                    Err(_)=>{app.disconnect();writer=None;reader=None;reconnect_after=Instant::now()+Duration::from_secs(2);}
                }
            }
            _=tick.tick()=>{
                app.tick();
                if !is_demo && app.draft_dirty && app.last_draft_save.elapsed()>Duration::from_secs(1) {
                    save_drafts(&app,&draft_path)?;app.draft_dirty=false;app.last_draft_save=Instant::now();
                    if app.edit.is_none() {
                        if let Some(chat)=app.active {
                            app.request("draft",json!({"chat_id":chat,"text":app.editor.text,"topic":app.topic}),Job::Draft);
                        }
                    }
                }
                if !is_demo && !app.connected && (app.reconnect || Instant::now()>=reconnect_after) {
                    app.reconnect=false;
                    if let Ok(client)=IpcClient::connect(&config.ipc.socket_path).await {
                        let(w,r)=client.split();writer=Some(w);reader=Some(r);app.connected=true;
                        app.generation+=1;app.bootstrap();app.refresh_chats();app.load_history(0);
                    }
                    reconnect_after=Instant::now()+Duration::from_secs(5);
                }
            }
        }
    }
    app.save_draft();
    if !is_demo {
        save_drafts(&app, &draft_path)?;
    }
    Ok(())
}
fn save_drafts(app: &App, path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec(&app.drafts)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(temp, path)?;
    Ok(())
}
