//! CLI entry point. stdout is a machine protocol when piped.
mod commands;
mod init;
mod login;
mod output;
mod runtime;
mod tui;
use anyhow::{Context, Result};
use clap::Parser;
use commands::{AuthStep, Cli, Commands, TextInput};
use serde_json::{json, Value};
use std::{
    io::{IsTerminal, Read},
    time::Duration,
};
use tg_core::config::TgConfig;
use tg_ipc::{
    client::IpcClient,
    protocol::{RpcError, ServerMessage},
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .init();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if error.use_stderr()
                && (std::env::args().any(|a| a == "--json") || !std::io::stdout().is_terminal())
            {
                let _ = output::json(
                    &json!({"schema_version":1,"ok":false,"error":{"kind":"usage","code":2,"message":error.to_string()}}),
                );
            } else {
                let _ = error.print();
            }
            std::process::exit(error.exit_code());
        }
    };
    let machine = cli.json || (!cli.human && !std::io::stdout().is_terminal());
    if let Err(error) = run(&cli, machine).await {
        let rpc = error.downcast_ref::<RpcError>();
        let timeout = error
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.kind() == std::io::ErrorKind::TimedOut);
        let (kind, exit) = if timeout || rpc.is_some_and(|r| r.code == -32001) {
            ("timeout", 4)
        } else if rpc.is_some_and(|r| r.code == 401) {
            ("authentication", 3)
        } else if rpc.is_some() {
            ("rpc", 1)
        } else {
            ("runtime", 1)
        };
        if machine {
            let _ = output::json(&json!({"schema_version":1,"ok":false,"error":{
                "kind":kind,"code":rpc.map_or(-1,|r|r.code),"message":error.to_string()}}));
        } else {
            eprintln!("{}", output::safe(&error.to_string()));
        }
        std::process::exit(exit);
    }
}

