//! Owns one TDLib session and serves local CLI / TUI clients.
mod dispatcher;
mod handler;
mod ipc;
mod tdlib;
use anyhow::{Context, Result};
use clap::Parser;
use fs2::FileExt;
use tg_core::config::TgConfig;
use tokio::sync::{broadcast, watch};

#[derive(Parser)]
#[command(name = "tgcd", about = "Telegram-TUI 本地会话服务", version)]
struct Args {
    #[arg(short, long)]
    config: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args = Args::parse();
    let config = TgConfig::load_from(&args.config.unwrap_or_else(TgConfig::config_path))?;
    let (api_id, api_hash) = config.application_credentials()?;
    std::fs::create_dir_all(&config.tdlib.database_directory)?;
    std::fs::create_dir_all(&config.tdlib.files_directory)?;
    if let Some(parent) = config.ipc.socket_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let lock_path = config.ipc.socket_path.with_extension("lock");
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)?;
    lock.try_lock_exclusive()
        .context("此配置的 tgcd 已在运行")?;
    let db_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(config.tdlib.database_directory.join("tui.lock"))?;
    db_lock
        .try_lock_exclusive()
        .context("此 TDLib 会话正在被另一服务使用")?;
    tg_tdjson::load_library()?;
    tg_tdjson::set_log_verbosity(config.tdlib.verbosity);
    let (updates_tx, updates_rx) = broadcast::channel(4096);
    tg_tdjson::init(updates_tx.clone());
    let snapshot = std::sync::Arc::new(std::sync::RwLock::new(dispatcher::Snapshot::default()));
    let snapshot_task = tokio::spawn(dispatcher::run(updates_rx, snapshot.clone()));
    let td = tg_tdjson::TdClient::new();
    let auth = tdlib::query(&td, serde_json::json!({"@type":"getAuthorizationState"})).await?;
    if auth["@type"] == "authorizationStateWaitTdlibParameters" {
        tdlib::query(
            &td,
            serde_json::json!({
                "@type":"setTdlibParameters",
                "database_directory":config.tdlib.database_directory,
                "files_directory":config.tdlib.files_directory,
                "database_encryption_key":"",
                "use_file_database":true, "use_chat_info_database":true,
                "use_message_database":config.tdlib.use_message_database,
                "use_secret_chats":config.tdlib.use_secret_chats,
                "use_test_dc":config.tdlib.test,
                "api_id":api_id, "api_hash":api_hash,
                "system_language_code":config.tdlib.system_language_code,
                "device_model":config.tdlib.device_model,
                "system_version":std::env::consts::OS,
                "application_version":env!("CARGO_PKG_VERSION")
            }),
        )
        .await?;
    }
    if config.proxy.enabled {
        let kind = match config.proxy.kind.as_str() {
            "socks5" => {
                serde_json::json!({"@type":"proxyTypeSocks5","username":config.proxy.username,"password":config.proxy.password})
            }
            "http" => {
                serde_json::json!({"@type":"proxyTypeHttp","username":config.proxy.username,"password":config.proxy.password,"http_only":false})
            }
            "mtproto" => {
                serde_json::json!({"@type":"proxyTypeMtproto","secret":config.proxy.password})
            }
            other => anyhow::bail!("不支持的代理类型：{other}"),
        };
        tdlib::query(&td, serde_json::json!({"@type":"addProxy",
            "proxy":{"@type":"proxy","server":config.proxy.host,"port":config.proxy.port,"type":kind},
            "enable":true,"comment":"Telegram-TUI"})).await?;
    }
    let (shutdown_tx, _) = watch::channel(false);
    let state = handler::AppState {
        config: config.clone(),
        td: td.clone(),
        updates_tx,
        shutdown_tx,
        snapshot,
    };
    let result = ipc::run(&config.ipc.socket_path, state).await;
    let _ = tdlib::query(&td, serde_json::json!({"@type":"close"})).await;
    snapshot_task.abort();
    drop(db_lock);
    drop(lock);
    result
}
