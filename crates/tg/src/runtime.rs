use anyhow::{Context, Result};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};
use tg_core::config::TgConfig;
use tg_ipc::client::IpcClient;

pub async fn ensure_daemon(config: &TgConfig, path: &Path) -> Result<IpcClient> {
    if let Ok(client) = IpcClient::connect(&config.ipc.socket_path).await {
        return Ok(client);
    }
    config.application_credentials()?;
    let executable =
        std::env::current_exe()?.with_file_name(if cfg!(windows) { "tgcd.exe" } else { "tgcd" });
    let log_path = config.ipc.socket_path.with_extension("log");
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;
    let mut command = Command::new(executable);
    command
        .arg("--config")
        .arg(std::path::absolute(path)?)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(log);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command
        .spawn()
        .context("无法启动 tgcd，请确认 tg 和 tgcd 安装在同一目录")?;
    for _ in 0..100 {
        if let Ok(client) = IpcClient::connect(&config.ipc.socket_path).await {
            return Ok(client);
        }
        if let Some(status) = child.try_wait()? {
            anyhow::bail!("tgcd 启动失败（{status}）。详情：{}", log_path.display());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // Do not kill a potentially healthy but slow daemon; a second invocation can reconnect.
    tokio::spawn(async move {
        while child.try_wait().ok().flatten().is_none() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    });
    anyhow::bail!("tgcd 仍在启动。详情：{}", log_path.display())
}