async fn run(cli: &Cli, machine: bool) -> Result<()> {
    let path = cli.config.clone().unwrap_or_else(TgConfig::config_path);
    let default = Commands::Tui {
        demo: false,
        snapshot: None,
    };
    let cmd = cli.command.as_ref().unwrap_or(&default);
    match cmd {
        Commands::Schema => return output::success(&schema(), true),
        Commands::Init => {
            if cli.dry_run {
                return output::success(&json!({"would_create":path}), machine);
            }
            init::run(&path)?;
            return output::success(&json!({"config":path}), machine);
        }
        Commands::Login => {
            anyhow::ensure!(!cli.dry_run, "login 是交互操作，不支持 --dry-run");
            login::run(&path).await?;
            return output::success(&json!({"authorized":true}), machine);
        }
        Commands::Tui { demo, snapshot } => {
            anyhow::ensure!(!cli.dry_run, "tui 不支持 --dry-run");
            if let Some(snapshot) = snapshot {
                return tui::snapshot(snapshot, 120, 36);
            }
            if *demo {
                return tui::run(TgConfig::default(), true).await;
            }
            anyhow::ensure!(
                std::io::stdin().is_terminal() && std::io::stdout().is_terminal(),
                "TUI 需要终端；AI 请使用 tg schema 查询 CLI"
            );
            if !path.exists() {
                TgConfig::default().save_to(&path)?;
            }
            let config = TgConfig::load_from(&path)?;
            runtime::ensure_daemon(&config, &path).await?;
            return tui::run(config, false).await;
        }
        Commands::Doctor => {
            let config = TgConfig::load_from(&path)?;
            let available = config.application_credentials().is_ok();
            let daemon = IpcClient::connect(&config.ipc.socket_path).await.is_ok();
            let executable = std::env::current_exe()?.with_file_name(if cfg!(windows) {
                "tgcd.exe"
            } else {
                "tgcd"
            });
            let library=tokio::task::spawn_blocking(move || {
                let mut command=std::process::Command::new(executable);
                command.arg("--check-library");
                #[cfg(windows)] {use std::os::windows::process::CommandExt;command.creation_flags(0x08000000);}
                match command.output() {
                    Ok(output) if output.status.success()=>serde_json::from_slice::<Value>(&output.stdout).unwrap_or(json!({"available":false})),
                    Ok(output)=>json!({"available":false,"diagnostic":String::from_utf8_lossy(&output.stderr)}),
                    Err(error)=>json!({"available":false,"diagnostic":error.to_string()}),
                }
            }).await?;
            return output::success(
                &json!({"config":path,"application_identity_available":available,"tdlib":library,
                "daemon_connected":daemon,"endpoint":config.ipc.socket_path,
                "library_override_set":std::env::var_os("LIBTDJSON_PATH").is_some()}),
                machine,
            );
        }
        _ => {}
    }
    let operation = if matches!(cmd, Commands::Watch { .. }) {
        None
    } else {
        Some(operation(cmd)?)
    };
    if cli.dry_run {
        let (method, params) = operation.context("watch 不支持 --dry-run")?;
        return output::success(
            &json!({"dry_run":true,"method":method,"params":params}),
            machine,
        );
    }
    let config = TgConfig::load_from(&path)?;
    let mut client = if cli.start_daemon {
        runtime::ensure_daemon(&config, &path).await?
    } else {
        IpcClient::connect(&config.ipc.socket_path)
            .await
            .context("无法连接 tgcd；运行 tg login 或增加 --start-daemon")?
    };
    if let Commands::Watch { chat, count } = cmd {
        let end = tokio::time::Instant::now() + Duration::from_secs(cli.timeout);
        let mut seen = 0;
        if *count == Some(0) {
            return Ok(());
        }
        loop {
            let message = tokio::select! {
                _ = tokio::signal::ctrl_c() => return Ok(()),
                result = tokio::time::timeout_at(end,client.read_message()) => match result {
                    Ok(result) => result?,
                    Err(_) => return Ok(()),
                }
            };
            if let ServerMessage::Event(event) = message {
                let id = event.data["chat_id"]
                    .as_i64()
                    .or_else(|| event.data["message"]["chat_id"].as_i64());
                if chat.is_some() && id.is_some() && *chat != id {
                    continue;
                }
                output::json(
                    &json!({"schema_version":1,"type":"event","name":event.name,"data":event.data}),
                )?;
                seen += 1;
                if count.is_some_and(|n| seen >= n) {
                    return Ok(());
                }
            }
        }
    }
    let (method, params) = operation.unwrap();
    let result = tokio::time::timeout(Duration::from_secs(cli.timeout), async {
        let mut result = client
            .call_with_timeout(&method, params, Duration::from_secs(cli.timeout))
            .await?;
        if let Commands::Download {
            file_id,
            wait: true,
        } = cmd
        {
            while !result["local"]["is_downloading_completed"]
                .as_bool()
                .unwrap_or(false)
            {
                if !result["local"]["is_downloading_active"]
                    .as_bool()
                    .unwrap_or(false)
                {
                    anyhow::bail!("下载已停止：请检查文件状态或重新下载");
                }
                tokio::time::sleep(Duration::from_millis(250)).await;
                result = client.call("get_file", json!({"file_id":file_id})).await?;
            }
        }
        Ok::<_, anyhow::Error>(result)
    })
    .await
    .map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::TimedOut,
            "请求超时，结果未知；发送操作请先核对聊天历史，勿自动重试",
        )
    })??;
    output::success(&result, machine)
}

