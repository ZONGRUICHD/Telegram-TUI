use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "tg", about = "Telegram-TUI · 终端聊天与 AI CLI", version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,
    /// 输出单个 JSON 信封；管道默认启用
    #[arg(long, global = true, conflicts_with = "human")]
    pub json: bool,
    #[arg(long, global = true)]
    pub human: bool,
    #[arg(long,global=true,default_value_t=35,value_parser=clap::value_parser!(u64).range(1..=3600))]
    pub timeout: u64,
    /// 只输出计划发送的本地 RPC 请求
    #[arg(long, global = true)]
    pub dry_run: bool,
    /// CLI 在服务未运行时自动启动
    #[arg(long, global = true)]
    pub start_daemon: bool,
}

#[derive(Args)]
pub struct TextInput {
    /// 消息文本，也可用 --stdin 或 --file
    #[arg(num_args=0..,conflicts_with_all=["stdin","file"])]
    pub text: Vec<String>,
    #[arg(long, conflicts_with = "file")]
    pub stdin: bool,
    #[arg(long)]
    pub file: Option<PathBuf>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 创建默认配置（不询问应用 API）
    Init,
    /// 手机号、验证码、两步验证交互登录
    Login,
    /// 可编排的认证步骤；敏感值建议通过 --stdin
    Auth {
        #[arg(value_enum)]
        step: AuthStep,
        value: Option<String>,
        #[arg(long, conflicts_with = "value")]
        stdin: bool,
    },
    Me,
    /// 列出主列表、归档或指定文件夹
    Chats {
        #[arg(short,long,default_value_t=20,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long,default_value_t=0,value_parser=clap::value_parser!(u32).range(0..=9900))]
        offset: u32,
        #[arg(long, default_value = "main")]
        list: String,
    },
    Chat {
        #[arg(allow_hyphen_values = true)]
        chat: String,
    },
    Find {
        query: String,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    Contacts,
    Folders,
    History {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(short,long,default_value_t=50,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        before: i64,
    },
    Message {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        message_id: i64,
    },
    Send {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(long)]
        reply_to: Option<i64>,
        #[arg(long)]
        topic: Option<i64>,
        #[arg(long)]
        silent: bool,
        #[command(flatten)]
        input: TextInput,
    },
    Edit {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        message_id: i64,
        #[command(flatten)]
        input: TextInput,
    },
    SendFile {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        path: PathBuf,
        #[arg(long, default_value = "")]
        caption: String,
        #[arg(long)]
        photo: bool,
        #[arg(long)]
        reply_to: Option<i64>,
    },
    Search {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        query: String,
        #[arg(short,long,default_value_t=20,value_parser=clap::value_parser!(u32).range(1..=100))]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        before: i64,
    },
    Forward {
        #[arg(allow_hyphen_values = true)]
        from: String,
        #[arg(allow_hyphen_values = true)]
        to: String,
        message_id: i64,
    },
    Delete {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        message_id: i64,
        /// 同时撤回对方的消息副本
        #[arg(long)]
        revoke: bool,
    },
    Download {
        file_id: i64,
        /// 等待下载完成；默认只启动下载
        #[arg(long)]
        wait: bool,
    },
    File {
        file_id: i64,
    },
    /// 显式标记已读；默认使用聊天最后一条消息
    Read {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        message_ids: Vec<i64>,
    },
    Pin {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(long)]
        undo: bool,
        #[arg(long, default_value = "main")]
        list: String,
    },
    Archive {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(long)]
        undo: bool,
    },
    Mute {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(long, default_value_t = 3600)]
        seconds: u32,
    },
    Members {
        #[arg(allow_hyphen_values = true)]
        chat: String,
        #[arg(long, default_value_t = 50)]
        limit: u32,
        #[arg(long, default_value_t = 0)]
        offset: u32,
    },
    Topics {
        #[arg(allow_hyphen_values = true)]
        chat: String,
    },
    Join {
        target: String,
    },
    Leave {
        #[arg(allow_hyphen_values = true)]
        chat: String,
    },
    /// NDJSON 实时事件；掉线以非零状态结束，丢帧输出 resync_required
    Watch {
        #[arg(long, allow_hyphen_values = true)]
        chat: Option<i64>,
        #[arg(long)]
        count: Option<usize>,
    },
    /// 高级 TDLib 调用；params 为 JSON，或通过 --stdin 提供
    Api {
        method: String,
        #[arg(long, default_value = "{}")]
        params: String,
        #[arg(long)]
        stdin: bool,
    },
    Status,
    Logout,
    Stop,
    /// AI 调用说明与稳定的输出约定
    Schema,
    /// 检查配置、应用身份是否可用和服务连接（不泄漏凭据）
    Doctor,
    /// 启动终端界面
    Tui,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum AuthStep {
    Status,
    Phone,
    Code,
    Password,
    Email,
    EmailCode,
    Resend,
    Qr,
}
