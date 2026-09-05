use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::error::TgError;

// ── Config root ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TgConfig {
    #[serde(default)]
    pub telegram: TelegramConfig,
    #[serde(default)]
    pub tdlib: TdlibConfig,
    #[serde(default)]
    pub proxy: ProxyConfig,
    #[serde(default)]
    pub tui: TuiConfig,
    #[serde(default)]
    pub ipc: IpcConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramConfig {
    #[serde(default)]
    pub api_id: i32,
    #[serde(default)]
    pub api_hash: String,
    #[serde(default)]
    pub phone: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdlibConfig {
    #[serde(default = "default_tdlib_database_dir")]
    pub database_directory: PathBuf,
    #[serde(default = "default_tdlib_files_dir")]
    pub files_directory: PathBuf,
    #[serde(default = "default_true")]
    pub use_message_database: bool,
    #[serde(default)]
    pub use_secret_chats: bool,
    #[serde(default = "default_lang")]
    pub system_language_code: String,
    #[serde(default = "default_device")]
    pub device_model: String,
    #[serde(default)]
    pub application_version: String,
    #[serde(default)]
    pub verbosity: i32,
    #[serde(default)]
    pub test: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_proxy_kind")]
    pub kind: String,
    #[serde(default = "default_proxy_host")]
    pub host: String,
    #[serde(default = "default_proxy_port")]
    pub port: u16,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TuiConfig {
    #[serde(default = "default_true")]
    pub enable_mouse: bool,
    #[serde(default = "default_page_size")]
    pub message_page_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpcConfig {
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
}

// ── Defaults ───────────────────────────────────────────────────────

fn project_dirs() -> directories::ProjectDirs {
    directories::ProjectDirs::from("com", "telegram-cli", "tg")
        .expect("cannot determine home directory")
}

fn default_tdlib_database_dir() -> PathBuf {
    project_dirs().data_dir().join("tdlib").join("db")
}

fn default_tdlib_files_dir() -> PathBuf {
    project_dirs().data_dir().join("tdlib").join("files")
}

fn default_socket_path() -> PathBuf {
    #[cfg(windows)]
    return project_dirs()
        .data_local_dir()
        .join("runtime")
        .join("tgcd.sock");
    #[cfg(unix)]
    {
        let runtime = std::env::var("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_dirs().data_local_dir().join("runtime"));
        runtime.join("tg").join("tgcd.sock")
    }
}

fn default_true() -> bool {
    true
}
fn default_lang() -> String {
    "en".into()
}
fn default_device() -> String {
    "Telegram-TUI".into()
}
fn default_proxy_kind() -> String {
    "socks5".into()
}
fn default_proxy_host() -> String {
    "127.0.0.1".into()
}
fn default_proxy_port() -> u16 {
    1080
}
fn default_page_size() -> usize {
    50
}

impl Default for TdlibConfig {
    fn default() -> Self {
        Self {
            database_directory: default_tdlib_database_dir(),
            files_directory: default_tdlib_files_dir(),
            use_message_database: true,
            use_secret_chats: false,
            system_language_code: default_lang(),
            device_model: default_device(),
            application_version: env!("CARGO_PKG_VERSION").to_string(),
            verbosity: 0,
            test: false,
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            kind: default_proxy_kind(),
            host: default_proxy_host(),
            port: default_proxy_port(),
            username: String::new(),
            password: String::new(),
        }
    }
}

impl Default for TuiConfig {
    fn default() -> Self {
        Self {
            enable_mouse: true,
            message_page_size: default_page_size(),
        }
    }
}

impl Default for IpcConfig {
    fn default() -> Self {
        Self {
            socket_path: default_socket_path(),
        }
    }
}

// ── Load / Save ────────────────────────────────────────────────────

impl TgConfig {
    pub fn config_path() -> PathBuf {
        std::env::var("TG_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| project_dirs().config_dir().join("config.toml"))
    }

    pub fn load() -> Result<Self, TgError> {
        Self::load_from(&Self::config_path())
    }

    pub fn load_from(path: &std::path::Path) -> Result<Self, TgError> {
        if path.exists() {
            let content = std::fs::read_to_string(path)?;
            Ok(toml::from_str(&content)?)
        } else {
            Err(TgError::Config(format!(
                "Config not found at {}. Run `tg init`.",
                path.display()
            )))
        }
    }

    pub fn save(&self) -> Result<(), TgError> {
        self.save_to(&Self::config_path())
    }

    pub fn save_to(&self, path: &std::path::Path) -> Result<(), TgError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, toml::to_string_pretty(self)?)?;

        // Restrict config file to owner only (may contain API secrets)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        Ok(())
    }

    pub fn database_path(&self) -> PathBuf {
        self.tdlib.database_directory.join("tui-cache.db")
    }

    pub fn store_keyring(&self) -> Result<(), TgError> {
        if let Ok(entry) = keyring::Entry::new("tg-cli", "api_hash") {
            let _ = entry.set_password(&self.telegram.api_hash);
        }
        Ok(())
    }

    pub fn load_api_hash(&self) -> String {
        if !self.telegram.api_hash.is_empty() {
            return self.telegram.api_hash.clone();
        }
        if let Ok(entry) = keyring::Entry::new("tg-cli", "api_hash") {
            if let Ok(val) = entry.get_password() {
                return val;
            }
        }
        self.telegram.api_hash.clone()
    }

    /// Application identity is supplied by the distributor, separately from user login.
    /// Never mix the ID from one source with the hash from another source.
    pub fn application_credentials(&self) -> Result<(i32, String), TgError> {
        let runtime_id = std::env::var("TG_API_ID").ok();
        let runtime_hash = std::env::var("TG_API_HASH").ok();
        let (id, hash) = if runtime_id.is_some() || runtime_hash.is_some() {
            (
                runtime_id.and_then(|v| v.parse().ok()).unwrap_or(0),
                runtime_hash.unwrap_or_default(),
            )
        } else if self.telegram.api_id != 0 || !self.telegram.api_hash.is_empty() {
            (self.telegram.api_id, self.load_api_hash())
        } else {
            (
                option_env!("TG_APP_API_ID")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0),
                option_env!("TG_APP_API_HASH")
                    .unwrap_or_default()
                    .to_owned(),
            )
        };
        if id <= 0 || hash.len() != 32 || !hash.bytes().all(|c| c.is_ascii_hexdigit()) {
            return Err(TgError::Config("此构建未配置 Telegram 应用身份。维护者需设置 TG_APP_API_ID / TG_APP_API_HASH 后重新构建；源码开发可设置 TG_API_ID / TG_API_HASH。用户登录仍使用手机号和验证码。".into()));
        }
        Ok((id, hash))
    }
}