fn operation(cmd: &Commands) -> Result<(String, Value)> {
    let (method, params) = match cmd {
        Commands::Me => ("get_me", json!({})),
        Commands::Status => ("status", json!({})),
        Commands::Stop => ("shutdown", json!({})),
        Commands::Logout => ("logout", json!({})),
        Commands::Chats {
            limit,
            offset,
            list,
        } => (
            "list_dialogs",
            json!({"limit":limit,"offset":offset,"list":list}),
        ),
        Commands::Chat { chat } => ("get_chat", json!({"chat_id":chat})),
        Commands::Find { query, limit } => ("find_chats", json!({"query":query,"limit":limit})),
        Commands::Contacts => ("contacts", json!({})),
        Commands::Folders => ("folders", json!({})),
        Commands::History {
            chat,
            limit,
            before,
        } => {
            anyhow::ensure!(*before >= 0, "--before 必须大于等于 0");
            (
                "get_messages",
                json!({"chat_id":chat,"limit":limit,"from_message_id":before}),
            )
        }
        Commands::Message { chat, message_id } => (
            "get_message",
            json!({"chat_id":chat,"message_id":message_id}),
        ),
        Commands::Send {
            chat,
            reply_to,
            topic,
            silent,
            input,
        } => (
            "send_message",
            json!({"chat_id":chat,"text":read_text(input)?,"reply_to":reply_to,"topic":topic,"silent":silent}),
        ),
        Commands::Edit {
            chat,
            message_id,
            input,
        } => (
            "edit_message",
            json!({"chat_id":chat,"message_id":message_id,"text":read_text(input)?}),
        ),
        Commands::SendFile {
            chat,
            path,
            caption,
            photo,
            reply_to,
        } => (
            "send_file",
            json!({"chat_id":chat,
            "path":std::fs::canonicalize(path).context("找不到附件")?,"caption":caption,"photo":photo,"reply_to":reply_to}),
        ),
        Commands::Search {
            chat,
            query,
            limit,
            before,
        } => (
            "search",
            json!({"chat_id":chat,"query":query,"limit":limit,"from_message_id":before}),
        ),
        Commands::Forward {
            from,
            to,
            message_id,
        } => (
            "forward_message",
            json!({"from_chat_id":from,"to_chat_id":to,"message_id":message_id}),
        ),
        Commands::Delete {
            chat,
            message_id,
            revoke,
        } => (
            "delete_message",
            json!({"chat_id":chat,"message_id":message_id,"revoke":revoke}),
        ),
        Commands::Download { file_id, .. } => ("download_file", json!({"file_id":file_id})),
        Commands::File { file_id } => ("get_file", json!({"file_id":file_id})),
        Commands::Read { chat, message_ids } => (
            "mark_read",
            json!({"chat_id":chat,"message_ids":message_ids}),
        ),
        Commands::Pin { chat, undo, list } => {
            ("pin", json!({"chat_id":chat,"pinned":!undo,"list":list}))
        }
        Commands::Archive { chat, undo } => ("archive", json!({"chat_id":chat,"archived":!undo})),
        Commands::Mute { chat, seconds } => ("mute", json!({"chat_id":chat,"seconds":seconds})),
        Commands::Members {
            chat,
            limit,
            offset,
        } => (
            "members",
            json!({"chat_id":chat,"limit":limit,"offset":offset}),
        ),
        Commands::Topics { chat } => ("topics", json!({"chat_id":chat})),
        Commands::Join { target } => (
            "join",
            if target.starts_with("https://t.me/+") || target.starts_with("https://t.me/joinchat/")
            {
                json!({"invite_link":target})
            } else {
                json!({"chat_id":target})
            },
        ),
        Commands::Leave { chat } => ("leave", json!({"chat_id":chat})),
        Commands::Auth { step, value, stdin } => {
            let method = match step {
                AuthStep::Status => "auth_trigger",
                AuthStep::Phone => "auth_phone",
                AuthStep::Code => "auth_code",
                AuthStep::Password => "auth_password",
                AuthStep::Email => "auth_email",
                AuthStep::EmailCode => "auth_email_code",
                AuthStep::Resend => "auth_resend",
                AuthStep::Qr => "auth_qr",
            };
            let key = match step {
                AuthStep::Phone => Some("phone"),
                AuthStep::Code | AuthStep::EmailCode => Some("code"),
                AuthStep::Password => Some("password"),
                AuthStep::Email => Some("email"),
                _ => None,
            };
            let p = if let Some(key) = key {
                let v = if *stdin {
                    read_stdin()?.trim_end_matches(['\r', '\n']).to_owned()
                } else {
                    value.clone().context("缺少认证值；使用 --stdin 传入")?
                };
                json!({key:v})
            } else {
                json!({})
            };
            (method, p)
        }
        Commands::Api {
            method,
            params,
            stdin,
        } => {
            let mut q: Value = serde_json::from_str(&if *stdin {
                read_stdin()?
            } else {
                params.clone()
            })?;
            anyhow::ensure!(q.is_object(), "params 必须是 JSON 对象");
            q["@type"] = json!(method);
            ("api", json!({"query":q}))
        }
        _ => anyhow::bail!("此命令不对应单次 RPC"),
    };
    Ok((method.to_owned(), params))
}

