//! `tg` — Telegram CLI / TUI frontend.
//!
//! Communicates with `tgcd` over a length-delimited Unix socket.

use anyhow::Result;
use clap::{Parser, Subcommand};
use serde_json::json;

use tg_core::config::TgConfig;
use tg_ipc::client::IpcClient;
use tg_ipc::protocol::methods;

mod init;
mod login;
mod output;
mod runtime;
mod tui;

#[derive(Parser)]
#[command(
    name = "tg",
    about = "Telegram CLI — manage chats from your terminal",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, global = true)]
    config: Option<String>,
}

#[derive(Subcommand)]
enum Commands {
    /// Create config file
    Init,
    /// Interactive login
    Login,
    /// Show account info
    Me,
    /// List chats
    Chats {
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },
    /// Show messages in a chat
    History {
        chat: String,
        #[arg(short, long, default_value = "50")]
        limit: i64,
    },
    /// Send a message
    Send {
        chat: String,
        #[arg(required = true, trailing_var_arg = true)]
        text: Vec<String>,
    },
    /// Search messages
    Search {
        chat: String,
        query: String,
        #[arg(short, long, default_value = "20")]
        limit: i64,
    },
    /// Forward a message
    Forward {
        from: String,
        to: String,
        msg_id: i64,
    },
    /// Delete a message
    Delete { chat: String, msg_id: i64 },
    /// Download a file
    Download { file_id: i64 },
    /// Mark chat as read
    Read { chat: String },
    /// Daemon status
    Status,
    /// Log out
    Logout,
    /// Stop daemon
    Stop,
    /// Launch TUI
    Tui,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("warn").init();

    let cli = Cli::parse();
    let path = cli
        .config
        .as_ref()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(TgConfig::config_path);

    match &cli.command {
        Commands::Init => {
            init::run(&path)?;
            return Ok(());
        }
        Commands::Login => {
            login::run(&path).await?;
            return Ok(());
        }
        Commands::Tui => {
            tui::run(TgConfig::load_from(&path)?).await?;
            return Ok(());
        }
        _ => {}
    }

    let config = TgConfig::load_from(&path)?;
    let socket = &config.ipc.socket_path;

    let mut client = IpcClient::connect(socket).await?;

    let (method, params) = match &cli.command {
        Commands::Me => (methods::GET_ME, json!({})),
        Commands::Chats { limit } => (methods::LIST_DIALOGS, json!({ "limit": limit })),
        Commands::History { chat, limit } => (
            methods::GET_MESSAGES,
            json!({ "chat_id": parse_chat(chat), "limit": limit }),
        ),
        Commands::Send { chat, text } => (
            methods::SEND_MESSAGE,
            json!({ "chat_id": parse_chat(chat), "text": text.join(" ") }),
        ),
        Commands::Search { chat, query, limit } => (
            methods::SEARCH,
            json!({ "chat_id": parse_chat(chat), "query": query, "limit": limit }),
        ),
        Commands::Forward { from, to, msg_id } => (
            methods::FORWARD_MESSAGE,
            json!({ "from_chat_id": parse_chat(from), "to_chat_id": parse_chat(to), "message_id": msg_id }),
        ),
        Commands::Delete { chat, msg_id } => (
            methods::DELETE_MESSAGE,
            json!({ "chat_id": parse_chat(chat), "message_id": msg_id }),
        ),
        Commands::Download { file_id } => (methods::DOWNLOAD_FILE, json!({ "file_id": file_id })),
        Commands::Read { chat } => (methods::MARK_READ, json!({ "chat_id": parse_chat(chat) })),
        Commands::Status => (methods::GET_STATUS, json!({})),
        Commands::Logout => (methods::LOGOUT, json!({})),
        Commands::Stop => {
            let pid_path = config.ipc.socket_path.with_extension("pid");
            if pid_path.exists() {
                if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                    let pid = pid_str.trim();
                    std::process::Command::new("kill").arg(pid).status()?;
                    println!("✅  Sent SIGTERM to tgcd (pid {pid})");
                    return Ok(());
                }
            }
            anyhow::bail!("Cannot find tgcd PID file. Stop manually: kill $(pgrep tgcd)");
        }
        Commands::Init | Commands::Login | Commands::Tui => unreachable!(),
    };

    match client.call(method, params).await {
        Ok(val) => output::print_result(&cli.command, &val),
        Err(e) => eprintln!("❌  {e}"),
    }

    Ok(())
}

fn parse_chat(s: &str) -> serde_json::Value {
    if let Ok(id) = s.parse::<i64>() {
        json!(id)
    } else {
        json!(s)
    }
}