fn read_stdin() -> Result<String> {
    anyhow::ensure!(!std::io::stdin().is_terminal(), "--stdin 需要管道输入");
    let mut input = String::new();
    std::io::stdin()
        .take(1_048_577)
        .read_to_string(&mut input)?;
    anyhow::ensure!(input.len() <= 1_048_576, "输入超过 1 MiB");
    Ok(input)
}
fn read_text(input: &TextInput) -> Result<String> {
    let text = if input.stdin {
        read_stdin()?
    } else if let Some(path) = &input.file {
        let mut text = String::new();
        std::fs::File::open(path)?
            .take(1_048_577)
            .read_to_string(&mut text)?;
        anyhow::ensure!(text.len() <= 1_048_576, "文本文件超过 1 MiB");
        text
    } else {
        input.text.join(" ")
    };
    anyhow::ensure!(!text.trim().is_empty(), "消息文本不能为空");
    anyhow::ensure!(
        text.encode_utf16().count() <= 4096,
        "消息文本超过 4096 个 UTF-16 单元"
    );
    Ok(text)
}
fn schema() -> Value {
    json!({
        "name":"Telegram-TUI","cli":"tg","schema_version":1,
        "success":{"schema_version":1,"ok":true,"result":"TDLib object or paginated collection"},
        "failure":{"schema_version":1,"ok":false,"error":{"kind":"rpc|runtime|authentication|timeout|usage","code":"Telegram code or local code","message":"string"}},
        "exit_codes":{"0":"成功","1":"操作失败","2":"命令行参数错误","3":"认证失败 401","4":"超时，操作结果未知"},
        "chat_reference":["数字聊天 ID（包括负数）","@username","me"],
        "pagination":{"history":"result.next_from_message_id → --before；null 表示本次未返回更早消息","chats":"result.next_offset → --offset"},
        "read_receipts":"history/search/watch 不发送已读回执；仅 read 显式标记",
        "send_semantics":"成功响应表示 TDLib 已受理；检查 sending_state 并监听 updateMessageSendSucceeded/updateMessageSendFailed，超时不得盲目重试",
        "events":"tg watch 输出 NDJSON；--timeout 是流的总时长，默认 35 秒；resync_required 需要重新读取状态",
        "examples":["tg --json chats --limit 30","tg --json history -100123 --before 456","tg --json send me --reply-to 123 --stdin",
            "tg --json search @username 关键词","tg --json send-file me ./report.pdf","tg watch --chat -100123 --timeout 300",
            "tg --json auth code --stdin","tg --json api getUser --params '{\"user_id\":123}'"],
        "commands":Cli::command_names()
    })
}
impl Cli {
    fn command_names() -> Vec<String> {
        use clap::CommandFactory;
        Self::command()
            .get_subcommands()
            .map(|c| c.get_name().to_owned())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn negative_chat_and_global_flags() {
        let cli = Cli::try_parse_from([
            "tg", "--json", "history", "-100123", "--before", "999", "--limit", "100",
        ])
        .unwrap();
        assert!(cli.json);
        let (method, p) = operation(cli.command.as_ref().unwrap()).unwrap();
        assert_eq!(method, "get_messages");
        assert_eq!(p["chat_id"], "-100123");
        assert_eq!(p["from_message_id"], 999);
    }
    #[test]
    fn empty_send_and_conflicting_sources_fail() {
        let cli = Cli::try_parse_from(["tg", "send", "me"]).unwrap();
        assert!(operation(cli.command.as_ref().unwrap()).is_err());
        assert!(Cli::try_parse_from(["tg", "send", "me", "--stdin", "hello"]).is_err());
        assert!(Cli::try_parse_from(["tg", "history", "1", "--limit", "101"]).is_err());
    }
    #[test]
    fn authentication_values_use_field_names() {
        let cli = Cli::try_parse_from(["tg", "auth", "code", "12345"]).unwrap();
        assert_eq!(
            operation(cli.command.as_ref().unwrap()).unwrap().1,
            json!({"code":"12345"})
        );
    }
}
